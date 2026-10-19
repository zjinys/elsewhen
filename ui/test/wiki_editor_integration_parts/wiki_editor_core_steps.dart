part of '../wiki_editor_integration_test.dart';

//! 编辑器渲染与保存核心
/// 浏览态渲染/完成保存/Ctrl+S/取消/保存失败
void _m3CoreTests() {
    testWidgets('浏览态：正文经编辑器渲染，聊天块在页尾且仅一个', (tester) async {
      await pumpDetail(tester);

      expect(find.text('测试页面'), findsWidgets);
      expect(find.text('正文段落提到张三', findRichText: true), findsOneWidget);
      expect(find.byType(WikiContentEditor), findsOneWidget);
      expect(
        tester
            .widget<WikiContentEditor>(find.byType(WikiContentEditor))
            .editable,
        isFalse,
        reason: '浏览态应只读',
      );
      // 对话默认缩小，展开后使用浮层，正文不再被侧栏挤窄。
      expect(find.byType(WikiAiChatPanel), findsNothing);
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsOneWidget);
      expect(find.text('AI对话'), findsOneWidget);
      expect(find.byTooltip('缩小对话窗口'), findsOneWidget);
      await tester.tap(find.byTooltip('缩小对话窗口'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsNothing);
      // 编辑入口存在
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('素材页（kind=source）不显示编辑入口', (tester) async {
      await pumpDetail(tester, page: _sourcePage);

      expect(find.text('素材推文'), findsWidgets);
      expect(find.text('编辑正文'), findsNothing, reason: '素材采集页内容不可改，不提供人工编辑入口');
      // 沉浸阅读与可编辑性无关（覆盖层只读），素材页同样要有入口。
      await tester.tap(find.byTooltip('沉浸阅读（Esc 退出）'));
      await tester.pumpAndSettle();
      expect(
        find.byTooltip('退出沉浸阅读（Esc）'),
        findsOneWidget,
        reason: '素材页应能进入沉浸阅读',
      );
      // 正常态正文编辑器仍挂着 + 沉浸覆盖层里的只读编辑器，都必须只读。
      final editors = tester.widgetList<WikiContentEditor>(
        find.byType(WikiContentEditor),
      );
      expect(editors, isNotEmpty);
      expect(editors.every((e) => !e.editable), isTrue, reason: '沉浸覆盖层是只读的');
      await tester.tap(find.byTooltip('退出沉浸阅读（Esc）'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('退出沉浸阅读（Esc）'), findsNothing);
    });

    testWidgets('浮层回复渲染 Markdown，双击展开阅读', (tester) async {
      await pumpDetail(tester);
      repo.chatMessages = [
        Message(
          id: 'reply-1',
          conversationId: 'conv-page',
          role: MessageRole.assistant,
          content: '## 回答标题\n\n**重要内容**',
          createdAt: DateTime(2026, 1, 1),
        ),
      ];
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(MarkdownView), findsWidgets);
      await tester.ensureVisible(find.text('回答标题').first);
      await tester.tap(find.text('回答标题').first);
      await tester.pump(const Duration(milliseconds: 80));
      await tester.tap(find.text('回答标题').first);
      await tester.pumpAndSettle();
      expect(find.text('AI 回复'), findsOneWidget);
      expect(find.byType(Dialog), findsOneWidget);
      expect(find.byType(MarkdownView), findsAtLeastNWidgets(2));
    });

    testWidgets('编辑 → 修改 → 完成：saveWikiPageContent 收到含改动的 markdown，退出编辑态', (
      tester,
    ) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      expect(find.text('完成'), findsOneWidget, reason: '编辑态出现「完成」');
      expect(
        tester
            .widget<WikiContentEditor>(find.byType(WikiContentEditor))
            .editable,
        isTrue,
        reason: '编辑态应可写',
      );

      await insertText(tester, '人工补充内容');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
        reason: '改动后应标记脏',
      );

      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '应只保存一次');
      final (slug, md, reason) = repo.savedContent.single;
      expect(slug, _testPage.slug);
      expect(reason, 'GUI 编辑');
      expect(md, contains('人工补充内容'), reason: '保存的 markdown 应包含人工改动');
      expect(md, isNot(contains('wiki_chat')), reason: '对话块不进 markdown');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '保存后清除脏标记',
      );
      expect(find.text('完成'), findsNothing, reason: '保存成功退出编辑态');
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('无改动点「完成」：不写库直接退出', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 0, reason: '无改动不触发保存');
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('编辑 → 修改 → 取消：丢弃改动，不写库', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '将被丢弃');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
      );

      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 0, reason: '取消不保存');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '取消后清除脏标记',
      );
      expect(find.text('完成'), findsNothing, reason: '退出编辑态');
      // 文档回到加载快照
      final editorState = tester
          .state<WikiContentEditorState>(find.byType(WikiContentEditor))
          .editorState;
      expect(editorState.document.toJson(), isNot(contains('将被丢弃')));
    });

    testWidgets('Ctrl+S 保存并退出编辑态', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '保存快捷键');

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: 'Ctrl+S 应触发保存');
      expect(repo.savedContent.single.$2, contains('保存快捷键'));
      expect(find.text('编辑正文'), findsOneWidget, reason: '保存后退出编辑态');
    });

    testWidgets('保存失败：留在编辑态并展示错误条', (tester) async {
      repo = _FakeRepo(page: _testPage)..saveThrows = true;
      container = ProviderContainer(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
      );
      container.read(wikiOpenTabsProvider.notifier).set([
        ImportTabEntry(),
        PageTabEntry(slug: _testPage.slug, title: _testPage.title),
      ]);
      container
          .read(wikiActiveTabIdProvider.notifier)
          .set('page-${_testPage.slug}');
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
      await tester.pumpAndSettle();

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '保存会失败');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '尝试过一次保存');
      expect(find.textContaining('保存失败'), findsOneWidget, reason: '应展示错误条');
      expect(find.text('完成'), findsOneWidget, reason: '失败后留在编辑态');
    });
}
