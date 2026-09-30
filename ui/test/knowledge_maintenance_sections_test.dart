import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int64List;
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_maintenance_sections.dart';

import 'knowledge_flow_test.dart' show proposal, compiledPage;

class MaintenanceFake extends KnowledgeRepository {
  List<String>? repaired;
  List<String>? resolution;
  final offsets = <int>[];
  @override
  Future<List<api.KnowledgeRepairSource>> repairSources(String slug) async => [
    const api.KnowledgeRepairSource(
      snapshotId: 'a',
      title: '不认可的来源',
      version: 1,
      selected: true,
      eligible: false,
    ),
    const api.KnowledgeRepairSource(
      snapshotId: 'b',
      title: '有效的来源',
      version: 2,
      selected: true,
      eligible: true,
    ),
    const api.KnowledgeRepairSource(
      snapshotId: 'c',
      title: '替代原料',
      version: 1,
      selected: false,
      eligible: true,
    ),
  ];
  @override
  Future<String> repair(String slug, List<String> ids) async {
    repaired = ids;
    return proposal.id;
  }

  @override
  Future<api.KnowledgePageDetails> details(String slug) async =>
      const api.KnowledgePageDetails(
        sourcePages: [],
        outputPages: [],
        sources: [],
        history: [],
        proposals: [proposal],
        issues: [],
        metadata: api.KnowledgeMetadata(
          applicableWhen: '',
          strength: 'reference',
        ),
      );
  @override
  Future<List<api.KnowledgeReviewRecord>> history(
    String slug,
    int offset,
  ) async => [
    api.KnowledgeReviewRecord(
      id: 'history',
      action: 'accepted',
      title: '人工选段修订',
      createdAt: '2026-09-29',
      beforeContent: '修改前的知识',
      originalContent: '原始 AI 建议',
      resultContent: '实际采用的段落',
      originalApplicable: '建议条件',
      resultApplicable: '人工条件',
      selectedParts: Int64List.fromList([0, 2]),
      description: '补充边界',
      revisionId: 'revision-1',
    ),
  ];
  @override
  Future<List<api.WikiPageDto>> resolutionTargets(String fingerprint) async => [
    compiledPage,
  ];
  @override
  Future<List<api.KnowledgeRevisionDto>> revisions(String slug) async => [
    const api.KnowledgeRevisionDto(
      id: 'revision',
      contentMd: '已区分不同适用条件',
      reason: '人工修订',
      createdAt: '2026-09-29',
    ),
  ];
  @override
  Future<void> resolveIssue(
    String fingerprint,
    String slug,
    String revision,
    String note,
  ) async {
    resolution = [fingerprint, slug, revision, note];
  }

  @override
  Future<api.KnowledgeQueuePage> queue(int offset, String? status) async {
    offsets.add(offset);
    return api.KnowledgeQueuePage(
      pending: 50,
      running: 0,
      waiting: 1,
      retry: 0,
      skipped: 0,
      completed: 0,
      total: 51,
      items: [
        api.KnowledgeQueueItem(
          id: 'task-$offset',
          task: 'source-compilation',
          pageSlug: 'source/one',
          title: '排队原料 $offset',
          status: status ?? 'waiting',
          detail: '等待全文阅读',
        ),
      ],
      hasMore: offset == 0,
    );
  }
}

Future<void> mount(
  WidgetTester tester,
  MaintenanceFake fake,
  Widget child,
) async {
  await tester.binding.setSurfaceSize(const Size(380, 1200));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    ProviderScope(
      overrides: [knowledgeRepositoryProvider.overrideWithValue(fake)],
      child: MaterialApp(
        home: Scaffold(body: SingleChildScrollView(child: child)),
      ),
    ),
  );
}

