import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/relation.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

/// 知识页标题的复制能力（本地渲染验证，不触达 FFI）：
/// 1) 头部标题旁有复制按钮，点击后剪贴板拿到的是**标题原文**；
/// 2) 长标题被省略号截断时，按钮仍在标题末尾可见且可点（不被挤掉、不换行丢在别处）；
/// 3) 反馈文案是「标题已复制」，与页面上其他可复制对象（来源链接、路径、正文）区分开。
void main() {
  const slug = 'test/copy-title';
  const title = '一个用 Omni flash 模型生成视频的 prompt';

  /// 打开某一页的详情 tab（页 provider 由假仓库提供，无需 FFI）
  Future<void> openPage(
    WidgetTester tester, {
    required List<MethodCall> clipboardCalls,
    String pageTitle = title,
  }) async {
    final container = ProviderContainer(
      overrides: [
        storageRepositoryProvider.overrideWithValue(
          _FakeWikiRepo(pageTitle: pageTitle),
        ),
      ],
    );
    addTearDown(container.dispose);
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      PageTabEntry(slug: slug, title: pageTitle),
    ]);
    container.read(wikiActiveTabIdProvider.notifier).set('page-$slug');

    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (MethodCall call) async {
        if (call.method == 'Clipboard.setData') clipboardCalls.add(call);
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();
  }

  testWidgets('标题旁有复制按钮，点击后剪贴板拿到标题原文', (tester) async {
    final calls = <MethodCall>[];
    await openPage(tester, clipboardCalls: calls);

    // 头部标题在。注意标题在页面上出现两次：tab 栏（12.5px）与详情头（23px）。
    // 按字号锁定详情头那一个，比 findsOneWidget 更能说明「复制按钮贴着的是哪个」。
    expect(_headerTitle(tester, title), findsOneWidget, reason: '详情头应显示页面标题');

    // 标题旁有一个复制图标（页面上此 tab 内应只有这一个 Icons.copy）
    expect(find.byIcon(Icons.copy), findsOneWidget, reason: '标题旁应有复制图标');

    await tester.tap(find.byIcon(Icons.copy));
    await tester.pump();

    expect(calls, hasLength(1), reason: '点击应触发一次写剪贴板');
    final copied = calls.single.arguments as Map<Object?, Object?>;
    expect(copied['text'], title, reason: '复制的应是标题原文，而不是 slug 或整页内容');

    // 反馈文案与页面上其他可复制对象区分开
    expect(find.text('标题已复制'), findsOneWidget);
  });

  testWidgets('长标题被截断时，复制按钮仍贴在标题末尾且可点', (tester) async {
    // 窄屏 + 超长标题，逼出省略号
    tester.view.physicalSize = const Size(420, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    const longTitle =
        '这是一个刻意写得非常非常长的知识页标题用来验证复制按钮在文本被省略号截断之后'
        '依然紧贴标题末尾并且仍然可以被点击而不换行跑到别的地方去';
    final calls = <MethodCall>[];
    await openPage(tester, clipboardCalls: calls, pageTitle: longTitle);

    // 标题确实溢出（文本被截断，不是完整显示）
    final titleText = tester.widget<Text>(_headerTitle(tester, longTitle));
    expect(titleText.overflow, TextOverflow.ellipsis);
    expect(titleText.maxLines, 1);

    // 按钮仍在树里，且与标题同处一个 Row（贴末尾而非独立换行）。
    // 比的是祖先 Row 列表本身是否相同，所以取 evaluate() 后逐元素比。
    expect(find.byIcon(Icons.copy), findsOneWidget);
    final iconRows = find
        .ancestor(of: find.byIcon(Icons.copy), matching: find.byType(Row))
        .evaluate();
    final titleRows = find
        .ancestor(
          of: _headerTitle(tester, longTitle),
          matching: find.byType(Row),
        )
        .evaluate();
    expect(iconRows, isNotEmpty, reason: '复制按钮应在某个 Row 内');
    expect(
      iconRows.first,
      same(titleRows.first),
      reason: '复制按钮应与标题同处标题那一行 Row（紧贴末尾），而非头部 Wrap 的独立子节点',
    );

    await tester.tap(find.byIcon(Icons.copy));
    await tester.pump();
    final copied = calls.single.arguments as Map<Object?, Object?>;
    expect(copied['text'], longTitle, reason: '应复制完整标题，不是被截断的可见文本');
  });
}

/// 标题在页面上出现两次：tab 栏（12.5px）与详情头（23px）。
/// 按 23px 这个字号锁定详情头的那一个。
Finder _headerTitle(WidgetTester tester, String title) =>
    find.byWidgetPredicate(
      (w) => w is Text && w.data == title && w.style?.fontSize == 23,
    );

/// 假仓库：只提供详情头所需的页面与空的人物关系（不触达 FFI）
class _FakeWikiRepo extends RustBridgeRepository {
  final String pageTitle;

  _FakeWikiRepo({required this.pageTitle});

  WikiPage get _page => WikiPage(
    id: 'p-copy',
    slug: 'test/copy-title',
    kind: 'source',
    title: pageTitle,
    summary: '',
    contentMd: '正文',
    tags: const [],
    sourceEventIds: const [],
    evidenceCount: 0,
    firstSeenAt: DateTime(2026),
    lastSeenAt: DateTime(2026),
    status: 'active',
    createdAt: DateTime(2026),
    updatedAt: DateTime(2026),
    area: 'imported',
  );

  @override
  Future<WikiPage?> getWikiPage(String slug) async => _page;

  @override
  Future<List<Relation>> listRelationsForPage(String slug) async => const [];

  @override
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async =>
      const [];
}
