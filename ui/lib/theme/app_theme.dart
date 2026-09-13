import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class AppTheme {
  // Design tokens - deep tonal surfaces
  static const surface0 = Color(0xFF05070C);
  static const surface1 = Color(0xFF0A0D12);
  static const surface2 = Color(0xFF0F131C);
  static const surface3 = Color(0xFF161D2B);
  static const surface4 = Color(0xFF1E2636);

  // Accent colors - warm orange emphasis
  static const accentPrimary = Color(0xFFE9A568);
  static const accentMuted = Color(0xFF7A5C3D);

  // Text colors
  static const textPrimary = Color(0xFFE8E9EC);
  static const textSecondary = Color(0xFF9BA1AB);
  static const textTertiary = Color(0xFF5F6570);

  // Semantic colors
  static const success = Color(0xFF6EE7B7);
  static const warning = Color(0xFFFBBF24);
  static const error = Color(0xFFEF4444);

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

  static ThemeData get darkTheme {
    final textTheme = GoogleFonts.interTextTheme(
      ThemeData.dark().textTheme,
    );

    return ThemeData(
      useMaterial3: true,
      brightness: Brightness.dark,
      scaffoldBackgroundColor: surface0,
      colorScheme: ColorScheme.dark(
        surface: surface1,
        onSurface: textPrimary,
        primary: accentPrimary,
        secondary: accentMuted,
        error: error,
      ),
      textTheme: textTheme.copyWith(
        displayLarge: textTheme.displayLarge?.copyWith(
          color: textPrimary,
          letterSpacing: -0.02,
        ),
        displayMedium: textTheme.displayMedium?.copyWith(
          color: textPrimary,
          letterSpacing: -0.02,
        ),
        displaySmall: textTheme.displaySmall?.copyWith(
          color: textPrimary,
          letterSpacing: -0.01,
        ),
        bodyLarge: textTheme.bodyLarge?.copyWith(
          color: textPrimary,
          height: 1.6,
        ),
        bodyMedium: textTheme.bodyMedium?.copyWith(
          color: textSecondary,
          height: 1.6,
        ),
        bodySmall: textTheme.bodySmall?.copyWith(
          color: textTertiary,
          height: 1.5,
        ),
      ),
      cardTheme: CardThemeData(
        color: surface2,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radiusMedium),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: surface2,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radiusMedium),
          borderSide: BorderSide.none,
        ),
        contentPadding: const EdgeInsets.all(space4),
      ),
    );
  }
}
