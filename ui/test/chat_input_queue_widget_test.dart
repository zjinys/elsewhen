import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';

import 'chat_run_widget_test.dart' show app, settleFrames;
import 'support/chat_run_fixture.dart';

class QueueRepository extends RunTestRepository {
  bool paused = true;
  int resumes = 0;
  final List<ChatQueuedInputDto> queued = [];
  final List<ChatQueuedInputDto> injected = [];
  final List<(String, String)> supplements = [];
  final List<ChatRunDto> history = [];
  @override
  Future<ChatRunDto?> scheduleChatInput(
    String conversationId,
    String inputMessageId,
  ) async {
    await enqueueChatInput(conversationId, inputMessageId);
    return null;
  }

  @override
  Future<List<UnscheduledChatInputDto>> listUnscheduledChatInputs(
    String conversationId, {
    int offset = 0,
  }) async {
    final associated = {
      for (final run in [...runs.values, ...history]) run.inputMessageId,
      for (final item in [...queued, ...injected]) item.inputMessageId,
    };
    return messages
        .where(
          (message) =>
              message.isUser &&
              message.conversationId == conversationId &&
              !associated.contains(message.id),
        )
        .skip(offset)
        .take(20)
        .map(
          (message) => UnscheduledChatInputDto(
            inputMessageId: message.id,
            preview: message.content,
            createdAt: message.createdAt.toIso8601String(),
          ),
        )
        .toList();
  }

  @override
  Future<List<ChatRunDto>> listChatRuns(
    String conversationId, {
    int offset = 0,
  }) async => history.skip(offset).take(20).toList();
  @override
  Future<ChatRunDto?> getChatRun(String conversationId, String runId) async =>
      history.where((run) => run.id == runId).firstOrNull ??
      await super.getChatRun(conversationId, runId);
  @override
  Future<int> enqueueChatSupplement(
    String conversationId,
    String inputMessageId,
    String runId, {
    String contextJson = '{}',
  }) async {
    supplements.add((inputMessageId, runId));
    final id = queued.length + 1;
    queued.add(
      ChatQueuedInputDto(
        id: id,
        inputMessageId: inputMessageId,
        status: 'pending',
        mode: 'steer',
        preview: messages.firstWhere((m) => m.id == inputMessageId).content,
      ),
    );
    return id;
  }

  @override
  bool get supportsChatQueue => true;
  @override
  Future<int> enqueueChatInput(
    String conversationId,
    String inputMessageId,
  ) async {
    final id = queued.length + 1;
    queued.add(
      ChatQueuedInputDto(
        id: id,
        inputMessageId: inputMessageId,
        status: 'pending',
        mode: 'next',
        preview: messages.firstWhere((m) => m.id == inputMessageId).content,
      ),
    );
    return id;
  }

  @override
  Future<ChatInputQueueDto> getChatInputQueue(
    String conversationId, {
    int afterId = 0,
  }) async => ChatInputQueueDto(
    paused: paused,
    items: List.of(queued),
    recentInjections: List.of(injected),
  );
  @override
  Future<ChatRunDto?> dispatchChatInputQueue(String conversationId) async =>
      null;
  @override
  Future<void> resumeChatInputQueue(String conversationId) async {
    paused = false;
    resumes++;
  }

  @override
  Future<void> pauseChatInputQueue(String conversationId) async {
    paused = true;
  }

  @override
  Future<bool> withdrawChatInput(String conversationId, int queueId) async {
    queued.removeWhere((item) => item.id == queueId);
    return true;
  }
}

void main() {
  for (final wiki in [false, true]) {
    testWidgets(
      '${wiki ? 'wiki' : 'main'} saves while generating and withdraws only eligibility',
      (tester) async {
        final repo = QueueRepository();
        repo.runs['conv-1'] = fixtureRun(input: 'earlier-input');
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        await tester.enterText(find.byType(TextField), 'saved next question');
        await tester.tap(find.byIcon(Icons.arrow_upward));
        await settleFrames(tester);
        expect(repo.messages.single.content, 'saved next question');
        expect(repo.queued.single.inputMessageId, repo.messages.single.id);
        expect(repo.starts, isEmpty);
        expect(
          repo.resumes,
          0,
          reason: 'queue observation never implicitly resumes a stopped queue',
        );
        await tester.pump(const Duration(seconds: 1));
        await settleFrames(tester);
        await tester.ensureVisible(find.text('撤回生成'));
        await tester.tap(find.text('撤回生成'));
        await settleFrames(tester);
        expect(repo.queued, isEmpty);
        expect(repo.messages.length, 1);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      },
    );

    testWidgets(
      '${wiki ? 'wiki' : 'main'} explicit supplement binds the displayed run',
      (tester) async {
        final repo = QueueRepository();
        repo.runs['conv-1'] = fixtureRun(input: 'earlier-input');
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        await tester.enterText(find.byType(TextField), '只保留项目相关部分');
        await tester.tap(find.byTooltip('输入保存方式'));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 350));
        await tester.tap(find.text('补充当前任务').hitTestable());
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 350));
        await settleFrames(tester);
        expect(repo.supplements.single, (repo.messages.single.id, 'run-1'));
        expect(repo.queued.single.mode, 'steer');
        expect(repo.starts, isEmpty);
        expect(repo.resumes, 0);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      },
    );

    testWidgets(
      '${wiki ? 'wiki' : 'main'} record-only saves without enqueue or generation',
      (tester) async {
        final repo = QueueRepository();
        repo.runs['conv-1'] = fixtureRun(input: 'earlier-input');
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        await tester.enterText(find.byType(TextField), 'only local record');
        await tester.tap(find.byTooltip('输入保存方式'));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 350));
        await tester.tap(find.text('仅保存，不生成回复').hitTestable());
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 350));
        await settleFrames(tester);
        expect(repo.messages.single.content, 'only local record');
        expect(repo.queued, isEmpty);
        expect(repo.starts, isEmpty);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      },
    );
  }
}
