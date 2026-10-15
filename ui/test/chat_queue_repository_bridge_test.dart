import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'chat_run_bridge_test.dart' show waitForRun;
import 'support/isolated_bridge.dart';

void main() {
  test(
    'repository dispatches three saved inputs without a mounted chat view',
    () async {
      final repo = await createIsolatedBridge(runChatQueueWorker: true);
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final firstRequest = Completer<void>();
      final releaseFirst = Completer<void>();
      var requests = 0;
      addTearDown(() async {
        if (!releaseFirst.isCompleted) releaseFirst.complete();
        await server.close(force: true);
      });
      server.listen((request) async {
        await utf8.decoder.bind(request).join();
        final number = ++requests;
        if (number == 1) {
          firstRequest.complete();
          await releaseFirst.future;
        }
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
                'delta': {'content': '回答 $number：这是合成测试。'},
              },
            ],
          })}\n\n',
        );
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {'index': 0, 'delta': {}, 'finish_reason': 'stop'},
            ],
          })}\n\n',
        );
        request.response.write('data: [DONE]\n\n');
        await request.response.close();
      });
      await api.updateAiProviderConfig(
        baseUrl: 'http://127.0.0.1:${server.port}/v1',
        model: 'gpt-4o',
        apiKey: 'synthetic',
      );
      final conversation = await repo.createConversation(
        title: 'repository queue',
      );
      final first = await repo.submitConversationInput(
        conversation.id,
        '解释概念一',
        idempotencyKey: 'repo-queue-1',
      );
      final run = await repo.scheduleChatInput(conversation.id, first.id);
      await firstRequest.future.timeout(const Duration(seconds: 10));
      final second = await repo.submitConversationInput(
        conversation.id,
        '解释概念二',
        idempotencyKey: 'repo-queue-2',
      );
      final third = await repo.submitConversationInput(
        conversation.id,
        '解释概念三',
        idempotencyKey: 'repo-queue-3',
      );
      expect(await repo.scheduleChatInput(conversation.id, second.id), isNull);
      expect(await repo.scheduleChatInput(conversation.id, third.id), isNull);
      releaseFirst.complete();
      await waitForRun(
        conversation.id,
        run!.id,
        (value) => value.status == 'completed',
      );
      final deadline = DateTime.now().add(const Duration(seconds: 15));
      while (DateTime.now().isBefore(deadline)) {
        final runs = await repo.listChatRuns(conversation.id);
        if (runs.length == 3 &&
            runs.every((run) => run.status == 'completed')) {
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      final runs = await repo.listChatRuns(conversation.id);
      expect(runs.length, 3);
      expect(runs.every((run) => run.status == 'completed'), isTrue);
      expect(runs.map((run) => run.inputMessageId).toSet(), {
        first.id,
        second.id,
        third.id,
      });
      expect((await repo.getChatInputQueue(conversation.id)).items, isEmpty);
      expect(requests, 3);
      await repo.pauseChatInputQueue(conversation.id);
      final fourth = await repo.submitConversationInput(
        conversation.id,
        '暂停后下一问',
        idempotencyKey: 'repo-queue-4',
      );
      expect(await repo.scheduleChatInput(conversation.id, fourth.id), isNull);
      await Future<void>.delayed(const Duration(milliseconds: 1200));
      expect((await repo.getChatInputQueue(conversation.id)).paused, isTrue);
      expect((await repo.listChatRuns(conversation.id)).length, 3);
      expect(
        requests,
        3,
        reason: 'background observation cannot resume a paused queue',
      );
    },
  );
}
