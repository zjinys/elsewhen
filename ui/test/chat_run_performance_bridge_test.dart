import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/generated.dart/api/chat_runs.dart' as runs;
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/widgets/chat_run_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

Future<void> pumpUntil(WidgetTester tester, bool Function() ready) async {
  final until = DateTime.now().add(const Duration(seconds: 8));
  while (!ready()) {
    if (DateTime.now().isAfter(until)) {
      throw StateError('live bridge UI timed out');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 25)),
    );
    await tester.pump(const Duration(milliseconds: 25));
  }
}

void main() {
  testWidgets(
    'loopback SSE reaches actual shared widget and stop without waiting for EOF',
    (tester) async {
      final repo = (await tester.runAsync(createIsolatedBridge))!;
      final server = (await tester.runAsync(
        () => HttpServer.bind(InternetAddress.loopbackIPv4, 0),
      ))!;
      final send = Completer<void>();
      final release = Completer<void>();
      final received = Completer<void>();
      addTearDown(() async {
        if (!send.isCompleted) send.complete();
        if (!release.isCompleted) release.complete();
        await server.close(force: true);
      });
      await tester.runAsync(() async {
        server.listen((request) async {
          await utf8.decoder.bind(request).join();
          if (!received.isCompleted) received.complete();
          await send.future;
          request.response.bufferOutput = false;
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
                  'delta': {'content': '这里是一段可见说明。'},
                },
              ],
            })}\n\n',
          );
          await request.response.flush();
          await release.future;
          try {
            await request.response.close();
          } catch (_) {
            /* stopped client */
          }
        });
        await api.updateAiProviderConfig(
          baseUrl: 'http://127.0.0.1:${server.port}/v1',
          model: 'gpt-4o',
          apiKey: 'synthetic-key',
        );
      });
      final conversation = (await tester.runAsync(
        () => repo.createConversation(title: 'latency fixture'),
      ))!;
      final input = (await tester.runAsync(
        () => repo.submitConversationInput(
          conversation.id,
          '说明一个概念',
          idempotencyKey: 'latency-input',
        ),
      ))!;
      final run = (await tester.runAsync(
        () => runs.startChatRun(
          conversationId: conversation.id,
          inputMessageId: input.id,
          idempotencyKey: 'latency-run',
        ),
      ))!;
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            conversationRepositoryProvider.overrideWithValue(
              ConversationRepository(repo),
            ),
          ],
          child: MaterialApp(
            home: Scaffold(
              body: ChatRunStatus(conversationId: conversation.id),
            ),
          ),
        ),
      );
      await pumpUntil(tester, () => received.isCompleted);
      final visible = Stopwatch()..start();
      send.complete();
      await pumpUntil(
        tester,
        () => find.text('这里是一段可见说明。').evaluate().isNotEmpty,
      );
      visible.stop();
      expect(find.text('未核验草稿 · 不代表操作成功或引用有效'), findsOneWidget);
      final cancellation = Stopwatch()..start();
      await tester.tap(find.text('停止生成'));
      await pumpUntil(tester, () => find.text('已停止生成').evaluate().isNotEmpty);
      cancellation.stop();
      final finalRun = await tester.runAsync(
        () => runs.getChatRun(conversationId: conversation.id, runId: run.id),
      );
      expect(finalRun!.status, 'cancelled');
      expect(finalRun.finalMessageId, isNull);
      expect(visible.elapsed, lessThan(const Duration(seconds: 2)));
      expect(cancellation.elapsed, lessThan(const Duration(seconds: 2)));
      debugPrint(
        'CHAT_LOOPBACK_UI first_visible_ms=${visible.elapsedMilliseconds} cancel_visible_ms=${cancellation.elapsedMilliseconds}; polling=300ms checkpoint=100ms; synthetic provider, no model latency claim',
      );
      release.complete();
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 25)),
      );
    },
  );
}
