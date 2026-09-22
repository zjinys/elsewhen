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
      TextStyle? bodyOf(String fontName) =>
          AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark, fontName)
              .textTheme
              .bodyLarge;

      expect(
        bodyOf('inter')!.fontFamily,
        contains('Inter'),
        reason: '默认 Inter（现状保持）',
      );
      expect(bodyOf('notoSansSc')!.fontFamily, contains('NotoSansSC'));
      expect(bodyOf('notoSerifSc')!.fontFamily, contains('NotoSerifSC'));
      expect(
        bodyOf('system')!.fontFamily,
        'Roboto',
        reason: 'system 不套网络字体，保持 flex 默认（Roboto 由系统字体回退解析）',
      );
      expect(
        bodyOf('Noto Sans CJK SC')!.fontFamily,
        'Noto Sans CJK SC',
        reason: '非内建值按系统字体族名原样套用（fontconfig 解析）',
      );
    });
  });

  test('AppFontFamily.displayNameOf：内建取中文名，系统字体原样显示', () {
    expect(AppFontFamily.displayNameOf('inter'), 'Inter（默认）');
    expect(AppFontFamily.displayNameOf('system'), '系统默认');
    expect(AppFontFamily.displayNameOf('LXGW WenKai'), 'LXGW WenKai');
    expect(AppFontFamily.builtinNames, containsAll(['inter', 'system']));
  });
}
