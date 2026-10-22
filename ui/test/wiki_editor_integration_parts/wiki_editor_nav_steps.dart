part of '../wiki_editor_integration_test.dart';

//! 链接跳转与降级
/// wikilink 跳转 + code 块降级
void _m3NavTests() {
  testWidgets('wikilink 点击：跳转目标页 tab', (tester) async {
    await pumpDetail(tester);

    // 段落渲染为纯文本「正文段落提到张三」，其中「张三」是 wikilink span
    final richTextFinder = find.byWidgetPredicate(
      (w) => w is RichText && w.text.toPlainText() == '正文段落提到张三',
    );
    expect(richTextFinder, findsOneWidget);
    final renderParagraph = tester.renderObject<RenderParagraph>(
      richTextFinder,
    );
    final localBox = renderParagraph
        .getBoxesForSelection(
          const TextSelection(baseOffset: 6, extentOffset: 8),
        )
        .first;
    final center = renderParagraph.localToGlobal(
      Offset(
        (localBox.left + localBox.right) / 2,
        (localBox.top + localBox.bottom) / 2,
      ),
    );
    await tester.tapAt(center);
    await tester.pumpAndSettle();

    expect(repo.wikilinkLookups, contains('人物/张三'));
    expect(
      container.read(wikiActiveTabIdProvider),
      'page-人物/张三',
      reason: 'wikilink 应打开目标页',
    );
  });

  testWidgets('多个页面 tab 同时挂载时 FAB Hero tag 不冲突', (tester) async {
    await pumpDetail(tester);
    // IndexedStack 会同时挂载所有 page tab；两个页面各有一个 AI FAB。
    // 路由切换时 Hero 会扫描整棵子树，固定 tag 会触发
    // 「multiple heroes share the same tag」。
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      PageTabEntry(slug: _testPage.slug, title: _testPage.title),
      PageTabEntry(slug: _wikilinkTarget.slug, title: _wikilinkTarget.title),
    ]);
    await tester.pumpAndSettle();

    final context = tester.element(find.byType(WikiPageDetailView));
    unawaited(
      Navigator.of(context).push<void>(
        MaterialPageRoute<void>(
          builder: (_) => const Scaffold(body: Text('next')),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));

    expect(tester.takeException(), isNull);
  });

  testWidgets('code 块：详情页渲染只读降级块，复制按钮写剪贴板（§11 Q4）', (tester) async {
    final clipboardLog = <MethodCall>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        clipboardLog.add(call);
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    await pumpDetail(tester, page: _codePage);

    // 经 WikiContentEditor（生产路径）渲染：不再是 30px placeholder
    expect(find.text('placeholder'), findsNothing);
    expect(find.text('final x = 1;', findRichText: true), findsOneWidget);
    expect(find.text('dart'), findsOneWidget, reason: '语言角标');
    expect(find.text('复制'), findsOneWidget);

    // 点复制：Clipboard.setData 收到代码全文
    await tester.tap(find.text('复制'));
    await tester.pumpAndSettle();
    final copyCall = clipboardLog
        .where((c) => c.method == 'Clipboard.setData')
        .lastOrNull;
    expect(copyCall, isNotNull, reason: '应调用 Clipboard.setData');
    expect(
      (copyCall!.arguments as Map)['text'],
      'final x = 1;',
      reason: '复制内容应去掉围栏与语言行',
    );
  });
}
