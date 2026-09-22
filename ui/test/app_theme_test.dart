import 'package:elsewhen_ui/models/settings.dart' show AppThemePreset;
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  // google_fonts 在构建 textTheme 时走 ServicesBinding（字体加载失败仅记日志）
  TestWidgetsFlutterBinding.ensureInitialized();

  test('对话框圆角跟随设计令牌（radiusMedium=12），不用 M3 默认 28', () {
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
}
