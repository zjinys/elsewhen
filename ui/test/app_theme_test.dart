import 'dart:async';

import 'package:elsewhen_ui/models/settings.dart' show AppFonts, AppThemePreset;
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/theme/content_font.dart';
import 'package:elsewhen_ui/utils/system_fonts.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 在独立 zone 中运行主题测试，避免异步异常污染其它测试。
Future<void> swallowFontLoadErrors(FutureOr<void> Function() body) {
  return runZonedGuarded(
    () async {
      await body();
      // 给 zone 内 pending 的字体加载失败一个落地的机会
      await Future<void>.delayed(const Duration(milliseconds: 100));
    },
    (error, stack) {
      expect(
        error.toString(),
        anyOf(contains('font'), contains('Font')),
        reason: '只应吞掉字体加载错误，其它错误要暴露',
      );
    },
  )!.then((_) {});
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('对话框圆角跟随设计令牌（radiusMedium=12），不用 M3 默认 28', () async {
    await swallowFontLoadErrors(() {
      for (final brightness in [Brightness.light, Brightness.dark]) {
        final theme = AppTheme.buildTheme(AppThemePreset.indigo, brightness);
        final shape = theme.dialogTheme.shape;
        expect(shape, isA<RoundedRectangleBorder>());
        expect(
          (shape! as RoundedRectangleBorder).borderRadius,
          BorderRadius.circular(AppTheme.radiusMedium),
        );
      }
    });
  });

  test('界面字体固定：textTheme 恒为 uiFont，回退链含中文字体', () async {
    await swallowFontLoadErrors(() {
      final body = AppTheme.buildTheme(
        AppThemePreset.amber,
        Brightness.dark,
      ).textTheme.bodyLarge!;
      expect(body.fontFamily, AppFonts.uiFont);
      expect(
        body.fontFamilyFallback,
        containsAll(AppFonts.cjkFallback),
        reason: '显式中文回退，避免平台自选回退字体',
      );
    });
  });

  group('内容字体作用域', () {
    // 测试环境不枚举真实字体（SystemFontService 里 [listFonts] 特判跳过），
    // 播种“本机已安装”索引，让 Noto 字体走「已安装 → 生效」路径。
    setUp(() {
      SystemFontService.instance.debugSeedFileIndex({
        'Noto Serif SC': '/fake/system/NotoSerifSC.otf',
      });
    });

    Future<({String? content, String? ui, String? markdown})> pump(
      WidgetTester tester,
      String stored,
    ) async {
      await tester.pumpWidget(
        ContentFont(
          family: resolveFontFamily(stored, fallback: AppFonts.defaultFont),
          child: MaterialApp(
            theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
            home: Scaffold(
              body: Column(
                children: [
                  const Text('界面'),
                  const ContentFontScope(child: Text('内容')),
                  const MarkdownView(markdown: 'Markdown 段落'),
                ],
              ),
            ),
          ),
        ),
      );
      String? familyOf(String text) {
        final el = tester.element(find.text(text));
        return DefaultTextStyle.of(el).style.fontFamily;
      }

      final rich = tester.widget<RichText>(
        find.byWidgetPredicate(
          (w) => w is RichText && w.text.toPlainText().contains('Markdown 段落'),
        ),
      );
      return (
        content: familyOf('内容'),
        ui: familyOf('界面'),
        markdown: rich.text.style?.fontFamily,
      );
    }

    testWidgets('本地字体：内容区与 Markdown 生效，界面不变', (tester) async {
      final r = await pump(tester, 'local:Noto Serif SC');
      expect(r.content, 'Noto Serif SC');
      expect(r.markdown, 'Noto Serif SC');
      expect(r.ui, AppFonts.uiFont, reason: '界面文案不受内容字体影响');
    });

    testWidgets('system：内容区回到平台字体，界面仍为 uiFont', (tester) async {
      final r = await pump(tester, AppFonts.system);
      expect(r.content, 'Roboto', reason: '平台 typography 的默认字体族');
      expect(r.markdown, 'Roboto');
      expect(r.ui, AppFonts.uiFont);
    });
  });

  test('AppFonts.normalize：旧机器名与空值映射到家族名，其余透传', () {
    expect(AppFonts.normalize(''), 'Inter');
    expect(AppFonts.normalize('inter'), 'Inter');
    expect(AppFonts.normalize('notoSansSc'), 'Noto Sans SC');
    expect(AppFonts.normalize('notoSerifSc'), 'Noto Serif SC');
    expect(AppFonts.normalize('system'), 'system');
    expect(AppFonts.normalize('LXGW WenKai'), 'LXGW WenKai');
    expect(AppFonts.displayNameOf('system'), '系统默认');
    expect(AppFonts.displayNameOf('Inter'), 'Inter');
  });
}
