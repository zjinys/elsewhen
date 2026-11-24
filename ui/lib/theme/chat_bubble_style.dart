import 'package:material_ui/material_ui.dart';

import 'app_theme.dart';

/// Chat surfaces need their own contrast against the conversation background.
class ChatBubbleStyle {
  ChatBubbleStyle.of(BuildContext context)
    : this._(Theme.of(context).colorScheme);

  ChatBubbleStyle._(ColorScheme scheme)
    : isLight = scheme.brightness == Brightness.light,
      accent = scheme.brightness == Brightness.light
          ? scheme.primary
          : AppTheme.accentPrimary;

  final bool isLight;
  final Color accent;

  AppThemePalette get palette =>
      isLight ? AppThemePalette.light : AppThemePalette.dark;

  Color get metadataColor =>
      isLight ? palette.textSecondary : palette.textTertiary;

  // Keep small labels readable with bright presets such as amber.
  Color get accentInk =>
      isLight ? Color.lerp(accent, palette.textPrimary, .40)! : accent;

  double get assistantMaxWidth => double.infinity;

  BoxDecoration assistantCard({
    Color? darkFill,
    Color? darkBorder,
    bool emphasized = false,
  }) {
    final border = isLight
        ? Border.all(
            color: emphasized
                ? Color.lerp(palette.surface4, accentInk, .35)!
                : palette.surface4,
            width: 0.5,
          )
        : Border.all(
            color: emphasized
                ? accent.withValues(alpha: 0.35)
                : darkBorder ?? Colors.white.withValues(alpha: 0.08),
            width: 0.5,
          );

    return BoxDecoration(
      color: isLight
          ? palette.surface1
          : darkFill ?? palette.surface2,
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      border: border,
      boxShadow: isLight
          ? const [
              BoxShadow(
                color: Color(0x0C0F172A),
                offset: Offset(0, 2),
                blurRadius: 5,
              ),
            ]
          : [
              BoxShadow(
                color: Colors.black.withValues(alpha: 0.25),
                offset: const Offset(0, 2),
                blurRadius: 8,
              ),
            ],
    );
  }

  BoxDecoration userBubble({bool outlined = false}) {
    return BoxDecoration(
      color: isLight
          ? Color.lerp(palette.surface1, accent, .09)
          : accent.withValues(alpha: .15),
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      border: Border.all(
        color: isLight
            ? (outlined
                ? Color.lerp(palette.surface4, accentInk, .30)!
                : Color.lerp(palette.surface4, accent, .18)!)
            : (outlined
                ? accent.withValues(alpha: .55)
                : accent.withValues(alpha: .22)),
        width: 0.5,
      ),
      boxShadow: !isLight
          ? (outlined
              ? [BoxShadow(color: accent.withValues(alpha: .28), blurRadius: 14)]
              : [
                  BoxShadow(
                    color: Colors.black.withValues(alpha: 0.18),
                    offset: const Offset(0, 2),
                    blurRadius: 6,
                  ),
                ])
          : null,
    );
  }
}
