import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/left_sidebar.dart';

void main() {
  testWidgets('empty area keeps filters available and can return to materials',
      (tester) async {
    final now = DateTime(2026, 9, 17);
    final page = WikiPage(
      id: 'source', slug: 'tweet-source', kind: 'source', title: '测试素材',
      summary: '', contentMd: '原文', tags: const [], sourceEventIds: const [],
      evidenceCount: 1, firstSeenAt: now, lastSeenAt: now, status: 'active',
      createdAt: now, updatedAt: now, area: 'imported',
    );
    await tester.pumpWidget(ProviderScope(
      overrides: [
        sidebarTabProvider.overrideWith((ref) => SidebarTab.wiki),
        wikiPagesProvider.overrideWith((ref) async => [page]),
      ],
      child: const MaterialApp(home: Scaffold(body: LeftSidebar())),
    ));
    await tester.pumpAndSettle();
    expect(find.text('测试素材'), findsOneWidget);
    await tester.ensureVisible(find.text('人物/项目 0'));
    await tester.tap(find.text('人物/项目 0'));
    await tester.pumpAndSettle();
    expect(find.text('测试素材'), findsNothing);
    expect(find.text('人物/项目 0'), findsOneWidget);
    await tester.ensureVisible(find.text('素材库 1'));
    await tester.tap(find.text('素材库 1'));
    await tester.pumpAndSettle();
    expect(find.text('测试素材'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
