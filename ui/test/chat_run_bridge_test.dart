import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/generated.dart/api/chat_runs.dart' as runs;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

Future<runs.ChatRunDto> waitForRun(
  String conversation,
  String id,
  bool Function(runs.ChatRunDto) ready,
) async {
  final until = DateTime.now().add(const Duration(seconds: 10));
  runs.ChatRunDto? last;
  while (DateTime.now().isBefore(until)) {
    final run = await runs.getChatRun(conversationId: conversation, runId: id);
    last = run;
    if (run != null && ready(run)) return run;
    await Future<void>.delayed(const Duration(milliseconds: 25));
  }
  throw StateError(
    'run did not reach expected state: ${last?.status}/${last?.phase}; ${last?.errorText}; content=${last?.content}',
  );
}

void main() {
  test(
    'isolated main/wiki run identity, SSE tools, reconnect and cancellation',
    () async {
      final repo = await createIsolatedBridge();
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));
      var requestCount = 0;
      var stalled = false;
      final release = Completer<void>();
      addTearDown(() {
        if (!release.isCompleted) release.complete();
      });
      server.listen((request) async {
        await utf8.decoder.bind(request).join();
        requestCount++;
        request.response.bufferOutput = false;
        request.response.headers.contentType = ContentType(
          'text',
          'event-stream',
          charset: 'utf-8',
        );
        void data(Object value) =>
            request.response.write('data: ${jsonEncode(value)}\n\n');
        if (stalled) {
          data({
            'choices': [
              {
                'index': 0,
                'delta': {'content': '这是普通说明。已保存全部记录。'},
              },
            ],
          });
          await request.response.flush();
          await release.future;
        } else if (requestCount.isOdd) {
          data({
            'choices': [
              {
                'index': 0,
                'delta': {
                  'tool_calls': [
                    {
                      'index': 0,
                      'id': 'read-$requestCount',
                      'function': {'name': 'list_rules', 'arguments': '{'},
                    },
                  ],
                },
              },
            ],
          });
          data({
            'choices': [
              {
                'index': 0,
                'delta': {
                  'tool_calls': [
                    {
                      'index': 0,
                      'function': {'arguments': '}'},
                    },
                  ],
                },
                'finish_reason': 'tool_calls',
              },
            ],
          });
        } else {
          data({
            'choices': [
              {
                'index': 0,
                'delta': {'content': '查询结束。这是测试回答。'},
                'finish_reason': 'stop',
              },
            ],
          });
          data({
            'choices': [],
            'usage': {'prompt_tokens': 20, 'completion_tokens': 5},
          });
        }
        try {
          request.response.write('data: [DONE]\n\n');
          await request.response.close();
        } catch (_) {
          /* The cancellation test intentionally closes the client. */
        }
      });
      await api.updateAiProviderConfig(
        baseUrl: 'http://127.0.0.1:${server.port}/v1',
        model: 'gpt-4o',
        apiKey: 'synthetic',
      );
      final main = await repo.createConversation(title: 'run main');
      final page = await repo.saveTextPage(
        text: '隔离测试材料。',
        title: 'run source',
        tags: [],
      );
      final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
      for (final conversation in [main.id, wiki.id]) {
        final input = conversation == main.id
            ? await repo.submitConversationInput(
                conversation,
                '查询规则后解释一个概念',
                idempotencyKey: 'input-$conversation',
              )
            : await repo.sendMessage(conversation, 'user', '查询规则后解释一个概念');
        final run = await runs.startChatRun(
          conversationId: conversation,
          inputMessageId: input.id,
          idempotencyKey: 'run-$conversation',
        );
        final repeated = await runs.startChatRun(
          conversationId: conversation,
          inputMessageId: input.id,
          idempotencyKey: 'run-$conversation',
        );
        expect(repeated.id, run.id);
        final finalRun = await waitForRun(
          conversation,
          run.id,
          (r) => r.status == 'completed' || r.status == 'failed',
        );
        expect(finalRun.status, 'completed', reason: finalRun.errorText);
        expect(finalRun.content, '查询结束。这是测试回答。');
        expect(finalRun.finalMessageId, isNotNull);
        expect(
          (await repo.listMessages(conversation)).where((m) => m.isUser),
          hasLength(1),
        );
        var cursor = 0;
        final kinds = <String>[];
        while (true) {
          final events = await runs.listChatRunEvents(
            conversationId: conversation,
            runId: run.id,
            afterSequence: cursor,
            limit: 2,
          );
          for (final event in events.events) {
            expect(event.sequence, cursor + 1);
            cursor = event.sequence.toInt();
            kinds.add(event.kind);
          }
          if (!events.hasMore) break;
        }
        expect(
          kinds,
          containsAll(['tool_started', 'tool_result', 'completed']),
        );
        expect(cursor, finalRun.sequence);
        expect(
          (await runs.getLatestChatRun(conversationId: conversation))!.id,
          run.id,
        );
        await expectLater(
          runs.getChatRun(
            conversationId: conversation == main.id ? wiki.id : main.id,
            runId: run.id,
          ),
          throwsA(anything),
        );
      }
      expect(requestCount, 4);
      stalled = true;
      final input = await repo.submitConversationInput(
        main.id,
        '继续解释概念',
        idempotencyKey: 'cancel-input',
      );
      final run = await runs.startChatRun(
        conversationId: main.id,
        inputMessageId: input.id,
        idempotencyKey: 'cancel-run',
      );
      final draft = await waitForRun(
        main.id,
        run.id,
        (r) => r.content.isNotEmpty,
      );
      expect(draft.content, '这是普通说明。');
      final stoppedAt = Stopwatch()..start();
      await runs.cancelChatRun(conversationId: main.id, runId: run.id);
      final cancelled = await waitForRun(
        main.id,
        run.id,
        (r) => r.status == 'cancelled',
      );
      expect(stoppedAt.elapsed, lessThan(const Duration(seconds: 2)));
      expect(cancelled.finalMessageId, isNull);
      expect(cancelled.content, '这是普通说明。');
      expect(
        (await runs.getLatestChatRun(conversationId: wiki.id))!.status,
        'completed',
      );
      release.complete();
      expect(
        (await repo.listMessages(main.id)).where((m) => m.isAssistant),
        hasLength(1),
      );
    },
  );
}
