import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/models/conversation.dart';

import 'chat_input_queue_widget_test.dart' show QueueRepository;
import 'chat_run_widget_test.dart' show app, settleFrames;
import 'support/chat_run_fixture.dart';

class FailedRegistrationRepository extends QueueRepository {
  bool failRegistration = true;
  final List<String> scheduled = [];

  @override
  Future<ChatRunDto?> scheduleChatInput(String conversationId, String inputMessageId) async {
    scheduled.add(inputMessageId);
    if (failRegistration) throw StateError('synthetic registration failure');
    return super.scheduleChatInput(conversationId, inputMessageId);
  }
}

void main() {
  for (final wiki in [false, true]) {
    testWidgets('${wiki ? 'wiki' : 'main'} recovers saved input after registration failure and an older answer', (tester) async {
      final repo = FailedRegistrationRepository();
      final original = await repo.sendMessage('conv-1', '先前的问题');
      repo.runs['conv-1'] = fixtureRun(input: original.id);
      Widget view() => wiki ? const WikiAiChatPanel(slug: 'topic/test') : const MessageArea();
      await tester.pumpWidget(app(repo, view()));
      await settleFrames(tester);
      await tester.enterText(find.byType(TextField), '已保存但未登记的问题');
      await tester.tap(find.byIcon(Icons.arrow_upward));
      await settleFrames(tester);
      final saved = repo.messages.last;
      expect(saved.content, '已保存但未登记的问题');
      expect(repo.scheduled, [saved.id]);
      expect(repo.queued, isEmpty);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
      final completed = fixtureRun(input: original.id, status: 'completed');
      repo.runs['conv-1'] = completed;
      repo.history.add(completed);
      repo.messages.add(Message(
        id: 'older-answer', conversationId: 'conv-1', role: MessageRole.assistant,
        content: '先前问题的迟到回复', createdAt: DateTime(2026, 10, 2),
      ));
      repo.failRegistration = false;
      await tester.pumpWidget(app(repo, view()));
      await settleFrames(tester);
      await tester.ensureVisible(find.text('生成历史与恢复'));
      await tester.tap(find.text('生成历史与恢复'));
      await settleFrames(tester);
      final recovery = find.byKey(ValueKey('recover-saved-${saved.id}'));
      expect(recovery, findsOneWidget);
      expect(find.byKey(ValueKey('recover-saved-${original.id}')), findsNothing);
      expect(repo.queued, isEmpty, reason: 'viewing recovery is read-only');
      await tester.ensureVisible(recovery);
      await tester.tap(recovery);
      await settleFrames(tester);
      expect(repo.scheduled, [saved.id, saved.id]);
      expect(repo.queued.single.inputMessageId, saved.id);
      expect(repo.messages.length, 3, reason: 'recovery reuses the saved message');
      expect(repo.resumes, 0);
      expect(repo.paused, isTrue);
      expect(repo.starts, isEmpty, reason: 'recovery uses scheduling, not direct start');
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    });
  }

  testWidgets(
    'older interrupted input remains explicitly recoverable after a newer answer',
    (tester) async {
      final repo = QueueRepository();
      final old = fixtureRun(
        id: 'old-run',
        input: 'old-input',
        status: 'interrupted',
        content: '保留的部分回答',
      );
      final latest = fixtureRun(
        id: 'new-run',
        input: 'new-input',
        status: 'completed',
      );
      repo.runs['conv-1'] = latest;
      repo.history.addAll([latest, old]);
      await tester.pumpWidget(app(repo, const MessageArea()));
      await settleFrames(tester);
      await tester.ensureVisible(find.text('生成历史与恢复'));
      await tester.tap(find.text('生成历史与恢复'));
      await settleFrames(tester);
      await tester.tap(find.text('输入 old-input'));
      await settleFrames(tester);
      expect(find.text('保留的部分回答', findRichText: true), findsOneWidget);
      expect(repo.starts, isEmpty);
      await tester.ensureVisible(find.text('重试所选原输入'));
      await tester.tap(find.text('重试所选原输入'));
      await settleFrames(tester);
      expect(repo.starts.single.$2, 'old-input');
      expect(
        repo.messages,
        isEmpty,
        reason: 'recovery does not save the original again',
      );
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    },
  );
}
