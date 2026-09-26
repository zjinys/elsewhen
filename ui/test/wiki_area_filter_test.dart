import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

/// 区域筛选从侧栏迁入工作区浏览区（知识库首页下方）后，验证：
/// 空结果时筛选仍可用，清除后可回到素材。
void main() {
  testWidgets(
    'empty area keeps filters available and can return to materials',
    (tester) async {
      final now = DateTime(2026, 9, 17);
      final page = WikiPage(
        id: 'source',
        slug: 'tweet-source',
        kind: 'source',
        title: '测试素材',
        summary: '',
        contentMd: '原文',
        tags: const [],
        sourceEventIds: const [],
        evidenceCount: 1,
        firstSeenAt: now,
        lastSeenAt: now,
        status: 'active',
        createdAt: now,
        updatedAt: now,
        area: 'imported',
      );
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            wikiPagesProvider.overrideWith((ref) async => [page]),
          ],
          child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
        ),
      );
      await tester.pumpAndSettle();

      // 浏览区就位：区域筛选有「素材库」chip；未筛选时只显示引导文案
      expect(find.text('素材库'), findsOneWidget, reason: '区域筛选应有素材库 chip');
      expect(find.text('输入关键词或选择筛选条件开始浏览'), findsOneWidget);

      // 选中「素材库」→ 素材出现
      await tester.tap(find.text('素材库'));
      await tester.pumpAndSettle();
      expect(find.text('测试素材'), findsOneWidget, reason: '选中素材库应显示素材');

      // 空结果：输入不存在的关键词 → 空态提示，但筛选 chip 仍在
      await tester.enterText(find.byType(TextField).first, '不存在的关键词');
      await tester.pump();
      expect(find.text('没有匹配的知识页'), findsOneWidget, reason: '空结果应有提示');
      expect(find.text('素材库'), findsOneWidget, reason: '空结果时筛选仍保留');

      // 清除关键词 → 回到素材
      await tester.tap(find.byIcon(Icons.clear));
      await tester.pump();
      expect(find.text('测试素材'), findsOneWidget, reason: '清除后可回到素材');
      expect(tester.takeException(), isNull);
    },
  );
}