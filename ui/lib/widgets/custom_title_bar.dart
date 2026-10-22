import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/ai_activity.dart';
import '../bridge/rust_bridge_repository.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';
import '../utils/window_service.dart';
import 'glass.dart';

/// Custom title bar。
///
/// 桌面端（无边框窗口）：显示窗口控制按钮（最小化/最大化/关闭）+ 拖拽移动。
/// 移动端：仅显示标题与 actions（移动窗口由系统管理，无窗口控制概念）。
///
/// 窗口操作通过 [WindowService]（桌面实现由 main_desktop.dart 注册），
/// 本文件不 import 任何桌面窗口插件——Android main 可达图保持干净。
class CustomTitleBar extends ConsumerWidget {
  final String title;
  final List<Widget>? actions;

  /// 标题右侧、窗口/动作按钮左侧的「今天」操作行。
  /// 仅在传入时显示（移动端主对话界面）；桌面端不传。
  final List<Widget>? middleActions;

  /// 移动端平台的 app bar 顶部安全区高度（状态栏仍在时由 MainScreen 传入）。
  final double topInset;

  /// 最左侧的 leading（移动端为抽屉菜单按钮）。
  final Widget? leading;

  const CustomTitleBar({
    super.key,
    required this.title,
    this.actions,
    this.middleActions,
    this.topInset = 0,
    this.leading,
  });

  static bool get _isDesktop =>
      !kIsWeb && (Platform.isLinux || Platform.isMacOS || Platform.isWindows);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // 前台对话优先显示；否则显示后台当前调用方（分析/消化/检测/整理）。
    final chatBusy = ref.watch(aiGeneratingProvider).isNotEmpty;
    final backgroundActivity = ref.watch(backgroundAiActivityProvider);
    final activity = chatBusy ? AiActivity.chat : backgroundActivity;
    final windowService = WindowService();
    // 玻璃顶栏：模糊 + 半透明 + 底部细分割线统一由 GlassBar 按令牌渲染；
    // 减少动效 / 移动端自动降级（见 design_tokens.dart）。
    return GlassBar(
      topInset: topInset,
      child: Row(
        children: [
          ?leading,
          // 标题/空白区整块可拖拽（桌面端无边框窗口）：Expanded 必须包在
          // GestureDetector 外层——之前改成普通 Spacer 后空白区不吃手势，
          // 只剩标题文字那一小块能拖，看起来就像「无法拖动」。
          Expanded(
            child: GestureDetector(
              behavior: HitTestBehavior.translucent,
              onPanStart: _isDesktop
                  ? (details) => windowService.startDragging()
                  : null,
              child: Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: leading != null ? 4 : 16,
                ),
                child: Row(
                  children: [
                    // 桌面端：logo 是窗口 chrome 的元素，logo+标题 + 可拖拽空白。
                    // 移动端永远不显示 logo：主页传 middleActions 时只显示动作
                    // 行；设置等二级页显示纯文字标题（紧挨 leading 返回键）。
                    if (_isDesktop) ...[
                      ClipRRect(
                        borderRadius: BorderRadius.circular(5),
                        child: SvgPicture.asset(
                          'assets/brand/elsewhen-icon-v2.svg',
                          width: 24,
                          height: 24,
                          semanticsLabel: 'Elsewhen',
                        ),
                      ),
                      const SizedBox(width: 10),
                      Text(
                        title,
                        style: TextStyle(
                          fontSize: 14,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.textPrimary,
                          letterSpacing: 0,
                        ),
                      ),
                      // 桌面端：标题后空白区推开右侧 actions/窗口按钮，
                      // 整块在 GestureDetector 内，处处可拖。
                      const Spacer(),
                    ] else if (middleActions == null) ...[
                      // 移动端二级页（设置等）：纯文字标题，紧挨 leading 返回键。
                      Text(
                        title,
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.textPrimary,
                          letterSpacing: 0,
                        ),
                      ),
                      const Spacer(),
                    ] else ...[
                      // 移动端主页：左侧仅菜单，右侧动作行。吸右：动作钉死右缘，
                      // AI 状态胶囊宽自适应，出现/变长只向左空白伸长，按钮不
                      // 挪位；窄屏极端情况 FittedBox 整体缩小防溢出。
                      const Spacer(),
                      FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: AlignmentDirectional.centerEnd,
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            // AI 状态胶囊（回复中/消化中/…）排在操作按钮左侧。
                            if (activity != null)
                              _AiActivityIndicator(activity: activity),
                            ...middleActions!,
                          ],
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
          if (activity != null && middleActions == null)
            _AiActivityIndicator(activity: activity),
          ...?actions,
          if (_isDesktop) ...[
            const SizedBox(width: 8),
            _WindowButton(
              icon: Icons.minimize,
              onPressed: () => windowService.minimize(),
            ),
            _WindowButton(
              icon: Icons.crop_square,
              onPressed: () => windowService.toggleMaximize(),
            ),
            _WindowButton(
              icon: Icons.close,
              // 关闭走统一入口：按设置「关闭时」决定隐藏到托盘还是退出，
              // 与 WM 原生关闭按钮行为一致（原生关闭见 onWindowClose）。
              onPressed: () => windowService.requestClose(),
              isClose: true,
            ),
          ],
        ],
      ),
    );
  }
}

class _AiActivityIndicator extends StatelessWidget {
  final AiActivity activity;

  const _AiActivityIndicator({required this.activity});

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: activity.tooltip,
      child: Center(
        child: Container(
          height: 26,
          padding: const EdgeInsets.symmetric(horizontal: 8),
          margin: const EdgeInsets.symmetric(horizontal: 4),
          decoration: BoxDecoration(
            color: AppTheme.accentPrimary.withValues(alpha: 0.12),
            borderRadius: BorderRadius.circular(AppTheme.radiusFull),
            border: Border.all(
              color: AppTheme.accentPrimary.withValues(alpha: 0.28),
              width: 1,
            ),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: 11,
                height: 11,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: AppTheme.accentPrimary,
                ),
              ),
              const SizedBox(width: 6),
              Text(
                activity.label,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.accentPrimary,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _WindowButton extends StatefulWidget {
  final IconData icon;
  final VoidCallback onPressed;
  final bool isClose;

  const _WindowButton({
    required this.icon,
    required this.onPressed,
    this.isClose = false,
  });

  @override
  State<_WindowButton> createState() => _WindowButtonState();
}

class _WindowButtonState extends State<_WindowButton> {
  bool _isHovered = false;
  bool _isPressed = false;

  @override
  Widget build(BuildContext context) {
    final hoverBg = widget.isClose
        ? const Color(0xFFE81123)
        : AppTheme.surface3.withValues(alpha: 0.65);
    final hoverIconColor = widget.isClose ? Colors.white : AppTheme.textPrimary;
    final normalIconColor = AppTheme.textSecondary;

    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() {
        _isHovered = false;
        _isPressed = false;
      }),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (_) => setState(() => _isPressed = true),
        onTapUp: (_) => setState(() => _isPressed = false),
        onTapCancel: () => setState(() => _isPressed = false),
        onTap: widget.onPressed,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          curve: Curves.easeOutCubic,
          width: 46,
          height: 48,
          color: _isHovered
              ? (_isPressed && !widget.isClose ? AppTheme.surface3 : hoverBg)
              : Colors.transparent,
          child: Center(
            child: AnimatedScale(
              scale: _isPressed ? 0.90 : 1.0,
              duration: const Duration(milliseconds: 100),
              curve: Curves.easeOutCubic,
              child: Icon(
                widget.icon,
                size: 15,
                color: _isHovered ? hoverIconColor : normalIconColor,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
