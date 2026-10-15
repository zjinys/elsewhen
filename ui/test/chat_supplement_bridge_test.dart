import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';
import 'chat_run_bridge_test.dart' show waitForRun;

void main() {
  test(
    'both entrances inject explicit supplements only after a real tool round',
    () async {
      final repo = await createIsolatedBridge();
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      var release = Completer<void>();
      var calls = 0;
      final payloads = <Map<String, dynamic>>[];
      addTearDown(() async {
        if (!release.isCompleted) release.complete();
        await server.close(force: true);
      });
      server.listen((request) async {
        final payload = jsonDecode(
          await utf8.decoder.bind(request).join(),
        ) as Map<String, dynamic>;
        payloads.add(payload);
        final round = ++calls;
        request.response.headers.contentType = ContentType(
          'text',
          'event-stream',
          charset: 'utf-8',
        );
        request.response.bufferOutput = false;
        if (round == 1) {
          await release.future;
          request.response.write(
            'data: ${jsonEncode({
              'choices': [
                {
                  'index': 0,
                  'delta': {
                    'tool_calls': [
                      {
                        'index': 0,
                        'id': 'rule-read',
                        'type': 'function',
                        'function': {'name': 'list_rules', 'arguments': '{}'},
                      },
                    ],
                  },
                  'finish_reason': 'tool_calls',
                },
              ],
            })}\n\n',
          );
        } else {
          request.response.write(
            'data: ${jsonEncode({
              'choices': [
                {
                  'index': 0,
                  'delta': {'content': '项目相关整理结果如下。'},
                  'finish_reason': 'stop',
                },
              ],
            })}\n\n',
          );
        }
        request.response.write('data: [DONE]\n\n');
        await request.response.close();
      });
      await api.updateAiProviderConfig(
        baseUrl: 'http://127.0.0.1:${server.port}/v1',
        model: 'gpt-4o',
        apiKey: 'synthetic',
      );
      final main = await repo.createConversation(title: 'steering main');
      final page = await repo.saveTextPage(
        text: '合成测试材料。',
        title: 'steering source',
        tags: [],
      );
      final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
      for (final conversation in [main.id, wiki.id]) {
        calls = 0;
        payloads.clear();
        release = Completer<void>();
        final input = await repo.submitConversationInput(
          conversation,
          '先查规则，再整理材料。',
          idempotencyKey: 'original-$conversation',
        );
        final run = await api.startChatRun(
          conversationId: conversation,
          inputMessageId: input.id,
          idempotencyKey: 'run-$conversation',
        );
        await waitForRun(conversation, run.id, (r) => r.requestCount > 0);
        final supplement = await repo.submitConversationInput(
          conversation,
          '只保留项目相关部分',
          idempotencyKey: 'supplement-$conversation',
        );
        final id = await api.enqueueChatSupplement(
          conversationId: conversation,
          inputMessageId: supplement.id,
          runId: run.id,
          contextJson: '{}',
        );
        expect(
          await api.enqueueChatSupplement(
            conversationId: conversation,
            inputMessageId: supplement.id,
            runId: run.id,
            contextJson: '{}',
          ),
          id,
        );
        release.complete();
        final done = await waitForRun(
          conversation,
          run.id,
          (r) => r.status == 'completed' || r.status == 'failed',
        );
        expect(done.status, 'completed');
        expect(payloads, hasLength(2));
        expect(jsonEncode(payloads.first).contains('只保留项目相关部分'), isFalse);
        expect(jsonEncode(payloads.last).contains('只保留项目相关部分'), isTrue);
        expect(
          (payloads.last['messages'] as List).any((m) => m['role'] == 'tool'),
          isTrue,
        );
        final queue = await api.getChatInputQueue(
          conversationId: conversation,
          afterId: 0,
          limit: 100,
        );
        expect(queue.items, isEmpty);
        expect(queue.recentInjections.single.inputMessageId, supplement.id);
        expect(queue.recentInjections.single.status, 'injected');
        expect(
          await api.dispatchChatInputQueue(conversationId: conversation),
          isNull,
        );
        expect(
          (await repo.listMessages(conversation))
              .where((m) => m.role.name == 'user'),
          hasLength(2),
        );
      }
    },
  );
}
