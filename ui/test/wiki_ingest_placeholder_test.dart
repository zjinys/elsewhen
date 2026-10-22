import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 知识库首页（合并工作区）：上方两张导入入口卡片，点击弹对话框。
/// 只测本地校验路径（空输入提示），不发起真实网络/FFI 调用。
void main() {
  testWidgets('首页展示网址/文本导入入口，两对话框各有空输入校验', (tester) async {
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

    // 网址模式对话框：输入框 + 抓取按钮；空输入 → 本地校验错误
    await tester.tap(find.text('网址导入'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200)); // 对话框动画落地
    expect(
      find.descendant(
        of: find.byType(Dialog),
        matching: find.byType(TextField),
      ),
      findsOneWidget,
      reason: '网址对话框应有输入框',
    );
    expect(find.text('抓取'), findsOneWidget);
    await tester.tap(find.text('抓取'));
    await tester.pump();
    expect(find.text('请先粘贴一个链接'), findsOneWidget);

    // 点 barrier 关闭对话框，打开「创建主题」对话框
    await tester.tapAt(const Offset(5, 5));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200)); // 对话框退场动画
    await tester.tap(find.text('创建主题'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('保存到知识库'), findsOneWidget, reason: '创建主题对话框应有保存按钮');
    await tester.tap(find.text('保存到知识库'));
    await tester.pump();
    expect(find.text('请先输入要保存的内容'), findsOneWidget);
  });

  testWidgets('创建主题对话框在宽屏下是横向比例，不再又高又窄', (tester) async {
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
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    final size = tester.getSize(
      find.byKey(const ValueKey('wiki-import-dialog-content')),
    );
    expect(size.width, 960);
    expect(size.height, lessThanOrEqualTo(680));
    expect(
      size.width / size.height,
      greaterThan(1.3),
      reason: '创建主题对话框应给正文足够的横向行宽',
    );

    // 编辑器历史记录有 50ms 防抖 timer，等它落地避免测试结束时报 pending timer。
    await tester.pump(const Duration(milliseconds: 100));
  });

  testWidgets('创建主题对话框：空正文编辑器有可聚焦段落，输入能写入文档', (tester) async {
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
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byType(Dialog), findsOneWidget);

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
