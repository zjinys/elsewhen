import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/todo_provider.dart';
import 'package:elsewhen_ui/widgets/chat_companion_workspace.dart';
import 'package:elsewhen_ui/widgets/chat_proposal_review.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_goal_repo.dart';

void main() {
  for (final locale in [const Locale('zh'), const Locale('en')]) {
    testWidgets(
      '${locale.languageCode}: both drafts survive opening, resize and closing',
      (tester) async {
        tester.view.devicePixelRatio = 1;
        tester.view.physicalSize = const Size(1200, 900);
        addTearDown(tester.view.resetDevicePixelRatio);
        addTearDown(tester.view.resetPhysicalSize);
        final container = ProviderContainer();
        addTearDown(container.dispose);
        final input = TextEditingController();
        addTearDown(input.dispose);
        await tester.pumpWidget(
          UncontrolledProviderScope(
            container: container,
            child: MaterialApp(
              locale: locale,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              supportedLocales: AppLocalizations.supportedLocales,
              home: Scaffold(
                body: ChatCompanionWorkspace(
                  child: Column(
                    children: [
                      Builder(
                        builder: (context) => TextButton(
                          onPressed: () => showChatCompanion(
                            context,
                            id: 'test-panel',
                            title: (_) => 'Panel',
                            builder: (_, _) =>
                                const TextField(key: ValueKey('panel-draft')),
                          ),
                          child: const Text('Open'),
                        ),
                      ),
                      TextField(
                        key: const ValueKey('chat-draft'),
                        controller: input,
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
        await tester.enterText(
          find.byKey(const ValueKey('chat-draft')),
          'Keep my draft',
        );
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        expect(
          find.byKey(const ValueKey('companion-modal-barrier')),
          findsNothing,
        );
        await tester.enterText(
          find.byKey(const ValueKey('panel-draft')),
          'Review notes',
        );
        await tester.enterText(
          find.byKey(const ValueKey('chat-draft')),
          'Still typing',
        );
        expect(find.text('Panel'), findsOneWidget);
        tester.view.physicalSize = const Size(390, 900);
        await tester.pumpAndSettle();
        expect(
          find.byKey(const ValueKey('companion-modal-barrier')),
          findsOneWidget,
        );
        expect(find.text('Review notes'), findsOneWidget);
        expect(input.text, 'Still typing');
        tester.view.physicalSize = const Size(1200, 900);
        await tester.pumpAndSettle();
        expect(
          find.byKey(const ValueKey('companion-modal-barrier')),
          findsNothing,
        );
        expect(find.text('Review notes'), findsOneWidget);
        container
            .read(selectedConversationIdProvider.notifier)
            .set('another-conversation');
        await tester.pumpAndSettle();
        expect(find.text('Panel'), findsNothing);
        expect(input.text, 'Still typing');
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const ValueKey('chat-draft')));
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        await tester.pumpAndSettle();
        expect(find.text('Panel'), findsNothing);
        expect(input.text, 'Still typing');
        // Repeating the same entry closes it rather than stacking another panel.
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        expect(find.text('Panel'), findsNothing);
        await tester.tap(find.text('Open'));
        await tester.pumpAndSettle();
        tester.view.physicalSize = const Size(390, 900);
        await tester.pumpAndSettle();
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        await tester.pumpAndSettle();
        expect(find.text('Panel'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'another route uses its own sheet while the chat host stays mounted',
    (tester) async {
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            home: Scaffold(
              body: ChatCompanionWorkspace(
                child: Builder(
                  builder: (context) => TextButton(
                    onPressed: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (context) => Scaffold(
                          body: TextButton(
                            onPressed: () => showChatCompanion(
                              context,
                              id: 'external',
                              title: (_) => 'External',
                              builder: (_, _) => const Text('External content'),
                            ),
                            child: const Text('External open'),
                          ),
                        ),
                      ),
                    ),
                    child: const Text('Next route'),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Next route'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('External open'));
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet), findsOneWidget);
      expect(find.text('External content'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('real MessageArea sends while todo panel remains open', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 900);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
    final repo = _WorkspaceRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          storageRepositoryProvider.overrideWithValue(repo),
          conversationRepositoryProvider.overrideWithValue(
            ConversationRepository(repo),
          ),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conversation'),
          ),
          activeGoalsProvider.overrideWith((ref) async => []),
          todosProvider.overrideWith((ref) async => []),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();
    final chat = find.byWidgetPredicate(
      (w) => w is TextField && w.decoration?.hintText == '输入消息...',
    );
    await tester.enterText(chat, 'Do not lose this');
    openTodoDialog(tester.element(find.byType(MessageArea)));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('companion-modal-barrier')), findsNothing);
    expect(find.text('Do not lose this'), findsOneWidget);
    await tester.tap(chat);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(
      repo.messages.where((m) => m.isUser).single.content,
      'Do not lose this',
    );
    expect(find.byTooltip('关闭'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('review remains inline and closing preserves the chat route', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 900);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
    final repo = _WorkspaceRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(
            ConversationRepository(repo),
          ),
        ],
        child: MaterialApp(
          home: Scaffold(
            body: ChatCompanionWorkspace(
              child: Column(
                children: [
                  const Text('Chat still here'),
                  const ChatProposalReviewButton(
                    conversationId: 'conversation',
                  ),
                  const TextField(),
                ],
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(ChatProposalReviewButton));
    await tester.pumpAndSettle();
    expect(find.byType(ChatProposalReviewDialog), findsOneWidget);
    expect(find.byKey(const ValueKey('companion-modal-barrier')), findsNothing);
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    expect(find.text('Chat still here'), findsOneWidget);
    expect(find.byType(ChatProposalReviewDialog), findsNothing);
    expect(tester.takeException(), isNull);
  });
}

class _WorkspaceRepo extends FakeGoalChatRepo {
  @override
  Future<Message> submitConversationInput(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) => sendMessage(conversationId, 'user', content);
}
