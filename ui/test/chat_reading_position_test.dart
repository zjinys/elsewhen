import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/chat_run_provider.dart';
import 'package:elsewhen_ui/widgets/chat_reading_controller.dart';
import 'package:elsewhen_ui/widgets/goal_chat_panel.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';

import 'chat_run_widget_test.dart' show app, settleFrames;
import 'support/chat_run_fixture.dart';

Message message(String id, String content, {bool user = false}) => Message(
  id: id,
  conversationId: 'conv-1',
  role: user ? MessageRole.user : MessageRole.assistant,
  content: content,
  createdAt: DateTime(2026, 11, 19, 10),
);

String longReply(String title) =>
    '$title\n\n${List.generate(65, (i) => '第 $i 段：这是需要从开头阅读的说明。').join('\n\n')}';

Finder get readingList => find.byWidgetPredicate(
  (w) => w is ListView && w.controller is ChatReadingController,
);

ChatReadingController controller(WidgetTester tester) =>
    tester.widget<ListView>(readingList).controller! as ChatReadingController;

Finder reply(String prefix) => find.byWidgetPredicate(
  (w) => w is MarkdownView && w.markdown.startsWith(prefix),
);

void main() {
  for (final kind in ['main', 'wiki', 'goal']) {
    Future<ProviderContainer> mount(
      WidgetTester tester,
      RunTestRepository repo,
    ) async {
      await tester.binding.setSurfaceSize(const Size(1000, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      repo.messages.addAll(
        List.generate(
          12,
          (i) =>
              message('history-$i', '历史 $i\n说明第一行\n说明第二行\n说明第三行', user: true),
        ),
      );
      final child = switch (kind) {
        'wiki' => const WikiAiChatPanel(slug: 'topic/test'),
        'goal' => const GoalChatPanel(goalId: 'goal-1', fillHeight: true),
        _ => const MessageArea(),
      };
      await tester.pumpWidget(app(repo, child, wikiBridge: _PanelBridge(repo)));
      await settleFrames(tester);
      expect(controller(tester).position.extentAfter, lessThan(1));
      return ProviderScope.containerOf(tester.element(readingList));
    }

    Future<void> refresh(
      WidgetTester tester,
      ProviderContainer container,
    ) async {
      switch (kind) {
        case 'wiki':
          container.invalidate(wikiChatMessagesProvider('conv-1'));
        case 'goal':
          container.invalidate(goalChatMessagesProvider('conv-1'));
        default:
          container.invalidate(messagesProvider);
      }
      await settleFrames(tester);
    }

    testWidgets(
      '$kind: batched replies reveal the first reply and later replies keep it',
      (tester) async {
        final repo = RunTestRepository();
        final container = await mount(tester, repo);
        repo.messages.addAll([
          message('answer-1', longReply('第一条回复开头')),
          message('answer-2', longReply('第二条回复开头')),
        ]);
        await refresh(tester, container);
        final viewport = tester.getRect(readingList);
        final firstTop = tester.getTopLeft(reply('第一条回复开头')).dy;
        expect(firstTop, inInclusiveRange(viewport.top, viewport.top + 100));
        expect(
          controller(tester).position.extentAfter,
          greaterThan(viewport.height),
        );

        final offset = controller(tester).offset;
        repo.messages.add(message('answer-3', longReply('后续回复')));
        await refresh(tester, container);
        expect(controller(tester).offset, closeTo(offset, 1));
        expect(tester.getTopLeft(reply('第一条回复开头')).dy, closeTo(firstTop, 1));
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      '$kind: a short first reply stays visible when a long second reply arrives',
      (tester) async {
        final repo = RunTestRepository();
        final container = await mount(tester, repo);
        repo.messages.add(message('short', '先给你一个简短结论。'));
        await refresh(tester, container);
        final viewport = tester.getRect(readingList);
        final firstTop = tester.getTopLeft(reply('先给你')).dy;
        expect(firstTop, inInclusiveRange(viewport.top, viewport.bottom));
        expect(controller(tester).position.extentAfter, lessThan(1));
        final offset = controller(tester).offset;

        repo.messages.add(message('long', longReply('下面展开解释')));
        await refresh(tester, container);
        expect(controller(tester).offset, closeTo(offset, 1));
        expect(tester.getTopLeft(reply('先给你')).dy, closeTo(firstTop, 1));
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('$kind: many replies do not evict the first unread reply', (
      tester,
    ) async {
      final repo = RunTestRepository();
      final container = await mount(tester, repo);
      repo.messages.addAll(
        List.generate(
          30,
          (i) => message(
            'batch-$i',
            i == 0 ? longReply('大量回复的第一条') : '后续第 $i 条回复',
          ),
        ),
      );
      await refresh(tester, container);
      final viewport = tester.getRect(readingList);
      expect(
        tester.getTopLeft(reply('大量回复的第一条')).dy,
        inInclusiveRange(viewport.top, viewport.top + 100),
      );
      expect(tester.takeException(), isNull);
    });

    testWidgets('$kind: same-count refresh still detects a new reply', (
      tester,
    ) async {
      final repo = RunTestRepository();
      final container = await mount(tester, repo);
      repo.messages.removeAt(0);
      repo.messages.add(message('replacement', longReply('数量不变的新回复')));
      await refresh(tester, container);
      final viewport = tester.getRect(readingList);
      expect(
        tester.getTopLeft(reply('数量不变的新回复')).dy,
        inInclusiveRange(viewport.top, viewport.top + 100),
      );
      expect(tester.takeException(), isNull);
    });

    testWidgets(
      '$kind: a reply inserted before queued inputs is revealed from its start',
      (tester) async {
        final repo = RunTestRepository();
        final container = await mount(tester, repo);
        repo.messages.insert(
          repo.messages.length - 5,
          message('inserted', longReply('排队问题前插入的回复')),
        );
        await refresh(tester, container);
        final viewport = tester.getRect(readingList);
        expect(
          tester.getTopLeft(reply('排队问题前插入的回复')).dy,
          inInclusiveRange(viewport.top, viewport.top + 100),
        );
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('$kind: reading history is not interrupted by new replies', (
      tester,
    ) async {
      final repo = RunTestRepository();
      final container = await mount(tester, repo);
      await tester.drag(readingList, const Offset(0, 350));
      await tester.pumpAndSettle();
      final offset = controller(tester).offset;
      expect(controller(tester).position.extentAfter, greaterThan(120));
      repo.messages.addAll([
        message('answer-1', longReply('不要抢走阅读位置')),
        message('answer-2', longReply('还有一条')),
      ]);
      await refresh(tester, container);
      expect(controller(tester).offset, closeTo(offset, 1));
      expect(tester.takeException(), isNull);
    });

    testWidgets(
      '$kind: growing content and repeated refresh keep the reading position',
      (tester) async {
        final repo = RunTestRepository();
        final container = await mount(tester, repo);
        repo.messages.add(message('answer', longReply('逐步增长的回复')));
        await refresh(tester, container);
        await tester.drag(readingList, const Offset(0, -250));
        await tester.pumpAndSettle();
        final offset = controller(tester).offset;
        repo.messages[repo.messages.length - 1] = message(
          'answer',
          '${longReply('逐步增长的回复')}\n\n${longReply('补充内容')}',
        );
        await refresh(tester, container);
        await refresh(tester, container);
        expect(controller(tester).offset, closeTo(offset, 1));
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      '$kind: legacy generation reveals the answer after sending from history',
      (tester) async {
        final repo = _LegacyRepository();
        await mount(tester, repo);
        await tester.drag(readingList, const Offset(0, 350));
        await tester.pumpAndSettle();
        expect(controller(tester).position.extentAfter, greaterThan(120));
        await tester.enterText(find.byType(TextField), '从历史位置发送新问题');
        await tester.tap(find.byIcon(Icons.arrow_upward));
        await settleFrames(tester);
        expect(controller(tester).position.extentAfter, lessThan(1));
        repo.gate.complete(longReply('兼容生成路径的回复开头'));
        await settleFrames(tester);
        final viewport = tester.getRect(readingList);
        expect(
          tester.getTopLeft(reply('兼容生成路径的回复开头')).dy,
          inInclusiveRange(viewport.top, viewport.top + 100),
        );
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('$kind: switching scope cancels the old reading anchor', (
      tester,
    ) async {
      final repo = RunTestRepository();
      final container = await mount(tester, repo);
      repo.messages.add(message('old-answer', longReply('旧会话的长回复')));
      await refresh(tester, container);
      repo.messages.clear();
      repo.messages.addAll(
        List.generate(
          24,
          (i) => Message(
            id: 'new-$i',
            conversationId: 'conv-2',
            role: MessageRole.user,
            content: '新会话 $i\n第二行\n第三行',
            createdAt: DateTime(2026, 11, 19),
          ),
        ),
      );
      if (kind == 'main') {
        container.read(selectedConversationIdProvider.notifier).set('conv-2');
        container.invalidate(messagesProvider);
      } else {
        final next = kind == 'wiki'
            ? const WikiAiChatPanel(slug: 'topic/next')
            : const GoalChatPanel(goalId: 'goal-2', fillHeight: true);
        await tester.pumpWidget(
          app(
            repo,
            next,
            wikiBridge: _PanelBridge(repo, conversationId: 'conv-2'),
          ),
        );
      }
      await settleFrames(tester);
      expect(controller(tester).position.extentAfter, lessThan(1));
      expect(find.text('新会话 23\n第二行\n第三行'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('$kind: a new user input starts a fresh reading turn', (
      tester,
    ) async {
      final repo = RunTestRepository();
      final container = await mount(tester, repo);
      repo.messages.add(message('short', '上一轮的结论'));
      await refresh(tester, container);
      repo.messages.add(message('next-input', '请继续', user: true));
      await refresh(tester, container);
      repo.messages.add(message('next-answer', longReply('新一轮的回复开头')));
      await refresh(tester, container);
      final top = tester.getTopLeft(reply('新一轮的回复开头')).dy;
      final viewport = tester.getRect(readingList);
      expect(top, inInclusiveRange(viewport.top, viewport.top + 100));
      expect(tester.takeException(), isNull);
    });
  }
}

class _PanelBridge extends RustBridgeRepository {
  final RunTestRepository repo;
  final String conversationId;
  _PanelBridge(this.repo, {this.conversationId = 'conv-1'});

  @override
  bool get supportsChatRuns => repo.supportsChatRuns;
  Conversation conversation() => Conversation(
    id: conversationId,
    messageCount: repo.messages.length,
    createdAt: DateTime(2026, 11, 19),
    updatedAt: DateTime(2026, 11, 19),
  );
  @override
  Future<Conversation> ensureWikiPageChat(String slug) async => conversation();
  @override
  Future<Conversation> ensureGoalChat(String goalId) async => conversation();
  @override
  Future<List<Message>> listMessages(String conversationId) =>
      repo.getMessages(conversationId);
  @override
  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) => repo.sendMessage(conversationId, content);
  @override
  Future<String> generateReply(
    String conversationId, {
    String? providerType,
    String? memoryType,
    int? memoryWindowSize,
  }) => repo.generateReply(conversationId);
}

class _LegacyRepository extends RunTestRepository {
  final gate = Completer<String>();
  @override
  bool get supportsChatRuns => false;
  @override
  Future<String> generateReply(String conversationId) async {
    final content = await gate.future;
    messages.add(message('legacy-answer', content));
    return content;
  }
}
