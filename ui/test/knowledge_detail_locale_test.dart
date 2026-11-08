import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/models/relation.dart';
import 'package:elsewhen_ui/models/todo.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/providers/todo_provider.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/artifact_versions.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'knowledge_flow_test.dart' show FakeKnowledge;
import 'knowledge_workflows_test.dart' show WorkflowFake;

class _Artifacts extends WorkflowFake {
  final adoptions = <(String, String, bool)>[];
  @override
  Future<List<api.ArtifactVersion>> versions(String slug, int offset) async => [
    for (var i = 1; i <= 2; i++)
      api.ArtifactVersion(
        slug: 'v$i',
        title: '用户作品 $i',
        contentType: '脚本',
        version: i,
        createdAt: '2026-11-07',
        adopted: adopted == 'v$i',
        model: '原始模型',
        strategy: 'strategy-v1',
        instruction: '用户自定义指令 <<body>>',
      ),
  ];
  @override
  Future<void> adopt(String slug, String revision, bool value) async {
    adoptions.add((slug, revision, value));
    await super.adopt(slug, revision, value);
  }
}

class _EntityRepo extends RustBridgeRepository {
  final facts = [
    for (var i = 1; i <= 2; i++)
      api.EntityFactDto(
        id: 'fact-$i',
        entityKind: 'person',
        entitySlug: 'person/locale',
        factText: '用户事实：值 $i',
        occurredAt: '2026-11-07',
        confidence: 4,
        sourceEventId: 'event-$i',
        createdAt: '2026-11-07',
        lastSeenAt: '2026-11-07',
      ),
  ];
  final removed = <String>[];
  final feedback = <String?>[];
  bool failFeedback = false;
  api.EntityMergeStatusDto? merge;
  int undos = 0;
  @override
  Future<WikiPage?> getWikiPage(String slug) async => _page;
  @override
  Future<List<Relation>> listRelationsForPage(String slug) async => [];
  @override
  Future<List<api.EntityFactDto>> listEntityFacts(
    String kind,
    String slug,
  ) async => facts;
  @override
  Future<api.EntityMergeStatusDto?> getEntityMergeStatus(String source) async =>
      merge;
  @override
  Future<List<String>> listEntityAliases(String kind, String slug) async => [];
  @override
  Future<bool> deleteEntityFact(String id) async {
    removed.add(id);
    facts.removeWhere((fact) => fact.id == id);
    return true;
  }

  @override
  Future<List<api.PageGuidanceDto>> listPageGuidance(String slug) async => [
    const api.PageGuidanceDto(
      analysisId: 'analysis',
      eventId: 'original-event',
      recordedAt: '2026-11-07',
      note: '用户原始指导',
      lessons: ['经验原文'],
      followUpsRefined: ['用户后续事项'],
    ),
  ];
  @override
  Future<bool> setGuidanceFeedback({
    required String analysisId,
    required String slug,
    String? note,
    String? feedback,
  }) async {
    this.feedback.add(feedback);
    if (failFeedback) throw Exception('SELECT private /database');
    return true;
  }

  @override
  Future<bool> undoEntityMerge(String source) async {
    undos++;
    merge = null;
    return true;
  }
}

final _page = WikiPage(
  id: 'page',
  slug: 'person/locale',
  kind: 'person',
  title: '用户自己的联系人',
  summary: '',
  contentMd: '原始正文',
  tags: const ['work-item'],
  sourceEventIds: const [],
  evidenceCount: 0,
  firstSeenAt: DateTime(2026),
  lastSeenAt: DateTime(2026),
  status: 'active',
  createdAt: DateTime(2026),
  updatedAt: DateTime(2026),
  area: 'network',
);

