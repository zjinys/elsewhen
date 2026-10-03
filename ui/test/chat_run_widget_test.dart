import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/chat_run_provider.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/todo_provider.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/chat_run_status.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';

import 'support/chat_run_fixture.dart';

Future<void> settleFrames(WidgetTester tester) async {
  for (var i = 0; i < 5; i++) {
    await tester.pump();
  }
}

Widget app(RunTestRepository repo, Widget child, {RustBridgeRepository? wikiBridge}) => ProviderScope(
  overrides: [
    storageRepositoryProvider.overrideWithValue(wikiBridge ?? _WikiBridge(repo)),
    conversationRepositoryProvider.overrideWithValue(repo),
    selectedConversationIdProvider.overrideWith(() => StateHolder('conv-1')),
    messagesProvider.overrideWith((ref) async => List.of(repo.messages)),
    wikiPageProvider.overrideWith((ref, slug) async => null),
    wikiPagesProvider.overrideWith((ref) async => []),
    todosProvider.overrideWith((ref) async => []),
    activeGoalsProvider.overrideWith((ref) async => []),
    activeAiProviderProvider.overrideWith((ref) async => null),
    todayTokenUsageProvider.overrideWith((ref) async => null),
  ],
  child: MaterialApp(home: Scaffold(body: child)),
);

void main() {
  testWidgets(
    'safe draft is plain text and tool progress never exposes raw payloads',
    (tester) async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun(
        status: 'cancelled',
        sequence: 4,
        content: '[[unverified source]] is only a draft',
      );
      repo.events.addAll([
        fixtureEvent(1, 'created'),
        fixtureEvent(
          2,
          'tool_started',
          payload: {'call_id': 'a', 'name': 'save_knowledge_draft'},
        ),
        fixtureEvent(
          3,
          'tool_result',
          payload: {
            'call_id': 'a',
            'name': 'save_knowledge_draft',
            'outcome': 'proposal_ready',
            'content': 'PRIVATE_PROTOCOL_SENTINEL',
            'reasoning': 'PRIVATE_REASONING_SENTINEL',
          },
        ),
        fixtureEvent(4, 'cancelled'),
      ]);
      await tester.pumpWidget(
        app(repo, const ChatRunStatus(conversationId: 'conv-1')),
      );
      await settleFrames(tester);
      final container = ProviderScope.containerOf(
        tester.element(find.byType(ChatRunStatus)),
      );
      await container.read(chatRunControllerProvider('conv-1')).poll();
      await settleFrames(tester);
      expect(find.text('已停止生成'), findsOneWidget);
      expect(find.textContaining('未核验草稿'), findsOneWidget);
      expect(find.byType(MarkdownView), findsNothing);
      await tester.tap(find.textContaining('实际工具步骤'));
      await tester.pump(const Duration(milliseconds: 250));
      expect(find.text('草稿已生成，等待审阅；尚未执行'), findsOneWidget);
      expect(find.textContaining('PRIVATE_'), findsNothing);
      expect(find.text('操作已执行（业务回执）'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
      expect(repo.stops, isEmpty);
    },
  );

  for (final wiki in [false, true]) {
    testWidgets(
      '${wiki ? 'wiki' : 'main'} chat starts saved input and exposes durable stop/retry',
      (tester) async {
        final repo = RunTestRepository();
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        final input = find.byType(TextField);
        await tester.enterText(input, 'synthetic question');
        await tester.tap(find.byIcon(Icons.arrow_upward));
        await settleFrames(tester);
        expect(repo.starts.length, 1);
        expect(repo.starts.single.$2, repo.messages.single.id);
        expect(repo.legacyCalls, 0);
        expect(find.text('停止生成'), findsOneWidget);
        expect(find.text('AI 正在思考…'), findsNothing);
        await tester.ensureVisible(find.text('停止生成'));
        await tester.tap(find.text('停止生成'));
        await settleFrames(tester);
        expect(repo.stops, [('conv-1', 'run-1')]);
        expect(find.text('已停止生成'), findsOneWidget);
        await repo.sendMessage('conv-1', 'a later saved input');
        await tester.ensureVisible(find.text('重新生成本条'));
        await tester.tap(find.text('重新生成本条'));
        await settleFrames(tester);
        expect(repo.starts.length, 2);
        expect(repo.starts.last.$2, 'input-1');
        expect(
          repo.messages.length,
          2,
          reason: 'retry must not save another user message',
        );
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
        expect(
          repo.stops.length,
          1,
          reason: 'unmount only detaches the observer',
        );
      },
    );

    testWidgets(
      '${wiki ? 'wiki' : 'main'} saved input without a run stays actionable after reopening',
      (tester) async {
        final repo = RunTestRepository();
        repo.runs['conv-1'] = fixtureRun(
          id: 'older-run',
          status: 'completed',
          input: 'earlier-input',
        );
        final saved = await repo.sendMessage('conv-1', 'saved before closing');
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        final action = find.text(wiki ? '生成回复' : '重新生成');
        expect(action, findsOneWidget);
        await tester.ensureVisible(action);
        await tester.tap(action);
        await settleFrames(tester);
        expect(repo.starts.single.$2, saved.id);
        expect(repo.messages.length, 1);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      },
    );

    testWidgets(
      '${wiki ? 'wiki' : 'main'} final snapshot reloads the persisted answer once',
      (tester) async {
        final repo = RunTestRepository();
        repo.runs['conv-1'] = fixtureRun(content: 'unverified draft');
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        expect(find.text('unverified draft'), findsOneWidget);
        repo.messages.add(
          Message(
            id: 'answer-run-1',
            conversationId: 'conv-1',
            role: MessageRole.assistant,
            content: 'verified answer',
            createdAt: DateTime(2026, 10, 1),
          ),
        );
        repo.runs['conv-1'] = fixtureRun(
          status: 'completed',
          sequence: 2,
          content: 'verified answer',
        );
        final container = ProviderScope.containerOf(
          tester.element(find.byType(ChatRunStatus)),
        );
        await container.read(chatRunControllerProvider('conv-1')).poll();
        await settleFrames(tester);
        expect(find.text('回复已完成'), findsOneWidget);
        expect(find.text('unverified draft'), findsNothing);
        expect(
          find.text('verified answer', findRichText: true),
          findsOneWidget,
        );
        expect(find.text('停止生成'), findsNothing);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      },
    );

    testWidgets(
      '${wiki ? 'wiki' : 'main'} chat restores interrupted run on reopen',
      (tester) async {
        final repo = RunTestRepository();
        repo.runs['conv-1'] = fixtureRun(
          status: 'interrupted',
          input: 'earlier-input',
        );
        await tester.pumpWidget(
          app(
            repo,
            wiki
                ? const WikiAiChatPanel(slug: 'topic/test')
                : const MessageArea(),
          ),
        );
        await settleFrames(tester);
        expect(find.text('生成已中断'), findsOneWidget);
        expect(find.text('重新生成本条'), findsOneWidget);
        expect(repo.starts, isEmpty);
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
        expect(repo.stops, isEmpty);
      },
    );
  }
}

class _WikiBridge extends RustBridgeRepository {
  final RunTestRepository repo;
  _WikiBridge(this.repo);
  @override
  bool get supportsChatRuns => true;
  @override
  Future<Conversation> ensureWikiPageChat(String pageSlug) async =>
      Conversation(
        id: 'conv-1',
        createdAt: DateTime(2026, 10, 1),
        updatedAt: DateTime(2026, 10, 1),
        wikiPageSlug: pageSlug,
        messageCount: repo.messages.length,
      );
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
}
