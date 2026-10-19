part of '../wiki_editor_integration_test.dart';


const _testMd = '# 测试标题\n\n正文段落提到[[人物/张三|张三]]';

/// 带代码块的正文字样（§11 Q4 降级展示）
const _codeMd = '# 带代码\n\n```dart\nfinal x = 1;\n```\n\n尾部段落。';

final _testPage = WikiPage(
  id: 'p1',
  slug: 'topic/测试',
  kind: 'topic',
  title: '测试页面',
  summary: '一句话摘要',
  contentMd: _testMd,
  tags: const ['test'],
  sourceEventIds: const ['ev1'],
  evidenceCount: 1,
  firstSeenAt: DateTime(2025, 1, 1),
  lastSeenAt: DateTime(2025, 1, 2),
  status: 'active',
  createdAt: DateTime(2025, 1, 1),
  updatedAt: DateTime(2025, 1, 2),
  area: 'insight',
);

final _sourcePage = WikiPage(
  id: 'p2',
  slug: 'tweet-1',
  kind: 'source',
  title: '素材推文',
  summary: '素材',
  contentMd: '# 素材\n\n不可编辑',
  tags: const [],
  sourceEventIds: const ['ev2'],
  evidenceCount: 1,
  firstSeenAt: DateTime(2025, 2, 1),
  lastSeenAt: DateTime(2025, 2, 2),
  status: 'active',
  createdAt: DateTime(2025, 2, 1),
  updatedAt: DateTime(2025, 2, 2),
  area: 'imported',
);

final _wikilinkTarget = WikiPage(
  id: 'p3',
  slug: '人物/张三',
  kind: 'person',
  title: '张三',
  summary: '',
  contentMd: '# 张三',
  tags: const [],
  sourceEventIds: const [],
  evidenceCount: 0,
  firstSeenAt: DateTime(2025, 3, 1),
  lastSeenAt: DateTime(2025, 3, 1),
  status: 'active',
  createdAt: DateTime(2025, 3, 1),
  updatedAt: DateTime(2025, 3, 1),
  area: 'network',
);

/// 带代码块正文的页面（M4 §11 Q4）
final _codePage = WikiPage(
  id: 'p4',
  slug: 'topic/带代码',
  kind: 'topic',
  title: '带代码页面',
  summary: '',
  contentMd: _codeMd,
  tags: const [],
  sourceEventIds: const [],
  evidenceCount: 0,
  firstSeenAt: DateTime(2025, 4, 1),
  lastSeenAt: DateTime(2025, 4, 1),
  status: 'active',
  createdAt: DateTime(2025, 4, 1),
  updatedAt: DateTime(2025, 4, 1),
  area: 'insight',
);

// ── main 内 fixture/pump helper 提升为顶层（part 可见，行为不变）──

late _FakeRepo repo;
late ProviderContainer container;

Future<void> pumpDetail(WidgetTester tester, {WikiPage? page}) async {
  final p = page ?? _testPage;
  repo = _FakeRepo(page: p, wikilinkTarget: _wikilinkTarget);
  container = ProviderContainer(
    overrides: [storageRepositoryProvider.overrideWithValue(repo)],
  );
  container.read(wikiOpenTabsProvider.notifier).set([
    ImportTabEntry(),
    PageTabEntry(slug: p.slug, title: p.title),
  ]);
  container.read(wikiActiveTabIdProvider.notifier).set('page-${p.slug}');
  addTearDown(container.dispose);

  tester.view.physicalSize = const Size(1600, 2200);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);

  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        localizationsDelegates: const [
          DefaultMaterialLocalizations.delegate,
          DefaultWidgetsLocalizations.delegate,
          AppFlowyEditorLocalizations.delegate,
        ],
        home: const Scaffold(body: WikiPageDetailView()),
      ),
    ),
  );
  await tester.pump(); // wikiPageProvider 等异步 provider 落地
  await tester.pumpAndSettle();
}


/// 在正文首段落末尾插入文本（模拟用户输入，绕开 IME 层）
Future<void> insertText(WidgetTester tester, String text) async {
  final editorState = tester
      .state<WikiContentEditorState>(find.byType(WikiContentEditor))
      .editorState;
  final firstPara = editorState.document.root.children.first;
  final t = editorState.transaction;
  t.insertText(firstPara, firstPara.delta?.length ?? 0, text);
  await editorState.apply(t);
  await tester.pump();
}

