import 'package:flex_color_scheme/flex_color_scheme.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

import '../models/settings.dart' show AppFonts, AppThemePreset;

/// 一整套界面色板（浅 / 深两套）。应用在 `AppTheme.apply()` 时切换，
/// 组件里继续写 `AppTheme.surface1` 就能自动跟随深浅模式。
class AppThemePalette {
  final Color surface0;
  final Color surface1;
  final Color surface2;
  final Color surface3;
  final Color surface4;
  final Color accentPrimary;
  final Color accentMuted;
  final Color textPrimary;
  final Color textSecondary;
  final Color textTertiary;
  final Color success;
  final Color warning;
  final Color error;

  const AppThemePalette({
    required this.surface0,
    required this.surface1,
    required this.surface2,
    required this.surface3,
    required this.surface4,
    required this.accentPrimary,
    required this.accentMuted,
    required this.textPrimary,
    required this.textSecondary,
    required this.textTertiary,
    required this.success,
    required this.warning,
    required this.error,
  });

  /// 深色：保留现有「深墨表面 + 暖橙强调」的视觉身份
  static const dark = AppThemePalette(
    surface0: Color(0xFF05070C),
    surface1: Color(0xFF0A0D12),
    surface2: Color(0xFF0F131C),
    surface3: Color(0xFF161D2B),
    surface4: Color(0xFF1E2636),
    accentPrimary: Color(0xFFE9A568),
    accentMuted: Color(0xFF7A5C3D),
    textPrimary: Color(0xFFE8E9EC),
    textSecondary: Color(0xFF9BA1AB),
    textTertiary: Color(0xFF5F6570),
    success: Color(0xFF6EE7B7),
    warning: Color(0xFFFBBF24),
    error: Color(0xFFEF4444),
  );

  /// 浅色：暖白纸面 + 深墨文字（accent 由所选 flex 色卡动态给到，见 AppTheme.apply）
  static const light = AppThemePalette(
    surface0: Color(0xFFF4F5F7),
    surface1: Color(0xFFFFFFFF),
    surface2: Color(0xFFEFF1F4),
    surface3: Color(0xFFE1E4EA),
    surface4: Color(0xFFD3D8E0),
    accentPrimary: Color(0xFFB45309),
    accentMuted: Color(0xFFD9A05F),
    textPrimary: Color(0xFF1A1D21),
    textSecondary: Color(0xFF5A6068),
    textTertiary: Color(0xFF8A919B),
    success: Color(0xFF047857),
    warning: Color(0xFFB45309),
    error: Color(0xFFDC2626),
  );
}

class AppTheme {
  // 当前生效的色板 + 强调色（跟着 flex_color_scheme 的预设走）
  static AppThemePalette _current = AppThemePalette.dark;
  static Color _accent = AppThemePalette.dark.accentPrimary;
  static Brightness _brightness = Brightness.dark;

  /// 切换全局色板。`accent` 传所选 flex 色卡的 primary，
  /// 让自定义组件（头像描边、激活态、spinner 等）跟着主题 preset 换色。
  static void apply(Brightness brightness, {Color? accent}) {
    _brightness = brightness;
    _current = brightness == Brightness.dark
        ? AppThemePalette.dark
        : AppThemePalette.light;
    if (accent != null) _accent = accent;
  }

  static Brightness get brightness => _brightness;

  static Color get surface0 => _current.surface0;
  static Color get surface1 => _current.surface1;
  static Color get surface2 => _current.surface2;
  static Color get surface3 => _current.surface3;
  static Color get surface4 => _current.surface4;

  static Color get accentPrimary => _accent;
  static Color get accentMuted => _current.accentMuted;

  static Color get textPrimary => _current.textPrimary;
  static Color get textSecondary => _current.textSecondary;
  static Color get textTertiary => _current.textTertiary;

  static Color get success => _current.success;
  static Color get warning => _current.warning;
  static Color get error => _current.error;