Future<ValueNotifier<Locale>> _host(
  WidgetTester tester,
  Widget child, {
  _EntityRepo? entity,
  _Artifacts? artifacts,
}) async {
  await tester.binding.setSurfaceSize(const Size(380, 1100));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  final container = ProviderContainer(
    overrides: [
      knowledgeRepositoryProvider.overrideWithValue(
        artifacts ?? FakeKnowledge(),
      ),
      if (entity != null) storageRepositoryProvider.overrideWithValue(entity),
      todosProvider.overrideWith(
        (ref) async => [
          Todo(
            id: 'todo',
            title: '用户待办原文',
            status: TodoStatus.open,
            priority: 'high',
            relatedWikiSlug: 'person/locale',
            createdAt: DateTime(2026),
            updatedAt: DateTime(2026),
          ),
        ],
      ),
    ],
  );
  addTearDown(container.dispose);
  if (entity != null) {
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      PageTabEntry(slug: _page.slug, title: _page.title),
    ]);
    container.read(wikiActiveTabIdProvider.notifier).set('page-${_page.slug}');
  }
  final locale = ValueNotifier(const Locale('en'));
  addTearDown(locale.dispose);
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: ValueListenableBuilder<Locale>(
        valueListenable: locale,
        builder: (_, value, _) => MaterialApp(
          locale: value,
          localizationsDelegates: const [
            ...AppLocalizations.localizationsDelegates,
            AppFlowyEditorLocalizations.delegate,
          ],
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(body: child),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return locale;
}

void main() {
  testWidgets(
    'artifact comparison and adoption retain original content and exact displayed revision through locale changes',
    (tester) async {
      final repo = _Artifacts();
      final locale = await _host(
        tester,
        const SingleChildScrollView(child: ArtifactVersions(slug: 'source')),
        artifacts: repo,
      );
      expect(find.textContaining('Script · Not adopted'), findsNWidgets(2));
      for (var i = 1; i <= 2; i++) {
        await tester.ensureVisible(find.text('用户作品 $i · v$i'));
        await tester.tap(find.text('用户作品 $i · v$i'));
        await tester.pumpAndSettle();
        await tester.ensureVisible(find.text('Add to comparison').at(i - 1));
        await tester.tap(find.text('Add to comparison').at(i - 1));
        await tester.pumpAndSettle();
      }
      expect(repo.adoptions, isEmpty);
      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('版本比较'), findsOneWidget);
      expect(find.textContaining('原始模型'), findsNWidgets(2));
      await tester.ensureVisible(find.text('采用这个版本').first);
      await tester.tap(find.text('采用这个版本').first);
      await tester.pumpAndSettle();
      expect(repo.adoptions, [('v1', 'revision-v1', true)]);
      expect(find.text('取消采用'), findsOneWidget);
      expect(repo.opened.where((slug) => slug == 'v1'), hasLength(1));
    },
  );

  testWidgets(
    'entity translations keep facts and guidance; cancel protects facts and failed feedback rolls back',
    (tester) async {
      final repo = _EntityRepo()..failFeedback = true;
      final locale = await _host(
        tester,
        const WikiPageDetailView(),
        entity: repo,
      );
      expect(find.text('High priority'), findsOneWidget);
      await tester.ensureVisible(find.widgetWithText(Tab, 'Related'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(Tab, 'Related'));
      await tester.pumpAndSettle();
      expect(find.text('用户事实：值 1'), findsOneWidget);
      await tester.tap(find.byTooltip('Correct by removing this fact').first);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(repo.removed, isEmpty);
      await tester.ensureVisible(find.text('用户原始指导'));
      await tester.tap(find.byTooltip('Helpful (tap again to clear)'));
      await tester.pumpAndSettle();
      expect(repo.feedback, ['helpful']);
      expect(find.byIcon(Icons.thumb_up_outlined), findsOneWidget);
      expect(find.textContaining('SELECT private'), findsNothing);
      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('结构化事实'), findsOneWidget);
      expect(find.text('用户原始指导'), findsOneWidget);
      await tester.ensureVisible(find.byTooltip('纠正：移除此事实').first);
      await tester.tap(find.byTooltip('纠正：移除此事实').first);
      await tester.pumpAndSettle();
      await tester.tap(find.text('移除'));
      await tester.pumpAndSettle();
      expect(repo.removed, ['fact-1']);
      expect(repo.facts.single.factText, '用户事实：值 2');
    },
  );

  testWidgets('narrow merged entity requires explicit undo confirmation', (
    tester,
  ) async {
    final repo = _EntityRepo()
      ..merge = const api.EntityMergeStatusDto(
        sourceSlug: 'person/locale',
        targetSlug: 'person/target',
        entityKind: 'person',
        createdAt: '2026-11-07',
      );
    await _host(tester, const WikiPageDetailView(), entity: repo);
    await tester.ensureVisible(find.widgetWithText(Tab, 'Related'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(Tab, 'Related'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Undo merge'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(repo.undos, 0);
    await tester.tap(find.text('Undo merge'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Undo merge').last);
    await tester.pumpAndSettle();
    expect(repo.undos, 1);
    expect(repo.merge, isNull);
  });
}
