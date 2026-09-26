// M2 §5.2 `[[wikilink]]` spike：行内 wikilink 属性的编解码收敛 +
// AppFlowyEditor 渲染样式与点击回调（slug 跳转接线）。
import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:elsewhen_ui/wiki/wiki_text_span_decorator.dart';

void main() {
  group('wiki codec · wikilink 编解码', () {
    test('[[target|alias]] → 行内 wikilink 属性 → 编码还原', () {
      final doc = wikiMarkdownToDocument('见 [[person/刘庆霖|刘庆霖]] 档案。');
      final delta = doc.root.children.first.delta!;

      final wikilink = delta.whereType<TextInsert>().firstWhere(
        (insert) =>
            insert.attributes?.containsKey(BuiltInAttributeKey.wikilink) ??
            false,
      );
      expect(wikilink.text, '刘庆霖');
      expect(wikilink.attributes![BuiltInAttributeKey.wikilink], 'person/刘庆霖');

      expect(wikiDocumentToMarkdown(doc), '见 [[person/刘庆霖|刘庆霖]] 档案。\n');
    });

    test('alias == target 时省略 [[target]]', () {
      final doc = wikiMarkdownToDocument('[[项目A]]');
      expect(wikiDocumentToMarkdown(doc), '[[项目A]]\n');
    });

    test('列表项内的 wikilink 也保持往返', () {
      final md = '- 相关：[[topic/投资|投资笔记]]\n- 复述：[[项目A]]\n';
      expect(wikiDocumentToMarkdown(wikiMarkdownToDocument(md)), md);
    });
  });

  group('AppFlowyEditor 渲染与点击', () {
    testWidgets('wikilink span 渲染视觉 + 点击回调 slug', (tester) async {
      String? tapped;
      final editorState = EditorState(
        document: wikiMarkdownToDocument('[[topic/投资|投资笔记]]'),
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
              editable: true,
              autoFocus: false,
              editorStyle: EditorStyle.desktop(
                textSpanDecorator: wikiTextSpanDecorator(
                  onTapWikiLink: (slug) => tapped = slug,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      // 正文整段即 wikilink：检查渲染出的 TextSpan 样式与手势
      final richTextFinder = find.byWidgetPredicate(
        (w) => w is RichText && w.text.toPlainText() == '投资笔记',
      );
      expect(richTextFinder, findsOneWidget);
      final rootSpan = tester.widget<RichText>(richTextFinder).text;
      final span = _findSpan(rootSpan, '投资笔记');
      expect(span, isNotNull);
      expect(span!.style?.color, AppTheme.accentPrimary);
      expect(span.style?.decoration, TextDecoration.underline);
      expect(span.style?.fontWeight, FontWeight.w600);
      expect(span.recognizer, isA<TapGestureRecognizer>());

      // 点击 wikilink 文本中心 → 回调 slug
      // getBoxesForSelection 返回的是段落局部坐标，须 localToGlobal 转全局
      final renderParagraph = tester.renderObject<RenderParagraph>(
        richTextFinder,
      );
      final localBox = renderParagraph
          .getBoxesForSelection(
            const TextSelection(baseOffset: 0, extentOffset: 4),
          )
          .first;
      final center = renderParagraph.localToGlobal(
        Offset(
          (localBox.left + localBox.right) / 2,
          (localBox.top + localBox.bottom) / 2,
        ),
      );
      await tester.tapAt(center);
      await tester.pump();
      expect(tapped, 'topic/投资');
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
