import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../theme/design_tokens.dart';

/// 响应式基础件：
///   AdaptiveContext      —— 平台 + 断点判定（手机横屏不误判桌面）
///   ResponsiveContent    —— 内容区最大宽度约束 + 水平居中（桌面阅读列宽）
///   ResponsiveVisibility —— 按 form factor 换组件（侧栏 ↔ 抽屉等）
class AdaptiveContext {
  /// 平台判定：Android/iOS 永远按移动交互渲染，与窗口宽度无关。
  static bool get isMobilePlatform =>
      !kIsWeb && (Platform.isAndroid || Platform.isIOS);

  static bool get isDesktopPlatform =>
      !kIsWeb && (Platform.isLinux || Platform.isMacOS || Platform.isWindows);

  static FormFactor formFactorOf(BuildContext context) =>
      Breakpoints.of(context, isMobilePlatform: isMobilePlatform);

  static bool isPhone(BuildContext context) =>
      formFactorOf(context) == FormFactor.phone;

  static bool isCompact(BuildContext context) =>
      formFactorOf(context) != FormFactor.desktop;
}

/// 桌面端阅读区：约束最大宽度并水平居中，两侧留白随窗口增大。
/// 移动端全宽 + 16px 安全边距。
class ResponsiveContent extends StatelessWidget {
  final Widget child;

  /// 桌面端内容最大宽度（默认 1280；阅读密集场景可传 840~960）。
  final double maxWidth;

  /// 移动端水平边距（默认 16 安全边距；桌面端为 0，由居中留白接管）。
  final double mobilePadding;

  /// 桌面端达到 maxWidth 后是否仍保留一点边距（默认 24）。
  final double desktopGutter;

  const ResponsiveContent({
    super.key,
    required this.child,
    this.maxWidth = DesignTokens.contentMaxWidth,
    this.mobilePadding = 16,
    this.desktopGutter = 24,
  });

  @override
  Widget build(BuildContext context) {
    final compact = AdaptiveContext.isCompact(context);
    if (compact) {
      return Padding(
        padding: EdgeInsets.symmetric(horizontal: mobilePadding),
        child: child,
      );
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final contentWidth = constraints.maxWidth - desktopGutter * 2;
        return Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxWidth: contentWidth < maxWidth ? contentWidth : maxWidth,
            ),
            child: child,
          ),
        );
      },
    );
  }
}

/// 按 form factor 渲染不同组件；null 分支表示该形态下不渲染。
class ResponsiveVisibility extends StatelessWidget {
  final Widget? phone;
  final Widget? tablet;
  final Widget? desktop;

  const ResponsiveVisibility({
    super.key,
    this.phone,
    this.tablet,
    this.desktop,
  });

  @override
  Widget build(BuildContext context) {
    final factor = AdaptiveContext.formFactorOf(context);
    final widget = switch (factor) {
      FormFactor.phone => phone,
      FormFactor.tablet => tablet ?? phone,
      FormFactor.desktop => desktop ?? tablet ?? phone,
    };
    return widget ?? const SizedBox.shrink();
  }
}
