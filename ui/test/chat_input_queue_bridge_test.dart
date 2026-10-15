import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';
import 'chat_run_bridge_test.dart' show waitForRun;

void main() {
  test('saved inputs queue once; stop freezes FIFO in both chat entrances', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final release = Completer<void>();
    addTearDown(() async {
      if (!release.isCompleted) release.complete();
      await server.close(force: true);
    });
    server.listen((request) async {
      await utf8.decoder.bind(request).join();
      request.response.headers.contentType = ContentType(
        'text',
        'event-stream',
        charset: 'utf-8',
      );
      request.response.bufferOutput = false;
      request.response.write(
        'data: ${jsonEncode({
          'choices': [
            {
              'index': 0,
              'delta': {'content': '正在处理。'},
            },
          ],
        })}\n\n',
      );
      await request.response.flush();
      await release.future;
      try {
        await request.response.close();
      } catch (_) {
        /* cancelled client */
      }
    });
    await api.updateAiProviderConfig(
      baseUrl: 'http://127.0.0.1:${server.port}/v1',
      model: 'gpt-4o',
      apiKey: 'synthetic',
    );
    final main = await repo.createConversation(title: 'queue main');
    final page = await repo.saveTextPage(
      text: '合成测试材料。',
      title: 'queue source',
      tags: [],
    );
    final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
    for (final conversation in [main.id, wiki.id]) {
      final first = await repo.submitConversationInput(
        conversation,
        '解释一个概念',
        idempotencyKey: 'first-$conversation',
      );
      final active = await api.startChatRun(
        conversationId: conversation,
        inputMessageId: first.id,
        idempotencyKey: 'first-run-$conversation',
      );
      await waitForRun(conversation, active.id, (r) => r.requestCount > 0);
      final save = Stopwatch()..start();
      final second = await repo.submitConversationInput(
        conversation,
        '这是下一问',
        idempotencyKey: 'second-$conversation',
      );
      final duplicate = await repo.submitConversationInput(
        conversation,
        '这是下一问',
        idempotencyKey: 'second-$conversation',
      );
      expect(duplicate.id, second.id);
      expect(save.elapsed, lessThan(const Duration(seconds: 2)));
      stdout.writeln(
        'CHAT_QUEUE_LOCAL ${jsonEncode({'entrance': conversation == main.id ? 'main' : 'wiki', 'save_and_duplicate_us': save.elapsedMicroseconds, 'provider_still_waiting': !release.isCompleted})}',
      );
      final secondQueue = await api.enqueueChatInput(
        conversationId: conversation,
        inputMessageId: second.id,
      );
      expect(
        await api.enqueueChatInput(
          conversationId: conversation,
          inputMessageId: second.id,
        ),
        secondQueue,
      );
      final third = await repo.submitConversationInput(
        conversation,
        '撤回这一问',
        idempotencyKey: 'third-$conversation',
      );
      final thirdQueue = await api.enqueueChatInput(
        conversationId: conversation,
        inputMessageId: third.id,
      );
      await api.resumeChatInputQueue(conversationId: conversation);
      expect(
        await api.dispatchChatInputQueue(conversationId: conversation),
        isNull,
      );
      await api.cancelChatRun(conversationId: conversation, runId: active.id);
      await waitForRun(conversation, active.id, (r) => r.status == 'cancelled');
      final paused = await api.getChatInputQueue(
        conversationId: conversation,
        afterId: 0,
        limit: 100,
      );
      expect(paused.paused, isTrue);
      expect(paused.items.map((i) => i.inputMessageId), [second.id, third.id]);
      expect(
        await api.dispatchChatInputQueue(conversationId: conversation),
        isNull,
      );
      expect(
        await api.withdrawChatInput(
          conversationId: conversation,
          queueId: thirdQueue,
        ),
        isTrue,
      );
      await api.resumeChatInputQueue(conversationId: conversation);
      final next = await api.dispatchChatInputQueue(
        conversationId: conversation,
      );
      expect(next!.inputMessageId, second.id);
      expect(
        await api.dispatchChatInputQueue(conversationId: conversation),
        isNull,
      );
      await api.cancelChatRun(conversationId: conversation, runId: next.id);
      await waitForRun(conversation, next.id, (r) => r.status == 'cancelled');
      expect(
        (await repo.listMessages(conversation))
            .where((m) => m.role.name == 'user')
            .length,
        3,
      );
      expect(
        (await api.getChatInputQueue(
          conversationId: conversation,
          afterId: 0,
          limit: 100,
        )).items,
        isEmpty,
      );
    }
  });
}
