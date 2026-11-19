import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

import 'support/localizations.dart';

void main() {
  Message message(String content) => Message(
    id: 'brief',
    conversationId: 'main',
    role: MessageRole.assistant,
    content: content,
    createdAt: DateTime(2026, 11, 19),
    kind: 'scout_brief',
  );
  testWidgets('legacy unread volume does not interrupt the conversation', (
    tester,
  ) async {
    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(
          localizationsDelegates: appTestLocalizationDelegates,
          locale: const Locale('zh'),
          supportedLocales: const [Locale('zh'), Locale('en')],
          home: Scaffold(
            body: ScoutBriefBubble(
              message: message('🛰 情报速览｜1530 条未读情报\n\ngithub-rust: 128 变'),
            ),
          ),
        ),
      ),
    );
    expect(find.textContaining('1530'), findsNothing);
    expect(find.byType(InkWell), findsNothing);
  });
  testWidgets('brief shows at most three named signals and opens discovery', (
    tester,
  ) async {
    final data = {
      'items': List.generate(
        5,
        (i) => {
          'title': 'Product $i',
          'currency': 'USD',
          'reason': {
            'kind': 'price_change',
            'from': 1000,
            'to': 2000,
            'from_at': '2026-11-18T00:00:00Z',
            'to_at': '2026-11-19T00:00:00Z',
          },
        },
      ),
    };
    final container = ProviderContainer();
    addTearDown(container.dispose);
    container
        .read(scoutWorkspaceProvider.notifier)
        .set(ScoutWorkspace.activity);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          localizationsDelegates: appTestLocalizationDelegates,
          locale: const Locale('zh'),
          supportedLocales: const [Locale('zh'), Locale('en')],
          home: Scaffold(
            body: ScoutBriefBubble(
              message: message('scout-brief-v2\n${jsonEncode(data)}'),
            ),
          ),
        ),
      ),
    );
    expect(find.text('值得关注'), findsOneWidget);
    expect(find.text('Product 0'), findsOneWidget);
    expect(find.text('Product 2'), findsOneWidget);
    expect(find.text('Product 3'), findsNothing);
    expect(find.textContaining('USD 10.00 → 20.00'), findsNWidgets(3));
    await tester.tap(find.text('Product 0'));
    await tester.pump();
    expect(container.read(sidebarTabProvider), SidebarTab.scout);
    expect(container.read(scoutWorkspaceProvider), ScoutWorkspace.discover);
    expect(container.read(scoutProductViewProvider), 'signals');
    expect(tester.takeException(), isNull);
  });
}