void main() {
  testWidgets('来源修复排除无效依据，空选择不请求模型，替代后就地显示待审建议', (tester) async {
    final fake = MaintenanceFake();
    var changes = 0;
    await mount(
      tester,
      fake,
      KnowledgeSourceRepairSection(
        slug: 'method/one',
        hasEvents: false,
        onChanged: () => changes++,
        previewBuilder: (p) => Text('待审预览：${p.title}'),
      ),
    );
    await tester.tap(find.text('修复来源依据'));
    await tester.pumpAndSettle();
    final invalid = tester.widget<CheckboxListTile>(
      find.widgetWithText(CheckboxListTile, '不认可的来源 · v1'),
    );
    expect(invalid.onChanged, isNull);
    expect(invalid.value, isFalse);
    await tester.tap(find.text('有效的来源 · v2'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, '准备修复建议'))
          .onPressed,
      isNull,
    );
    expect(fake.repaired, isNull);
    await tester.tap(find.text('替代原料 · v1'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('准备修复建议'));
    await tester.pumpAndSettle();
    expect(fake.repaired, ['c']);
    expect(changes, 1);
    expect(find.text('待审预览：${proposal.title}'), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('审阅历史展示原始建议和实际保存内容', (tester) async {
    await mount(
      tester,
      MaintenanceFake(),
      const KnowledgeReviewHistorySection(slug: 'method/one'),
    );
    await tester.tap(find.text('处理记录'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('人工选段修订'));
    await tester.pumpAndSettle();
    expect(find.text('采纳的差异段：1、3'), findsOneWidget);
    expect(find.text('保存的适用条件：人工条件'), findsOneWidget);
    await tester.tap(find.text('原始建议'));
    await tester.pumpAndSettle();
    expect(find.textContaining('原始 AI 建议', findRichText: true), findsWidgets);
    await tester.ensureVisible(find.text('实际保存的正文'));
    await tester.tap(find.text('实际保存的正文'));
    await tester.pumpAndSettle();
    expect(find.textContaining('实际采用的段落', findRichText: true), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  testWidgets('跨页解决需要先选择修订并填写处理说明', (tester) async {
    final fake = MaintenanceFake();
    await mount(
      tester,
      fake,
      KnowledgeIssueResolutionSection(
        issue: const api.KnowledgeIssue(
          fingerprint: 'issue',
          pageSlug: 'source/one',
          kind: 'conflict',
          description: '两份资料存在分歧',
        ),
        onChanged: () {},
      ),
    );
    await tester.tap(find.text('关联解决记录'));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(DropdownButtonFormField<String>));
    await tester.pumpAndSettle();
    await tester.tap(find.text(compiledPage.title).last);
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, '确认关联解决'))
          .onPressed,
      isNull,
    );
    expect(fake.resolution, isNull);
    await tester.enterText(find.byType(TextField), '已区分两种适用条件');
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('确认关联解决'));
    await tester.tap(find.text('确认关联解决'));
    await tester.pumpAndSettle();
    expect(fake.resolution, [
      'issue',
      compiledPage.slug,
      'revision',
      '已区分两种适用条件',
    ]);
    expect(find.text('已保存解决记录'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('队列显示未运行的等待任务，支持分页和筛选', (tester) async {
    final fake = MaintenanceFake();
    String? opened;
    await mount(
      tester,
      fake,
      KnowledgeWorkQueueSection(
        onOpenPage: (slug) async {
          opened = slug;
        },
      ),
    );
    await tester.tap(find.text('整理队列'));
    await tester.pumpAndSettle();
    expect(find.textContaining('共 51 项任务'), findsOneWidget);
    expect(find.textContaining('等待全文阅读'), findsOneWidget);
    await tester.tap(find.text('加载更多任务'));
    await tester.pumpAndSettle();
    expect(fake.offsets, [0, 1]);
    await tester.tap(find.text('排队原料 1'));
    expect(opened, 'source/one');
    await tester.tap(find.widgetWithText(ChoiceChip, '待处理'));
    await tester.pumpAndSettle();
    expect(fake.offsets.last, 0);
    expect(find.text('排队原料 1'), findsNothing);
    expect(find.text('手动运行'), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}
