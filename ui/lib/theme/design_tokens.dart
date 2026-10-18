import 'package:flutter/material.dart';

import 'app_theme.dart';

/// 设计令牌层：把「视觉决策」从组件里抽出来，集中在一个文件中管理。
///
/// 层级：
///   DesignTokens        —— 不变的原始常量（模糊值、阴影、时长、曲线、断点）
///   MotionTokens        —— 时长 / 曲线（配合 MediaQuery.disableAnimations 降级）
///   ShadowTokens        —— 多层柔和阴影（随深浅模式切换）
///   Breakpoints         —— 响应式断点 + 当前设备的 FormFactor 判定
///   AppDesignTheme      —— ThemeExtension：把随平台/亮度变化的玻璃参数
///                          注入 ThemeData，组件通过 AppDesignTheme.of(context)
///                          读取；无 Theme 环境（单测）可用 .lerp 回退。
class DesignTokens {
  DesignTokens._();

  // ── 背景模糊（BackdropFilter sigma） ──
  /// 桌面端最大模糊：标题栏 / 悬浮面板。
  static const double blurDesktop = 20;
  /// 移动端略低：GPU 弱 + 省电，视觉差异几乎不可感。
  static const double blurMobile = 12;
  /// 低功耗 / 减少动效模式下直接归零（关闭模糊，保留半透明底）。
  static const double blurReduced = 0;

  // ── 弹窗 / 面板 ──
  static const double dialogMaxWidth = 560;
  static const double contentMaxWidth = 1280;

  // ── 触控目标（移动端 ≥44） ──
  static const double minTouchTarget = 44;
}

/// 时长与曲线：交互动画 120–250ms，全部 easeOut 家族（不用 easeIn）。
class MotionTokens {
  MotionTokens._();

  static const Duration press = Duration(milliseconds: 120);
  static const Duration hover = Duration(milliseconds: 140);
  static const Duration fade = Duration(milliseconds: 200);
  static const Duration panel = Duration(milliseconds: 250);

  static const Curve standard = Curves.easeOutCubic;
  static const Curve emphasized = Curves.easeOutQuart;

  /// 尊重「减少动效」系统设置：返回缩短为 0 的时长。
  static Duration effective(BuildContext context, Duration d) =>
      MediaQuery.maybeDisableAnimationsOf(context) ?? false
          ? Duration.zero
          : d;
}

/// 阴影：多层低透明度叠加，深色模式用纯黑环境光、浅色模式带一点蓝灰。
class ShadowTokens {
  ShadowTokens._();

  static List<BoxShadow> level1(bool dark) => [
        BoxShadow(
          color: dark
              ? Colors.black.withValues(alpha: 0.28)
              : const Color(0xFF33415C).withValues(alpha: 0.06),
          blurRadius: 6,
          offset: const Offset(0, 2),
        ),
      ];

  static List<BoxShadow> level2(bool dark) => [
        BoxShadow(
          color: dark
              ? Colors.black.withValues(alpha: 0.35)
              : const Color(0xFF33415C).withValues(alpha: 0.08),
          blurRadius: 12,
          offset: const Offset(0, 4),
        ),
        BoxShadow(
          color: dark
              ? Colors.black.withValues(alpha: 0.18)
              : const Color(0xFF33415C).withValues(alpha: 0.04),
          blurRadius: 3,
          offset: const Offset(0, 1),
        ),
      ];

  /// 悬浮层（弹窗 / 下拉 / 补全面板）：明显脱离背景。
  static List<BoxShadow> overlay(bool dark) => [
        BoxShadow(
          color: dark
              ? Colors.black.withValues(alpha: 0.5)
              : const Color(0xFF33415C).withValues(alpha: 0.14),
          blurRadius: 28,
          offset: const Offset(0, 10),
        ),
        BoxShadow(
          color: dark
              ? Colors.black.withValues(alpha: 0.25)
              : const Color(0xFF33415C).withValues(alpha: 0.06),
          blurRadius: 8,
          offset: const Offset(0, 2),
        ),
      ];

  /// 主色光晕（强调按钮 / 激活态）：克制使用。
  static List<BoxShadow> accentGlow(Color accent) => [
        BoxShadow(
          color: accent.withValues(alpha: 0.30),
          blurRadius: 16,
          spreadRadius: -2,
          offset: const Offset(0, 4),
        ),
      ];
}

