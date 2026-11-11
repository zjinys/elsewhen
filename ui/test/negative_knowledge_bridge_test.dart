import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/generated.dart/api/chat_runs.dart' as runs;
import 'package:flutter_test/flutter_test.dart';

import 'chat_run_bridge_test.dart' show waitForRun;
import 'support/isolated_bridge.dart';

void main() {
  test('negative-only context reaches model without positive citations', () async {
    final repo = await createIsolatedBridge();
    final page = await api.confirmKnowledgeSource(
      title: '推广投放旧方案',
      contentMd: '推广投放面向大型客户。',
      sourceUrl: 'https://example.com/negative-fixture',
      sourceKind: 'webpage',
      tags: [],
    );
    await api.decideKnowledgeSource(
      slug: page.slug,
      decision: 'reject',
      reason: '预算不足，不适合当前小店客户群。',
    );
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final requests = <Map<String, dynamic>>[];
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
              'delta': {'content': '你曾因预算和客户群排除这个方案。现在条件变了吗？[[${page.slug}]]'},
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
    final conversation = await repo.createConversation(
      title: 'negative fixture',
    );
    final input = await repo.sendMessage(
      conversation.id,
      'user',
      '推广投放之前有什么限制？',
    );
    final run = await runs.startChatRun(
      conversationId: conversation.id,
      inputMessageId: input.id,
      idempotencyKey: 'negative-only',
    );
    final finished = await waitForRun(
      conversation.id,
      run.id,
      (r) => r.status == 'completed' || r.status == 'failed',
    );
    expect(finished.status, 'completed', reason: finished.errorText);
    final contexts = (requests.single['messages'] as List)
        .where((m) => (m['content'] as String? ?? '').startsWith('知识选材：\n'))
        .map(
          (m) =>
              jsonDecode((m['content'] as String).substring('知识选材：\n'.length))
                  as Map,
        )
        .toList();
    expect(contexts, hasLength(1));
    expect(contexts.single['knowledge_candidates'], isEmpty);
    final warning = (contexts.single['knowledge_warnings'] as List).single;
    expect(warning['page_slug'], page.slug);
    expect(warning['rejection_reason'], '预算不足，不适合当前小店客户群。');
    expect(
      await api.getMessageKnowledgeCitations(
        messageId: finished.finalMessageId!,
      ),
      isEmpty,
    );
    expect((await repo.getWikiPage(page.slug))!.status, 'archived');
  });
}
