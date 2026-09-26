import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/utils/system_fonts.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// WikiContentEditor 两层覆盖模型的渲染验证：
/// - fontFamily 覆盖 → 渲染字体族被覆盖（AppFonts.system 不注入）；
/// - lineHeight 覆盖 → 渲染 height 随覆盖；未覆盖回落 vendor 默认 1.5。
void main() {
  // 测试环境不枚举真实字体，播种 Noto Serif SC 让字体覆盖断言走“已安装”路径。
  SystemFontService.instance.debugSeedFileIndex({
    'Noto Serif SC': '/fake/system/NotoSerifSC.otf',
  });

  Widget harness({String? fontFamily, double? fontSize, double? lineHeight}) {
    return MaterialApp(
      theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: WikiContentEditor(
          slug: 'reading-layer-test',
          contentMd: '阅读参数正文',
          editable: false,
          onWikiLinkTap: (_) {},
          onSave: (_) async {},
          fontFamily: fontFamily,
          fontSize: fontSize ?? AppFonts.defaultFontSize,
          lineHeight: lineHeight,
        ),
      ),
    );
  }

  RenderParagraph firstParagraph(WidgetTester tester) => tester
      .renderObjectList<RenderParagraph>(find.byType(RichText))
      .firstWhere((w) => w.text.toPlainText().contains('阅读参数正文'));

  testWidgets('fontFamily 覆盖：渲染字体族等于覆盖值', (tester) async {
    await tester.pumpWidget(harness(fontFamily: 'Noto Serif SC'));
    await tester.pump();
    final style = firstParagraph(tester).text.style!;
    expect(style.fontFamily, 'Noto Serif SC', reason: '编辑器字体覆盖应注入本机字体族');
    expect(style.fontFamily, isNotNull);
  });

  testWidgets('fontFamily = 跟随系统：不注入家族（fallback），仍可渲染', (tester) async {
    await tester.pumpWidget(harness(fontFamily: AppFonts.system));
    await tester.pump();
    final style = firstParagraph(tester).text.style!;
    expect(
      style.fontFamily,
      isNot('Noto Serif SC'),
      reason: 'system 覆盖不应注入具体家族',
    );
  });

  testWidgets('lineHeight 覆盖 2.0：渲染 height = 2.0', (tester) async {
    await tester.pumpWidget(harness(lineHeight: 2.0));
    await tester.pump();
    final style = firstParagraph(tester).text.style!;
    expect(style.height, 2.0);
  });

  testWidgets('lineHeight 未覆盖：回落 vendor 默认 1.5', (tester) async {
    await tester.pumpWidget(harness());
    await tester.pump();
    final style = firstParagraph(tester).text.style!;
    expect(style.height, 1.5);
  });
}
