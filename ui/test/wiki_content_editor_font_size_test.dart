import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// 编辑器正文字号跟随「设置 → 正文字号」的回归测试。
///
/// `fontAwareTextStyleConfiguration` 把 fontSize 写入基础 `text` 样式，
/// bold/italic 等组合样式经 combine 继承。本测试验证真实渲染字号：
/// - 传入 18 → 正文渲染字号为 18；
/// - 不传（默认）→ 16（vendor 默认观感）；
/// - 加粗文本同样放大（继承基础字号）。
void main() {
  Widget harness({double? fontSize}) {
    return MaterialApp(
      theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: Builder(
          builder: (context) => AppFlowyEditor(
            editorState: EditorState(
              document: wikiMarkdownToDocument('字号验证正文 **加粗**'),
            ),
            editorStyle: EditorStyle.desktop(
              textStyleConfiguration: fontAwareTextStyleConfiguration(
                Theme.of(context).textTheme.bodyLarge?.fontFamily,
                color:
                    Theme.of(context).textTheme.bodyLarge?.color ??
                    Theme.of(context).textTheme.bodyMedium?.color,
                fontSize: fontSize,
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<double> effectiveSize(WidgetTester tester, double? fontSize) async {
    await tester.pumpWidget(harness(fontSize: fontSize));
    await tester.pump();
    final paragraphs = tester
        .renderObjectList<RenderParagraph>(find.byType(RichText))
        .toList();
    final target = paragraphs.firstWhere(
      (w) => w.text.toPlainText().contains('字号验证正文'),
    );
    return target.text.style!.fontSize!;
  }

  testWidgets('显式字号 18：正文渲染为 18（>16 变大的证明）', (tester) async {
    final size = await effectiveSize(tester, 18);
    expect(size, 18);
  });

  testWidgets('默认（未传）：渲染为 vendor 16', (tester) async {
    final size = await effectiveSize(tester, null);
    expect(size, 16);
  });

  testWidgets('加粗文本继承基础字号', (tester) async {
    await tester.pumpWidget(harness(fontSize: 20));
    await tester.pump();
    final paragraph = tester
        .renderObjectList<RenderParagraph>(find.byType(RichText))
        .firstWhere((w) => w.text.toPlainText().contains('字号验证正文'));
    // 粗体是段内的子 span（paragraph 基础样式自身无字重），递归找出来
    TextSpan? boldSpan;
    void walk(TextSpan span) {
      if (span.text == '加粗' || (span.text?.contains('加粗') ?? false)) {
        boldSpan = span;
      }
      span.children?.forEach((c) => c is TextSpan ? walk(c) : null);
    }

    final root = paragraph.text as TextSpan;
    walk(root);
    expect(boldSpan, isNotNull, reason: '应存在加粗 span');
    expect(boldSpan!.style!.fontSize, 20, reason: '加粗经 combine 继承基础字号');
    expect(boldSpan!.style!.fontWeight, FontWeight.bold);
  });
}
