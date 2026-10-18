import 'dart:ui';

import 'package:flutter/material.dart';

import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';

/// 玻璃拟态容器族：
///   GlassPanel  —— 通用玻璃卡片 / 面板（ClipRRect + BackdropFilter + 半透明
///                 填充 + 细边框 + 顶部 1px 高光线）。
///   GlassBar    —— 顶栏 / 悬浮工具条（圆角可选，通常贴边，只保留下边框）。
///
/// 性能纪律：
///  * BackdropFilter 开销大 —— 只用于「覆盖在内容之上的静态层」（标题栏、
///    弹窗、浮层），列表项内禁止使用；每个实例自带 RepaintBoundary 隔离。
///  * blur = 0（减少动效 / 移动端降级）时跳过 BackdropFilter，只保留半透明
///    底 + 边框，视觉几乎不变、零额外开销。
class GlassPanel extends StatelessWidget {
  final Widget child;
  final EdgeInsetsGeometry? padding;
  final EdgeInsetsGeometry? margin;
  final double borderRadius;

  /// 覆盖令牌默认值（通常不传，让 AppDesignTheme 统一决定）。
  final double? blur;
  final double? fillOpacity;
  final Color? fillColor;

  /// 顶部 1px 高光线（玻璃质感的关键细节）。
  final bool showHighlight;

  /// 阴影等级：0 无 / 1 卡片 / 2 浮起。
  final int elevation;

  final VoidCallback? onTap;

  const GlassPanel({
    super.key,
    required this.child,
    this.padding,
    this.margin,
    this.borderRadius = AppTheme.radiusLarge,
    this.blur,
    this.fillOpacity,
    this.fillColor,
    this.showHighlight = true,
    this.elevation = 1,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final tokens = AppDesignTheme.of(context);
    final dark = tokens.isDark;
    final radius = BorderRadius.circular(borderRadius);
    final sigma = blur ?? tokens.glassBlur;
    final fill = (fillColor ?? AppTheme.surface1)
        .withValues(alpha: fillOpacity ?? tokens.glassFillOpacity);

    final shadows = switch (elevation) {
      1 => ShadowTokens.level1(dark),
      2 => ShadowTokens.level2(dark),
      _ => const <BoxShadow>[],
    };

    Widget content = Container(
      padding: padding,
      decoration: BoxDecoration(
        color: fill,
        borderRadius: radius,
        border: Border.all(
          color: (dark ? Colors.white : AppTheme.surface4)
              .withValues(alpha: tokens.glassBorderOpacity),
        ),
      ),
      child: child,
    );

    if (showHighlight) {
      content = _TopHighlight(
        radius: radius,
        opacity: tokens.glassHighlightOpacity,
        dark: dark,
        child: content,
      );
    }

    // blur > 0 才付 BackdropFilter 的成本；ClipRRect 始终需要（高光/圆角）。
    if (sigma > 0) {
      content = ClipRRect(
        borderRadius: radius,
        child: BackdropFilter(
          filter: ImageFilter.blur(sigmaX: sigma, sigmaY: sigma),
          child: content,
        ),
      );
    } else {
      content = ClipRRect(borderRadius: radius, child: content);
    }

    return RepaintBoundary(
      child: Container(
        margin: margin,
        decoration: BoxDecoration(
          borderRadius: radius,
          boxShadow: shadows,
        ),
        child: onTap == null
            ? content
            : Material(
                type: MaterialType.transparency,
                child: InkWell(
                  onTap: onTap,
                  borderRadius: radius,
                  child: content,
                ),
              ),
      ),
    );
  }
}

/// 玻璃顶栏：贴边（无圆角），底部细分割线，供 CustomTitleBar / 悬浮工具条用。
class GlassBar extends StatelessWidget implements PreferredSizeWidget {
  final Widget child;
  final double height;

  /// 顶部安全区（移动端状态栏）——高度会叠加进 preferredSize。
  final double topInset;

  const GlassBar({
    super.key,
    required this.child,
    this.height = 48,
    this.topInset = 0,
  });

  @override
  Size get preferredSize => Size.fromHeight(height + topInset);

  @override
  Widget build(BuildContext context) {
    final tokens = AppDesignTheme.of(context);
    final sigma = tokens.glassBlur;

    Widget content = Container(
      height: height + topInset,
      padding: EdgeInsets.only(top: topInset),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(
          alpha: tokens.isDark ? 0.62 : 0.78,
        ),
        border: Border(
          bottom: BorderSide(
            color: AppTheme.surface3
                .withValues(alpha: tokens.isDark ? 0.65 : 0.9),
          ),
        ),
      ),
      child: child,
    );

    if (sigma > 0) {
      content = ClipRect(
        child: BackdropFilter(
          filter: ImageFilter.blur(sigmaX: sigma, sigmaY: sigma),
          child: content,
        ),
      );
    }

    return RepaintBoundary(child: content);
  }
}

/// 顶部 1px 内高光：一条渐隐的白色细线，是玻璃「厚度感」的来源。
class _TopHighlight extends StatelessWidget {
  final BorderRadius radius;
  final double opacity;
  final bool dark;
  final Widget child;

  const _TopHighlight({
    required this.radius,
    required this.opacity,
    required this.dark,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        child,
        Positioned(
          top: 0,
          left: 1,
          right: 1,
          child: IgnorePointer(
            child: Container(
              height: 1,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.vertical(top: radius.topLeft),
                gradient: LinearGradient(
                  colors: [
                    Colors.white.withValues(alpha: 0),
                    Colors.white.withValues(
                      alpha: dark ? opacity : opacity * 0.9,
                    ),
                    Colors.white.withValues(alpha: 0),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// 低饱和氛围渐变背景：叠在 surface0 之上，为玻璃层提供可透出的层次。
/// 色差克制（surface 色之间渐变 + 主色 4~6% 光斑），不会刺眼。
class AmbientBackground extends StatelessWidget {
  final Widget child;

  const AmbientBackground({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    final dark = AppDesignTheme.of(context).isDark;
    return Container(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: dark
              ? [AppTheme.surface0, AppTheme.surface1, AppTheme.surface0]
              : [AppTheme.surface1, AppTheme.surface0, AppTheme.surface2],
        ),
      ),
      child: child,
    );
  }
}
