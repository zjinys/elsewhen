import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// 编辑器正文渲染色跟随主题的回归测试（Issue：知识页内容字体显示成白色）。
///
/// AppFlowy 正文 TextSpan 基础样式原本无 color，颜色全靠上方
/// DefaultTextStyle 继承链；一旦该链断裂（例如被某层主题/容器改写），
/// 正文文字就会脱离主题变成硬编码黑/白。`fontAwareTextStyleConfiguration`
/// 现在把 `Theme.textTheme.bodyLarge.color` 显式注入基础样式，本测试验证：
/// - 深色主题 → 实际渲染色为浅色（可读）；
/// - 浅色主题 → 实际渲染色为深色（可读）；
/// - 两者不同（确实跟随主题切换）。
///
/// 注意：同一 testWidgets 内连续 pump 两个 MaterialApp 会让后一个的
/// Theme 错位，故深/浅各用一个独立 testWidgets。
void main() {
  Widget harness(Brightness b) {
    return MaterialApp(
      theme: AppTheme.buildTheme(AppThemePreset.amber, b),
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: Builder(
          builder: (context) => AppFlowyEditor(
            editorState: EditorState(
              document: wikiMarkdownToDocument('主题色验证正文'),
            ),
            editorStyle: EditorStyle.desktop(
              textStyleConfiguration: fontAwareTextStyleConfiguration(
                Theme.of(context).textTheme.bodyLarge?.fontFamily,
                color:
                    Theme.of(context).textTheme.bodyLarge?.color ??
                    Theme.of(context).textTheme.bodyMedium?.color,
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<Color> effectiveColor(WidgetTester tester, Brightness b) async {
    await tester.pumpWidget(harness(b));
    await tester.pump();
    final paragraphs = tester
        .renderObjectList<RenderParagraph>(find.byType(RichText))
        .toList();
    final target = paragraphs.firstWhere(
      (w) => w.text.toPlainText().contains('主题色验证正文'),
    );
    return target.text.style!.color!;
  }

  testWidgets('深色主题：正文字体为浅色', (tester) async {
    final color = await effectiveColor(tester, Brightness.dark);
    expect(
      color.computeLuminance(),
      greaterThan(0.5),
      reason: '深色主题下正文字应为浅色（否则看不清）',
    );
  });

  testWidgets('浅色主题：正文字体为深色', (tester) async {
    final color = await effectiveColor(tester, Brightness.light);
    expect(
      color.computeLuminance(),
      lessThan(0.5),
      reason: '浅色主题下正文字应为深色（否则白底白字看不清）',
    );
  });

  testWidgets('深浅主题渲染色不同（确实跟随主题）', (tester) async {
    final dark = await effectiveColor(tester, Brightness.dark);
    // 单独 pump 浅色（同 testWidgets 双 MaterialApp 会错位，用独立测试避免）
    await tester.pumpWidget(const SizedBox());
    final light = await effectiveColor(tester, Brightness.light);
    expect(dark, isNot(equals(light)));
  });
}
