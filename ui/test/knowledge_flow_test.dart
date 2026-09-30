import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/import_fetch.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_import.dart';
import 'package:elsewhen_ui/widgets/knowledge_panel.dart';

const proposal = api.KnowledgeProposal(
  id: 'proposal',
  targetSlug: 'method/one',
  kind: 'method',
  title: '事务方法',
  contentMd: '先明确事务边界。',
  applicableWhen: '多步写入',
  snapshotIds: ['original'],
  eventIds: [],
  reason: '依据原文提炼',
  status: 'pending',
  createdAt: '2026-09-29',
);
const page = api.WikiPageDto(
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

const compiledPage = api.WikiPageDto(
  id: 'compiled',
  slug: 'method/one',
  kind: 'method',
  title: '事务方法',
  summary: '',
  contentMd: '先明确事务边界，再完成多步写入。',
  tags: [],
  sourceEventIds: [],
  evidenceCount: 0,
  firstSeenAt: '2026-09-29',
  lastSeenAt: '2026-09-29',
  status: 'active',
  createdAt: '2026-09-29',
  updatedAt: '2026-09-29',
  area: 'insight',
);

class FakeKnowledge extends KnowledgeRepository {
  final decisions = <bool>[];
  bool fail = false;
  int saves = 0;
  bool returnPage = false;
  int proposeCalls = 0;
  String? expected;
  int detailsCalls = 0;
  List<int>? acceptedParts;
  bool? acceptedApplicability;
  @override
  Future<List<api.KnowledgeDiffPart>> diff(String id) async => [
    const api.KnowledgeDiffPart(before: '旧步骤', after: '新步骤', changed: true),
    const api.KnowledgeDiffPart(before: '共同段落', after: '共同段落', changed: false),
    const api.KnowledgeDiffPart(before: '旧边界', after: '新边界', changed: true),
  ];
  @override
  Future<api.WikiPageDto> acceptParts(
    String id,
    List<int> parts,
    bool applicability,
    List<String> issues,
  ) async {
    acceptedParts = parts;
    acceptedApplicability = applicability;
    return compiledPage;
  }

  @override
  Future<api.KnowledgePageDetails> details(String slug) async {
    detailsCalls++;
    return api.KnowledgePageDetails(
      sourcePages: slug == page.slug ? [] : [page],
      outputPages: slug == page.slug ? [compiledPage] : [],
      sources: [],
      history: [],
      proposals: [],
      issues: [],
      metadata: const api.KnowledgeMetadata(
        applicableWhen: '多步写入',
        strength: 'reference',
      ),
    );
  }

  @override
  Future<List<api.KnowledgeProposal>> proposals() async => [
    proposal,
    const api.KnowledgeProposal(
      id: 'done',
      targetSlug: 'topic/done',
      kind: 'topic',
      title: '自动完成的主题',
      contentMd: '正文',
      applicableWhen: '适用',
      snapshotIds: [],
      eventIds: [],
      reason: '自动',
      status: 'accepted',
      createdAt: '2026-09-29',
    ),
  ];
  @override
  Future<List<api.KnowledgeIssue>> issues() async => [];
  @override
  Future<List<api.KnowledgeBackgroundRunDto>> runs() async => [];
  @override
  Future<api.WikiPageDto?> resolve(String id, bool accept) async {
    decisions.add(accept);
    if (fail) throw Exception('来源已更新，请重新生成建议');
    return accept && returnPage ? page : null;
  }

  @override
  Future<String> propose(String slug, String kind) async {
    proposeCalls++;
    if (fail) throw Exception('Provider failed');
    return 'proposal';
  }

  @override
  Future<api.SourceSnapshot?> snapshot(String id) async =>
      const api.SourceSnapshot(
        id: 'original',
        sourceId: 'source',
        version: 1,
        title: '原始材料',
        contentMd: '可核验的原文内容',
        contentHash: 'hash',
        capturedAt: '2026-09-29',
        sourceKind: 'webpage',
      );
  @override
  Future<api.SourceUpdatePreview> preview(String url, String content) async =>
      const api.SourceUpdatePreview(
        existingSlug: 'source/one',
        previousContent: '旧版本',
        previousSnapshotId: 'version-one',
        changed: true,
      );
  @override
  Future<api.WikiPageDto> confirmSource({
    required String title,
    required String contentMd,
    required String sourceUrl,
    required String sourceKind,
    required List<String> tags,
    String? expectedSnapshotId,
  }) async {
    saves++;
    expected = expectedSnapshotId;
    return page;
  }
}

void main() {
  Future<void> pump(
    WidgetTester tester,
    FakeKnowledge repo,
    Widget child,
  ) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [knowledgeRepositoryProvider.overrideWithValue(repo)],
        child: MaterialApp(
          home: Scaffold(body: SingleChildScrollView(child: child)),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('审阅先看来源，点击确认才调用保存', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, const KnowledgeProposalCard(proposal: proposal));
    expect(repo.decisions, isEmpty);
    await tester.tap(find.text('查看原料'));
    await tester.pumpAndSettle();
    expect(find.text('可核验的原文内容', findRichText: true), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    await tester.tap(find.byTooltip('收起对照'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('确认保存'));
    await tester.pumpAndSettle();
    expect(repo.decisions, [true]);
    expect(tester.takeException(), isNull);
  });
  testWidgets('逐段审阅只提交选择的修改，不增加弹窗', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, const KnowledgeProposalCard(proposal: proposal));
    await tester.tap(find.text('逐段审阅'));
    await tester.pumpAndSettle();
    expect(repo.acceptedParts, isNull);
    await tester.ensureVisible(find.text('采纳修改 3'));
    await tester.tap(find.text('采纳修改 3'));
    await tester.ensureVisible(find.text('同时采纳建议的适用条件'));
    await tester.tap(find.text('同时采纳建议的适用条件'));
    await tester.ensureVisible(find.text('确认保存'));
    await tester.tap(find.text('确认保存'));
    await tester.pumpAndSettle();
    expect(repo.acceptedParts, [0]);
    expect(repo.acceptedApplicability, isFalse);
    expect(find.byType(Dialog), findsNothing);
    expect(tester.takeException(), isNull);
  });
  testWidgets('拒绝建议走拒绝路径', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, const KnowledgeProposalCard(proposal: proposal));
    await tester.tap(find.text('拒绝建议'));
    await tester.pumpAndSettle();
    expect(repo.decisions, [false]);
  });
  testWidgets('来源变化阻止保存并保留可读错误', (tester) async {
    final repo = FakeKnowledge()..fail = true;
    await pump(tester, repo, const KnowledgeProposalCard(proposal: proposal));
    await tester.tap(find.text('确认保存'));
    await tester.pumpAndSettle();
    expect(find.text('来源已更新，请重新生成建议'), findsOneWidget);
    expect(find.text('确认保存'), findsOneWidget);
  });
  testWidgets('采纳后保留当前上下文，不跳到另一个同名页', (tester) async {
    final repo = FakeKnowledge()..returnPage = true;
    await pump(tester, repo, const KnowledgeProposalCard(proposal: proposal));
    final container = ProviderScope.containerOf(
      tester.element(find.byType(KnowledgeProposalCard)),
    );
    final before = container.read(wikiOpenTabsProvider);
    await tester.tap(find.text('确认保存'));
    await tester.pumpAndSettle();
    expect(container.read(wikiOpenTabsProvider), before);
    expect(find.text('已处理，原文保持不变'), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
  });
  testWidgets('两个动作等高居中且有图标，提炼失败可原位重试', (tester) async {
    final repo = FakeKnowledge()..fail = true;
    var sources = 0;
    var outputs = 0;
    await pump(
      tester,
      repo,
      KnowledgePageActions(
        page: WikiPage.fromDto(page),
        onShowSources: () => sources++,
        onShowOutputs: () => outputs++,
      ),
    );
    final sourceText = find.text('来源与修订');
    final refineText = find.text('提炼知识');
    expect(
      (tester.getCenter(sourceText).dy - tester.getCenter(refineText).dy).abs(),
      lessThan(1),
    );
    expect(find.byIcon(Icons.source_outlined), findsOneWidget);
    expect(find.byIcon(Icons.auto_awesome_outlined), findsOneWidget);
    await tester.tap(sourceText);
    expect(sources, 1);
    await tester.tap(refineText);
    await tester.pumpAndSettle();
    await tester.tap(find.text('提炼方法'));
    await tester.pumpAndSettle();
    expect(find.text('重试整理'), findsOneWidget);
    expect(outputs, 0);
    repo.fail = false;
    await tester.tap(find.text('重试整理'));
    await tester.pumpAndSettle();
    expect(repo.proposeCalls, 2);
    expect(repo.decisions, isEmpty);
    expect(outputs, 1);
    expect(find.byType(Dialog), findsNothing);
  });
  testWidgets('已有知识显示在原料产出区，反向关联可见且无需弹窗', (tester) async {
    final repo = FakeKnowledge();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          knowledgeRepositoryProvider.overrideWithValue(repo),
          wikiDerivativesProvider(page.slug).overrideWith((ref) async => []),
        ],
        child: MaterialApp(
          home: Scaffold(
            body: KnowledgeOutputsSection(page: WikiPage.fromDto(page)),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('事务方法'), findsOneWidget);
    expect(find.text('方法 · 来自本页原料'), findsOneWidget);
    final container = ProviderScope.containerOf(
      tester.element(find.byType(KnowledgeOutputsSection)),
    );
    final before = repo.detailsCalls;
    (container.read(
      storageRepositoryProvider,
    ) as RustBridgeRepository).knowledgeRevision.value++;
    await tester.pumpAndSettle();
    expect(repo.detailsCalls, greaterThan(before));
    await tester.tap(find.text('事务方法'));
    await tester.pumpAndSettle();
    expect(find.text('先明确事务边界，再完成多步写入。', findRichText: true), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    await tester.pumpWidget(const SizedBox.shrink());
    await pump(tester, repo, const KnowledgeOriginLinks(slug: 'method/one'));
    expect(find.text('原料 · 原文'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets('审阅只呈现待确认修改，自动完成产出不挤占列表', (tester) async {
    final repo = FakeKnowledge();
    await pump(tester, repo, const KnowledgeMaintenanceButton());
    await tester.tap(find.text('知识审阅'));
    await tester.pumpAndSettle();
    expect(find.text('事务方法'), findsOneWidget);
    expect(find.text('自动完成的主题'), findsNothing);
    expect(find.text('来源与内容检查'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  for (final accept in [false, true]) {
    testWidgets('重复导入${accept ? '确认版本' : '取消不写库'}，窄屏对照无溢出', (tester) async {
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final repo = FakeKnowledge();
      await pump(
        tester,
        repo,
        Consumer(
          builder: (context, ref, _) => TextButton(
            child: const Text('导入'),
            onPressed: () async {
              await saveKnowledgeImport(
                context,
                ref,
                const ImportFetch(
                  sourceUrl: 'https://example.com/a',
                  sourceKind: 'webpage',
                  contentMd: '新版本',
                ),
                [],
              );
            },
          ),
        ),
      );
      await tester.tap(find.text('导入'));
      await tester.pumpAndSettle();
      expect(find.text('旧版本'), findsOneWidget);
      expect(find.text('新版本'), findsOneWidget);
      expect(repo.saves, 0);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text(accept ? '确认保存新版本' : '保留旧版本'));
      await tester.pumpAndSettle();
      expect(repo.saves, accept ? 1 : 0);
      if (accept) expect(repo.expected, 'version-one');
    });
  }
}
