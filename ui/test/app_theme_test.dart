import 'dart:async';

import 'package:elsewhen_ui/models/settings.dart'
    show AppFonts, AppThemePreset;
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:google_fonts/google_fonts.dart';

/// 测试环境无字体资产也无网络：google_fonts 的异步加载失败属预期。
/// 在独立 zone 里执行并吞掉字体加载错误，避免异步异常逃逸、污染同文件其它测试。
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
  // google_fonts 在构建 textTheme 时走 ServicesBinding；禁止测试内联网拉字体
  TestWidgetsFlutterBinding.ensureInitialized();
  GoogleFonts.config.allowRuntimeFetching = false;

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

  test('字体选择作用于 textTheme：system 不套网络字体，未知值回退 Inter', () async {
    await swallowFontLoadErrors(() {
      TextStyle? bodyOf(String fontName) =>
          AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark, fontName)
              .textTheme
              .bodyLarge;

      expect(
        bodyOf('Inter')!.fontFamily,
        contains('Inter'),
        reason: '默认 Inter（现状保持）',
      );
      expect(bodyOf('Noto Sans SC')!.fontFamily, contains('NotoSansSC'));
      expect(bodyOf('Noto Serif SC')!.fontFamily, contains('NotoSerifSC'));
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
