part of '../wiki_editor_integration_test.dart';

//! 窄屏走查/脏 tab 关闭确认/编辑冲突
/// 移动端走查、关闭确认与编辑冲突族
void _m3DirtyTabTests() {
    testWidgets('移动端走查（390×844）：窄视口不溢出，编辑器与卡座可用', (tester) async {
      // 单独起容器，视口改为手机尺寸（M4 移动端走查）
      repo = _FakeRepo(page: _testPage, wikilinkTarget: _wikilinkTarget);
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

      tester.view.physicalSize = const Size(390, 844);
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
      await tester.pump();
      await tester.pumpAndSettle();

      // 无溢出/布局异常（RenderFlex overflow 等会在此抛出）
      expect(tester.takeException(), isNull, reason: '窄视口不应有布局溢出');

      // 正文经编辑器渲染；窄屏（<1040）聊天不内联，入口是右下角 FAB。
      // 三个操作按钮在窄视口统一收成纯图标（tooltip 保留，无文字标签）。
      expect(find.byType(WikiContentEditor), findsOneWidget);
      expect(find.text('正文段落提到张三', findRichText: true), findsOneWidget);
      expect(find.byTooltip('编辑正文'), findsOneWidget);
      expect(find.byTooltip('沉浸阅读（Esc 退出）'), findsOneWidget);
      expect(find.byTooltip('正文阅读设置（字体 / 字号 / 行距）'), findsOneWidget);
      expect(find.text('编辑正文'), findsNothing, reason: '窄视口应收成纯图标');
      expect(find.text('阅读设置'), findsNothing, reason: '窄视口应收成纯图标');
      expect(find.byType(WikiAiChatPanel), findsNothing);
      expect(find.byIcon(Icons.auto_awesome_outlined), findsWidgets);
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsOneWidget);
      expect(tester.takeException(), isNull, reason: '窄视口展开浮层不应溢出');
      await tester.tap(find.byTooltip('缩小对话窗口'));
      await tester.pumpAndSettle();

      // 底部卡座在「产出」页签下：窄屏切换页签同样有界（内部自滚动）
      await tester.tap(find.text('产出'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiDerivatives), findsOneWidget);
      expect(tester.takeException(), isNull, reason: '产出页签窄屏不应溢出');

      // 回到内容页签，继续编辑
      await tester.tap(find.text('内容'));
      await tester.pumpAndSettle();

      // 编辑态在窄屏同样可用：进入 → 插入 → 保存
      await tester.tap(find.byTooltip('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '窄屏也编辑');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();
      expect(repo.savedCount, 1, reason: '窄屏保存正常');
      expect(repo.savedContent.single.$2, contains('窄屏也编辑'));
    });

    testWidgets('关闭有未保存修改的 tab：先确认，放弃才关', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '未保存');

      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      expect(find.text('关闭前确认'), findsOneWidget, reason: '脏 tab 关闭应弹确认');

      // 取消：tab 保留
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();
      expect(container.read(wikiActiveTabIdProvider), 'page-topic/测试');

      // 再次关闭并确认放弃
      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      await tester.tap(find.text('放弃修改并关闭'));
      await tester.pumpAndSettle();

      expect(
        container.read(wikiActiveTabIdProvider),
        'import',
        reason: '确认放弃后关闭',
      );
      expect(repo.savedCount, 0, reason: '放弃不触发保存');
    });

    testWidgets('鼠标中键点击脏 tab：弹同款关闭确认', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '中键未保存');

      // 中键点 tab 标题（chip 内 12.5px，区别于页面大标题 26px）
      final chipTitle = find.byWidgetPredicate(
        (w) => w is Text && w.data == '测试页面' && w.style?.fontSize == 12.5,
      );
      expect(chipTitle, findsOneWidget);
      await tester.tap(chipTitle, buttons: kMiddleMouseButton);
      await tester.pumpAndSettle();

      expect(find.text('关闭前确认'), findsOneWidget, reason: '中键关闭脏 tab 应先确认');
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();
      expect(container.read(wikiActiveTabIdProvider), 'page-topic/测试');
    });

    testWidgets('关闭脏 tab 选「保存并关闭」：先保存再关', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '关前保存');

      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      expect(find.text('关闭前确认'), findsOneWidget);

      await tester.tap(find.text('保存并关闭'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '关闭前应先保存');
      expect(repo.savedContent.single.$2, contains('关前保存'));
      expect(
        container.read(wikiActiveTabIdProvider),
        'import',
        reason: '保存成功后关闭',
      );
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
      );
    });

    testWidgets('编辑冲突 → 强制覆盖：跳过乐观锁再存', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '覆盖后台更新');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(find.text('页面已被后台更新'), findsOneWidget, reason: '应弹冲突确认');
      expect(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
        findsOneWidget,
      );
      expect(find.text('重新加载'), findsOneWidget);

      await tester.tap(find.text('强制覆盖'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 2, reason: '冲突一次 + 覆盖一次');
      expect(repo.savedExpectedUpdatedAt[0], isNotNull, reason: '首次携带乐观锁');
      expect(repo.savedExpectedUpdatedAt[1], isNull, reason: '覆盖时跳过乐观锁');
      expect(repo.savedContent[1].$3, 'GUI 编辑（冲突后覆盖）');
      expect(find.text('编辑正文'), findsOneWidget, reason: '覆盖成功退出编辑态');
    });

    testWidgets('编辑冲突 → 取消：留在编辑态、无错误条、改动保留', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '先不存了');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '只有被拒的那一次尝试');
      expect(find.text('完成'), findsOneWidget, reason: '取消后留在编辑态');
      expect(find.textContaining('保存失败'), findsNothing, reason: '取消不是失败');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
        reason: '改动仍未保存，脏标记保留',
      );
    });

    testWidgets('编辑冲突 → 重新加载：放弃本地改动并退出编辑态', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '将被重载丢弃');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('重新加载'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '重载不再写库');
      expect(find.text('编辑正文'), findsOneWidget, reason: '重载后回到浏览态');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '重载视为放弃改动，清脏标记',
      );
    });
}
