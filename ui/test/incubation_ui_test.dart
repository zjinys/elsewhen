import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_panel.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

import 'knowledge_flow_test.dart' show FakeKnowledge;

/// 孵化闭环 Flutter 侧（提案 2026-10-19 UI 段）：判据口径、详情页动作、
/// 已定论徽标。分组渲染依赖私有侧栏组件，判据由 [isIncubatingPage] 单一
/// 口径承载并在此直接单测。
void main() {
  const incubating = api.WikiPageDto(
    id: 'page',
    slug: 'source/one',
    kind: 'source',
    title: '原文',
    summary: '',
    contentMd: '新版本',
    tags: [],
    sourceEventIds: [],
    evidenceCount: 0,
    firstSeenAt: '2026-09-29',
    lastSeenAt: '2026-09-29',
    status: 'active',
    createdAt: '2026-09-29',
    updatedAt: '2026-09-29',
    area: 'imported',
  );

  Future<void> pump(WidgetTester tester, KnowledgeRepository repo, api.WikiPageDto page) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [knowledgeRepositoryProvider.overrideWithValue(repo)],
        child: MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: KnowledgePageActions(
                page: WikiPage.fromDto(page),
                onShowSources: () {},
                onShowOutputs: () {},
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
  }

  testWidgets('判据口径：素材待定论才是孵化中，已定论与非素材都不是', (tester) async {
    expect(isIncubatingPage('source', null), isTrue);
    expect(isIncubatingPage('note', null), isTrue);
    expect(isIncubatingPage('source', 'endorse'), isFalse);
    expect(isIncubatingPage('note', 'reject'), isFalse);
    // 非素材 kind 恒 false（insight/派生稿没有孵化态）
    expect(isIncubatingPage('method', null), isFalse);
    expect(isIncubatingPage('insight', null), isFalse);
  });

  testWidgets('孵化中的素材页显示纳入/丢弃动作；纳入调用 endorse', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, incubating);
    expect(find.text('纳入知识'), findsOneWidget);
    expect(find.text('丢弃'), findsOneWidget);
    expect(repo.decisionCall, isNull);
    await tester.tap(find.text('纳入知识'));
    await tester.pumpAndSettle();
    expect(repo.decisionCall, ('endorse', ''));
    expect(repo.decisionCall, isNotNull);
  });

  testWidgets('丢弃需要原因输入，空原因不提交', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, incubating);
    await tester.tap(find.text('丢弃'));
    await tester.pumpAndSettle();
    // 空原因确认 = 放弃，不调用
    await tester.tap(find.text('丢弃').last);
    await tester.pumpAndSettle();
    expect(repo.decisionCall, isNull);
    // 重新打开输入原因
    await tester.tap(find.byIcon(Icons.block_outlined));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '来源内容自相矛盾');
    await tester.tap(find.text('丢弃').last);
    await tester.pumpAndSettle();
    expect(repo.decisionCall, ('reject', '来源内容自相矛盾'));
  });

  testWidgets('手动重整理按钮调用 reconsolidate 并原位显示结果', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, incubating);
    // 观测行来自 details(fake 返回 2026-10-19 一条新证据)
    expect(find.textContaining('最近整理'), findsOneWidget);
    expect(find.textContaining('新增证据 1 条'), findsOneWidget);
    await tester.tap(find.byIcon(Icons.autorenew));
    await tester.pumpAndSettle();
    expect(repo.reconsolidations, ['source/one']);
    expect(find.textContaining('已重整'), findsOneWidget);
  });

  testWidgets('相关页面行：chip 渲染、人工边可删、派生边只读', (tester) async {
    final repo = FakeKnowledge();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [knowledgeRepositoryProvider.overrideWithValue(repo)],
        child: const MaterialApp(
          home: Scaffold(body: SingleChildScrollView(child: RelatedPagesRow(slug: 'source/one'))),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    // details.relations 带一条人工边
    expect(find.text('相关页面'), findsOneWidget);
    expect(find.text('咖啡豆'), findsOneWidget);
    expect(find.text('相关'), findsOneWidget);
    // 人工边：删除按钮存在并会触发 repo 调用
    await tester.tap(find.byIcon(Icons.close));
    await tester.pump();
    expect(repo.relatedRemoved, ['rel-1']);
    expect(repo.relatedAdded, isEmpty);
  });

  testWidgets('已定论显示状态徽标，不再出现纳入/丢弃动作', (tester) async {
    final repo = FakeKnowledge();
    await pump(
      tester,
      repo,
      const api.WikiPageDto(
        id: 'page', slug: 'source/one', kind: 'source', title: '原文',
        summary: '', contentMd: '', tags: [], sourceEventIds: [],
        evidenceCount: 0, firstSeenAt: '2026-09-29', lastSeenAt: '2026-09-29',
        status: 'active', createdAt: '2026-09-29', updatedAt: '2026-09-29',
        area: 'imported', opinion: 'endorse',
      ),
    );
    expect(find.text('已纳入知识'), findsOneWidget);
    expect(find.text('纳入知识'), findsNothing);
    expect(find.text('丢弃'), findsNothing);
    expect(repo.decisionCall, isNull);
  });
}
