import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'chat_run_bridge_test.dart' show waitForRun;
import 'support/isolated_bridge.dart';

void main() {
  test('both entrances recover the save-before-schedule gap after an older reply', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    var release = Completer<void>();
    addTearDown(() async {
      if (!release.isCompleted) release.complete();
      await server.close(force: true);
    });
    server.listen((request) async {
      await utf8.decoder.bind(request).join();
      await release.future;
      request.response.headers.contentType = ContentType('text', 'event-stream', charset: 'utf-8');
      request.response.write('data: ${jsonEncode({'choices': [{'index': 0, 'delta': {'content': '这是先前问题的说明。'}, 'finish_reason': 'stop'}]})}\n\n');
      request.response.write('data: [DONE]\n\n');
      await request.response.close();
    });
    await api.updateAiProviderConfig(baseUrl: 'http://127.0.0.1:${server.port}/v1', model: 'gpt-4o', apiKey: 'synthetic');
    final main = await repo.createConversation(title: 'saved recovery');
    final page = await repo.saveTextPage(text: '合成恢复测试材料。', title: 'saved recovery source', tags: []);
    final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
    for (final conversation in [main.id, wiki.id]) {
      release = Completer<void>();
      final first = await repo.submitConversationInput(conversation, '解释先前概念', idempotencyKey: 'recovery-first-$conversation');
      final run = await repo.scheduleChatInput(conversation, first.id);
      await waitForRun(conversation, run!.id, (r) => r.requestCount > 0);
      // Model a process/client interruption after the local save committed but
      // before any scheduling call. The preceding run can still finish later.
      final saved = await repo.submitConversationInput(conversation, '已保存尚未登记的问题', idempotencyKey: 'recovery-saved-$conversation');
      release.complete();
      await waitForRun(conversation, run.id, (r) => r.status == 'completed');
      final messagesBefore = await repo.listMessages(conversation);
      expect(messagesBefore.last.role.name, 'assistant');
      final available = await repo.listUnscheduledChatInputs(conversation);
      expect(available.map((input) => input.inputMessageId), [saved.id]);
      final earlierQueued = await repo.submitConversationInput(conversation, '已先登记的输入', idempotencyKey: 'recovery-queued-$conversation');
      await repo.enqueueChatInput(conversation, earlierQueued.id);
      await repo.pauseChatInputQueue(conversation);
      expect(await repo.scheduleChatInput(conversation, saved.id), isNull);
      expect(await repo.scheduleChatInput(conversation, saved.id), isNull);
      final queue = await repo.getChatInputQueue(conversation);
      expect(queue.paused, isTrue);
      expect(queue.items.map((input) => input.inputMessageId), [earlierQueued.id, saved.id]);
      expect(await repo.listUnscheduledChatInputs(conversation), isEmpty);
      expect((await repo.listMessages(conversation)).length, messagesBefore.length + 1);
      expect(await repo.dispatchChatInputQueue(conversation), isNull);
      expect((await repo.listChatRuns(conversation)).length, 1);
    }
  });
}
