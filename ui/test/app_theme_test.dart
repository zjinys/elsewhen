import 'dart:async';

import 'package:elsewhen_ui/models/settings.dart' show AppFonts, AppThemePreset;
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/utils/system_fonts.dart';
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

  test('字体选择作用于 textTheme：system 不套字体，未知值回退 Inter', () async {
    // 测试环境不枚举真实字体（SystemFontService 里 [listFonts] 特判跳过），
    // 播种“本机已安装”索引，让 Noto 双字体走「已安装 → 生效」路径。
    SystemFontService.instance.debugSeedFileIndex({
      'Noto Sans SC': '/fake/system/NotoSansSC.otf',
      'Noto Serif SC': '/fake/system/NotoSerifSC.otf',
    });
    await swallowFontLoadErrors(() {
      TextStyle? bodyOf(String fontName) => AppTheme.buildTheme(
        AppThemePreset.amber,
        Brightness.dark,
        fontName,
      ).textTheme.bodyLarge;

      expect(
        bodyOf('Inter')!.fontFamily,
        contains('Inter'),
        reason: '默认 Inter（现状保持）',
      );
      expect(bodyOf('Noto Sans SC')!.fontFamily, contains('Noto Sans SC'));
      expect(bodyOf('Noto Serif SC')!.fontFamily, contains('Noto Serif SC'));
      expect(
        bodyOf('system')!.fontFamily,
        'Roboto',
        reason: 'system 不套网络字体，保持 flex 默认（Roboto 由系统字体回退解析）',
      );
      expect(
        bodyOf('不存在的字体')!.fontFamily,
        contains('Inter'),
        reason: '非 Google Fonts 的未知值回退默认字体',
      );
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
