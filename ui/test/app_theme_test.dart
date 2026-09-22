import 'dart:async';

import 'package:elsewhen_ui/models/settings.dart'
    show AppFontFamily, AppThemePreset;
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

  test('字体选择作用于 textTheme：system 不套网络字体，其余注入对应字体族', () async {
    await swallowFontLoadErrors(() {
      TextStyle? bodyOf(AppFontFamily font) =>
          AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark, font)
              .textTheme
              .bodyLarge;

      expect(
        bodyOf(AppFontFamily.inter)!.fontFamily,
        contains('Inter'),
        reason: '默认 Inter（现状保持）',
      );
      expect(
        bodyOf(AppFontFamily.notoSansSc)!.fontFamily,
        contains('NotoSansSC'),
      );
      expect(
        bodyOf(AppFontFamily.notoSerifSc)!.fontFamily,
        contains('NotoSerifSC'),
      );
      expect(
        bodyOf(AppFontFamily.system)!.fontFamily,
        'Roboto',
        reason: 'system 不套网络字体，保持 flex 默认（Roboto 由系统字体回退解析）',
      );
    });
  });

  test('AppFontFamily.fromName 未知值回退 Inter（老库兼容）', () {
    expect(AppFontFamily.fromName('notoSansSc'), AppFontFamily.notoSansSc);
    expect(AppFontFamily.fromName('不存在的'), AppFontFamily.inter);
  });
}