/// 响应式断点。<600 手机；600–1024 平板/小窗；>1024 桌面。
/// 平台判定（isMobilePlatform）优先于断点——手机横屏不当桌面渲染。
enum FormFactor { phone, tablet, desktop }

class Breakpoints {
  Breakpoints._();

  static const double phone = 600;
  static const double desktop = 1024;

  static FormFactor of(BuildContext context, {bool isMobilePlatform = false}) {
    if (isMobilePlatform) return FormFactor.phone;
    final w = MediaQuery.sizeOf(context).width;
    if (w < phone) return FormFactor.phone;
    if (w < desktop) return FormFactor.tablet;
    return FormFactor.desktop;
  }
}

/// 玻璃参数（随亮度 + 平台变化的令牌集合），通过 ThemeExtension 注入。
@immutable
class AppDesignTheme extends ThemeExtension<AppDesignTheme> {
  /// 玻璃模糊 sigma（BackdropFilter 用；0 = 关闭模糊）。
  final double glassBlur;
  /// 玻璃面板填充透明度（叠在 surface1 之上）。
  final double glassFillOpacity;
  /// 玻璃边框透明度。
  final double glassBorderOpacity;
  /// 玻璃顶部 1px 高光线透明度。
  final double glassHighlightOpacity;
  /// 是否为深色模式（组件挑阴影/高光时用）。
  final bool isDark;
  /// 是否移动端平台（调模糊与触控目标时用）。
  final bool isMobile;

  const AppDesignTheme({
    required this.glassBlur,
    required this.glassFillOpacity,
    required this.glassBorderOpacity,
    required this.glassHighlightOpacity,
    required this.isDark,
    required this.isMobile,
  });

  /// 深色：底更透（0.55）让模糊透出来，高光线可见；浅色：底更不透明
  /// （0.72）保证可读，高光线更亮。
  factory AppDesignTheme.resolve({
    required Brightness brightness,
    required bool isMobile,
    bool reduceMotion = false,
  }) {
    final dark = brightness == Brightness.dark;
    final blur = reduceMotion
        ? DesignTokens.blurReduced
        : (isMobile ? DesignTokens.blurMobile : DesignTokens.blurDesktop);
    return AppDesignTheme(
      glassBlur: blur,
      glassFillOpacity: dark ? 0.55 : 0.72,
      glassBorderOpacity: dark ? 0.10 : 0.45,
      glassHighlightOpacity: dark ? 0.08 : 0.65,
      isDark: dark,
      isMobile: isMobile,
    );
  }

  /// 组件读取入口；无 extension 时按亮度现场合成（健壮回退）。
  static AppDesignTheme of(BuildContext context, {bool isMobile = false}) {
    final ext = Theme.of(context).extension<AppDesignTheme>();
    if (ext != null) return ext;
    return AppDesignTheme.resolve(
      brightness: AppTheme.brightness,
      isMobile: isMobile,
      reduceMotion:
          MediaQuery.maybeDisableAnimationsOf(context) ?? false,
    );
  }

  @override
  AppDesignTheme copyWith({
    double? glassBlur,
    double? glassFillOpacity,
    double? glassBorderOpacity,
    double? glassHighlightOpacity,
    bool? isDark,
    bool? isMobile,
  }) {
    return AppDesignTheme(
      glassBlur: glassBlur ?? this.glassBlur,
      glassFillOpacity: glassFillOpacity ?? this.glassFillOpacity,
      glassBorderOpacity: glassBorderOpacity ?? this.glassBorderOpacity,
      glassHighlightOpacity:
          glassHighlightOpacity ?? this.glassHighlightOpacity,
      isDark: isDark ?? this.isDark,
      isMobile: isMobile ?? this.isMobile,
    );
  }

  @override
  AppDesignTheme lerp(ThemeExtension<AppDesignTheme>? other, double t) {
    if (other is! AppDesignTheme) return this;
    return AppDesignTheme(
      glassBlur: lerpDouble(glassBlur, other.glassBlur, t),
      glassFillOpacity:
          lerpDouble(glassFillOpacity, other.glassFillOpacity, t),
      glassBorderOpacity:
          lerpDouble(glassBorderOpacity, other.glassBorderOpacity, t),
      glassHighlightOpacity:
          lerpDouble(glassHighlightOpacity, other.glassHighlightOpacity, t),
      isDark: t < 0.5 ? isDark : other.isDark,
      isMobile: t < 0.5 ? isMobile : other.isMobile,
    );
  }

  double lerpDouble(double a, double b, double t) => a + (b - a) * t;
}