  // Spacing tokens
  static const space1 = 4.0;
  static const space2 = 8.0;
  static const space3 = 12.0;
  static const space4 = 16.0;
  static const space6 = 24.0;
  static const space8 = 32.0;
  static const space12 = 48.0;

  // Border radius tokens
  static const radiusSmall = 6.0;
  static const radiusMedium = 12.0;
  static const radiusLarge = 16.0;
  static const radiusFull = 999.0;

  /// flex_color_scheme 对应每套预设的色卡
  static FlexScheme flexSchemeOf(AppThemePreset preset) {
    return switch (preset) {
      AppThemePreset.amber => FlexScheme.amber,
      AppThemePreset.indigo => FlexScheme.indigo,
      AppThemePreset.aqua => FlexScheme.aquaBlue,
      AppThemePreset.violet => FlexScheme.deepPurple,
    };
  }

  /// 用 flex_color_scheme 搭 Material 主题（浅 / 深都由设定色卡派生）。
  /// [fontName]：Google Fonts 家族名，或 [AppFonts.system] 跟随系统；
  /// 未知值回退 [AppFonts.defaultFont]。
  static ThemeData buildTheme(
    AppThemePreset preset,
    Brightness brightness, [
    String fontName = AppFonts.defaultFont,
  ]) {
    final scheme = flexSchemeOf(preset);
    final flex = brightness == Brightness.dark
        ? FlexThemeData.dark(
            scheme: scheme,
            useMaterial3: true,
            surfaceMode: FlexSurfaceMode.highScaffoldLowSurface,
            blendLevel: 22,
          )
        : FlexThemeData.light(
            scheme: scheme,
            useMaterial3: true,
            surfaceMode: FlexSurfaceMode.highScaffoldLowSurface,
            blendLevel: 12,
          );

    // 字体 + 行高微调（沿用现有 typography 习惯）
    final textTheme = _textThemeFor(fontName, flex.textTheme);
    return flex.copyWith(
      scaffoldBackgroundColor: _current.surface0,
      colorScheme: flex.colorScheme.copyWith(
        surface: _current.surface1,
        surfaceContainer: _current.surface2,
        surfaceContainerHighest: _current.surface3,
        outline: _current.surface3,
        outlineVariant: _current.surface3.withValues(alpha: 0.7),
      ),
      dividerTheme: DividerThemeData(
        color: _current.surface3.withValues(alpha: 0.72),
        thickness: 1,
        space: 1,
      ),
      splashFactory: InkSparkle.splashFactory,
      textTheme: textTheme.copyWith(
        bodyLarge: textTheme.bodyLarge?.copyWith(height: 1.75, fontSize: 16),
        bodyMedium: textTheme.bodyMedium?.copyWith(height: 1.65, fontSize: 14),
        bodySmall: textTheme.bodySmall?.copyWith(height: 1.55, fontSize: 12),
      ),
      cardTheme: CardThemeData(
        color: _current.surface2,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radiusMedium),
        ),
      ),
      // 对话框圆角与卡片/输入框一致（M3 默认 28 过大）
      dialogTheme: DialogThemeData(
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radiusMedium),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: _current.surface2,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radiusMedium),
          borderSide: BorderSide.none,
        ),
        contentPadding: const EdgeInsets.all(space4),
        hintStyle: TextStyle(color: _current.textTertiary, fontSize: 14),
        labelStyle: TextStyle(color: _current.textSecondary, fontSize: 13),
      ),
    );
  }

  /// 兼容引用（等价于默认预设的深色主题）
  static ThemeData get darkTheme => buildTheme(AppThemePreset.amber, Brightness.dark);

  /// 按字体选择生成 textTheme：system 不套网络字体（跟随系统），
  /// 其余经 google_fonts 按家族名动态加载（首次使用联网下载并缓存，
  /// 未知家族名回退默认字体）。
  static TextTheme _textThemeFor(String fontName, TextTheme base) {
    if (fontName == AppFonts.system) return base;
    final family =
        GoogleFonts.asMap().containsKey(fontName) ? fontName : AppFonts.defaultFont;
    return GoogleFonts.getTextTheme(family, base);
  }
}
