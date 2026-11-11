import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/generated.dart/api/chat_runs.dart' as runs;
import 'package:flutter_test/flutter_test.dart';

import 'chat_run_bridge_test.dart' show waitForRun;
import 'support/isolated_bridge.dart';

void main() {
  test(
    'main/wiki answers receive complete short evidence and bind ordinary links',
    () async {
      final repo = await createIsolatedBridge();
      final body =
          '装配手册的过程说明。${List.filled(90, '逐步校准。').join()}'
          '结尾要求：17 N·m；不对公网发布。';
      final page = await repo.saveTextPage(text: body, title: '装配手册', tags: []);
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));
      final requests = <Map<String, dynamic>>[];
      final answer =
          '记录要求17 N·m且不对公网发布。[[${page.slug}|装配手册]] '
          '另一个导航 [[method/not-provided]]';
      server.listen((request) async {
        requests.add(
          jsonDecode(await utf8.decoder.bind(request).join())
              as Map<String, dynamic>,
        );
        request.response.headers.contentType = ContentType(
          'text',
          'event-stream',
          charset: 'utf-8',
        );
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {
                'index': 0,
                'delta': {'content': answer},
                'finish_reason': 'stop',
              },
            ],
          })}\n\ndata: [DONE]\n\n',
        );
        await request.response.close();
      });
      await api.updateAiProviderConfig(
        baseUrl: 'http://127.0.0.1:${server.port}/v1',
        model: 'gpt-4o',
        apiKey: 'synthetic',
      );
      final main = await repo.createConversation(title: 'evidence main');
      final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
      for (final conversation in [main.id, wiki.id]) {
        final input = await repo.sendMessage(
          conversation,
          'user',
          '装配手册最后有哪些硬性要求？',
        );
        final run = await runs.startChatRun(
          conversationId: conversation,
          inputMessageId: input.id,
          idempotencyKey: 'evidence-$conversation',
        );
        final finished = await waitForRun(
          conversation,
          run.id,
          (r) => r.status == 'completed' || r.status == 'failed',
        );
        expect(finished.status, 'completed', reason: finished.errorText);
        expect(finished.content, answer);
        final citations = await api.getMessageKnowledgeCitations(
          messageId: finished.finalMessageId!,
        );
        expect(citations, hasLength(1));
        expect(citations.single.pageSlug, page.slug);
        expect(citations.single.excerpt, body);
        final messages = requests.last['messages'] as List;
        final mounted = messages
            .where((m) => (m['content'] as String? ?? '').startsWith('知识选材：\n'))
            .map(
              (m) => jsonDecode(
                (m['content'] as String).substring('知识选材：\n'.length),
              ) as Map,
            )
            .single;
        expect(mounted['knowledge_reading'][page.slug]['coverage'], 'full');
        expect(
          (mounted['knowledge_candidates'] as List).single['excerpt'],
          body,
        );
        final tools = requests.last['tools'] as List;
        final reader = tools.singleWhere(
          (t) => t['function']['name'] == 'get_wiki_page',
        );
        final properties =
            reader['function']['parameters']['properties'] as Map;
        expect(
          properties.keys,
          containsAll(['slug', 'offset', 'content_hash']),
        );
      }
      expect(requests, hasLength(2));
    },
  );
}
