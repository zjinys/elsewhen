// M2 §5.4 spike：editable:false 只读展示走查。
// 验证：嵌套列表 / 表格 / 引用 / 待办 / 分割线 / wikilink 在只读编辑器里正常渲染；
// 判定 code 块当前的展示降级（vendor 无 code 块组件 → 30px placeholder 占位，见 §11）；
// 只读下仍可点选正文（复制场景）。
import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_code_block.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:elsewhen_ui/wiki/wiki_text_span_decorator.dart';

/// 覆盖主要块类型的只读展示样例（表格/代码块放前面，避免被视口截断）
const _richMd = r'''
# 一级标题

```dart
final x = 1;
```

正文段落，支持 **加粗** 与 [[topic/投资|投资笔记]]。

## 二级标题

- 第一条
  - 嵌套一
    - 嵌套二
- 第二条

1. 第一步
2. 第二步

> 这是一段引用

- [ ] 未完成事项
- [x] 已完成事项

---

| 列A | 列B |
| --- | --- |
| 甲 | 乙 |
''';

void main() {
  group('wiki 只读展示（editable:false）', () {
    testWidgets('富文本样例渲染不崩 + 关键块样式到位', (tester) async {
      tester.view.physicalSize = const Size(1600, 1600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      final editorState = EditorState(
        document: wikiMarkdownToDocument(_richMd),
      );

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: const [
            DefaultMaterialLocalizations.delegate,
            DefaultWidgetsLocalizations.delegate,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: Scaffold(
            body: AppFlowyEditor(
              editorState: editorState,
              editable: false,
              autoFocus: false,
              editorStyle: EditorStyle.desktop(
                textSpanDecorator: wikiTextSpanDecorator(onTapWikiLink: (_) {}),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      // 标题 / 引用 / 待办 / 表格单元格文本都在只读态展示
      expect(find.text('一级标题', findRichText: true), findsOneWidget);
      expect(find.text('二级标题', findRichText: true), findsOneWidget);
      expect(find.text('这是一段引用', findRichText: true), findsOneWidget);
      expect(find.text('未完成事项', findRichText: true), findsOneWidget);
      expect(find.text('已完成事项', findRichText: true), findsOneWidget);
      expect(
        find.text('甲', findRichText: true),
        findsOneWidget,
        reason: '表格单元格应在只读态渲染',
      );
      expect(find.text('乙', findRichText: true), findsOneWidget);
      expect(
        find.text('第一条', findRichText: true),
        findsOneWidget,
        reason: '嵌套列表根项展示',
      );
      expect(
        find.text('嵌套一', findRichText: true),
        findsOneWidget,
        reason: '嵌套子列表展示',
      );

      // 分割线块
      expect(find.byType(Divider), findsWidgets, reason: '分割线应渲染');
    });

    testWidgets('wikilink 在只读态沿用同款视觉（accent + 下划线）', (tester) async {
      tester.view.physicalSize = const Size(1600, 1600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      final editorState = EditorState(
        document: wikiMarkdownToDocument(_richMd),
      );

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: const [
            DefaultMaterialLocalizations.delegate,
            DefaultWidgetsLocalizations.delegate,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: Scaffold(
            body: AppFlowyEditor(
              editorState: editorState,
              editable: false,
              autoFocus: false,
              editorStyle: EditorStyle.desktop(
                textSpanDecorator: wikiTextSpanDecorator(onTapWikiLink: (_) {}),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      final richText = find.byWidgetPredicate(
        (w) => w is RichText && w.text.toPlainText().contains('投资笔记'),
      );
      expect(richText, findsOneWidget);
      final span = _findSpan(tester.widget<RichText>(richText).text, '投资笔记');
      expect(span, isNotNull);
      expect(span!.style?.color, AppTheme.accentPrimary);
      expect(span.style?.decoration, TextDecoration.underline);
    });

    testWidgets('code 块：生产编辑器注册降级组件，只读展示 + 复制按钮（§11 Q4）', (tester) async {
      tester.view.physicalSize = const Size(1600, 1600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      final editorState = EditorState(
        document: wikiMarkdownToDocument(_richMd),
      );

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: const [
            DefaultMaterialLocalizations.delegate,
            DefaultWidgetsLocalizations.delegate,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: Scaffold(
            body: AppFlowyEditor(
              editorState: editorState,
              editable: false,
              autoFocus: false,
              // 与 WikiContentEditor 相同的注册集：code 节点走降级组件
              blockComponentBuilders: {
                ...standardBlockComponentBuilderMap,
                WikiCodeBlockKeys.type: WikiCodeBlockComponentBuilder(),
              },
            ),
          ),
        ),
      );
      await tester.pump();

      // 不再渲染 vendor 的 placeholder 占位；代码以等宽文本呈现
      expect(find.text('placeholder'), findsNothing);
      expect(find.text('final x = 1;', findRichText: true), findsOneWidget);
      expect(find.text('dart'), findsOneWidget, reason: '语言角标');
      expect(find.text('复制'), findsOneWidget);
      expect(find.byIcon(Icons.copy_rounded), findsOneWidget);
    });

    testWidgets('只读态仍可点选正文（不阻塞复制场景）', (tester) async {
      tester.view.physicalSize = const Size(1600, 1600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      final editorState = EditorState(
        document: wikiMarkdownToDocument(_richMd),
      );

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: const [
            DefaultMaterialLocalizations.delegate,
            DefaultWidgetsLocalizations.delegate,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: Scaffold(
            body: AppFlowyEditor(
              editorState: editorState,
              editable: false,
              autoFocus: false,
            ),
          ),
        ),
      );
      await tester.pump();

      // 段落含加粗+wikilink，整段拼接文本 ≠ 精确目标 → 用 contains 谓词定位
      final paragraphFinder = find.byWidgetPredicate(
        (w) => w is RichText && w.text.toPlainText().contains('正文段落'),
      );
      expect(paragraphFinder, findsOneWidget);
      await tester.tapAt(tester.getCenter(paragraphFinder));
      await tester.pump();

      // 段落路径由文档结构运行时计算，避免 fixture 调整时断言漂移
      final doc = wikiMarkdownToDocument(_richMd);
      final paraIndex = doc.root.children.indexWhere(
        (n) => n.delta?.toPlainText().contains('正文段落') ?? false,
      );

      final selection = editorState.selection;
      expect(selection, isNotNull, reason: '只读态点击正文应产生选区');
      expect(selection!.start.path, [paraIndex], reason: '选区应落在正文段落');
    });
  });
}

/// 在 InlineSpan 树中找文本为 [text] 的 TextSpan（用于样式断言）。
TextSpan? _findSpan(InlineSpan root, String text) {
  if (root is TextSpan) {
    if (root.text == text) {
      return root;
    }
    for (final child in root.children ?? const <InlineSpan>[]) {
      final hit = _findSpan(child, text);
      if (hit != null) {
        return hit;
      }
    }
  }
  return null;
}
