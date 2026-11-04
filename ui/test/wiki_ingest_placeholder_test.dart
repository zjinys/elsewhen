import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 知识库首页（合并工作区）：上方两张导入入口卡片，点击弹面板。
/// 导入面板已统一为 bottom sheet（与目标/待办/审阅提议同形态），
/// 只测本地校验路径（空输入提示），不发起真实网络/FFI 调用。
void main() {
  testWidgets('首页展示网址/文本导入入口，两面板各有空输入校验', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();
    await tester.pump();

    // 首页（不再有独立「导入」tab）：两张入口卡片
    expect(find.text('网址导入'), findsOneWidget, reason: '首页应有网址导入入口');
    expect(find.text('创建主题'), findsOneWidget, reason: '首页应有创建主题入口');

    // 网址模式面板：输入框 + 抓取按钮；空输入 → 本地校验错误
    await tester.tap(find.text('网址导入'));
    await tester.pump();
    await tester.pumpAndSettle(); // sheet 滑入动画
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('wiki-import-sheet-content')),
        matching: find.byType(TextField),
      ),
      findsOneWidget,
      reason: '网址面板应有输入框',
    );
    expect(find.text('抓取'), findsOneWidget);
    await tester.tap(find.text('抓取'));
    await tester.pump();
    expect(find.text('请先粘贴一个链接'), findsOneWidget);

    // 关闭面板，打开「创建主题」面板
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('创建主题'));
    await tester.pumpAndSettle();
    expect(find.text('保存到知识库'), findsOneWidget, reason: '创建主题面板应有保存按钮');
    await tester.tap(find.text('保存到知识库'));
    await tester.pump();
    expect(find.text('请先输入要保存的内容'), findsOneWidget);
  });

  testWidgets('创建主题面板（bottom sheet）内容区有足够高度容纳编辑器', (tester) async {
    tester.view.physicalSize = const Size(1920, 1080);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      ProviderScope(
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
    await tester.pump();

    await tester.tap(find.text('创建主题'));
    await tester.pumpAndSettle();

    // sheet 限高 0.85 屏：内容区应实际占满可用高度，编辑器不塌缩。
    final size = tester.getSize(
      find.byKey(const ValueKey('wiki-import-sheet-content')),
    );
    expect(size.height, greaterThanOrEqualTo(500), reason: '创建主题是长编辑器，sheet 内容区不应塌缩');

    // 编辑器历史记录有 50ms 防抖 timer，等它落地避免测试结束时报 pending timer。
    await tester.pump(const Duration(milliseconds: 100));
  });

  testWidgets('创建主题面板：空正文编辑器有可聚焦段落，输入能写入文档', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
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
    await tester.pump();

    await tester.tap(find.text('创建主题'));
    await tester.pumpAndSettle();
    expect(find.text('创建主题'), findsWidgets); // 入口卡片 + sheet 标题

    expect(
      tester.widget<WikiContentEditor>(find.byType(WikiContentEditor)).editable,
      isTrue,
    );
    final editorState = tester
        .state<WikiContentEditorState>(find.byType(WikiContentEditor))
        .editorState;
    expect(
      editorState.document.root.children,
      isNotEmpty,
      reason: '空正文必须补一个可编辑段落，否则编辑器没有可聚焦落点',
    );

    final paragraph = editorState.document.root.children.first;
    final transaction = editorState.transaction;
    transaction.insertText(paragraph, 0, '新建主题正文');
    await editorState.apply(transaction);
    // 编辑器历史记录有 50ms 防抖 timer，等它落地避免测试结束时报 pending timer。
    await tester.pump(const Duration(milliseconds: 100));

    expect(wikiDocumentToMarkdown(editorState.document), contains('新建主题正文'));
  });
}
