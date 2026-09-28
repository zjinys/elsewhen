import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'dart:ui';

import '../bridge/rust_bridge_repository.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';
import '../utils/window_service.dart';

/// Custom title bar。
///
/// 桌面端（无边框窗口）：显示窗口控制按钮（最小化/最大化/关闭）+ 拖拽移动。
/// 移动端：仅显示标题与 actions（移动窗口由系统管理，无窗口控制概念）。
///
/// 窗口操作通过 [WindowService]（桌面实现由 main_desktop.dart 注册），
/// 本文件不 import nativeapi——其 FFI 结构会让 Android release AOT 崩溃。
class CustomTitleBar extends ConsumerWidget {
  final String title;
  final List<Widget>? actions;

  const CustomTitleBar({super.key, required this.title, this.actions});

  static bool get _isDesktop =>
      !kIsWeb && (Platform.isLinux || Platform.isMacOS || Platform.isWindows);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // 对话生成中，或后台正在把事件消化进知识库
    final aiBusy =
        ref.watch(aiGeneratingProvider).isNotEmpty ||
        ref.watch(knowledgeDigestBusyProvider);
    final windowService = WindowService();
    return ClipRect(
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 18, sigmaY: 18),
        child: Container(
          height: 48,
          decoration: BoxDecoration(
            color: AppTheme.surface1.withValues(alpha: 0.82),
            border: Border(
              bottom: BorderSide(
                color: AppTheme.surface3.withValues(alpha: 0.8),
              ),
            ),
          ),
          child: Row(
            children: [
              Expanded(
                child: GestureDetector(
                  behavior: HitTestBehavior.translucent,
                  onPanStart: _isDesktop
                      ? (details) => windowService.startDragging()
                      : null,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    child: Row(
                      children: [
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
                      ],
                    ),
                  ),
                ),
              ),
              if (aiBusy) const _AiActivityIndicator(),
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
                  // 关闭即隐藏：窗口 isClosable=false 已拦截原生关闭，
                  // 这里直接 hide（应用继续驻留后台，热键再唤出）。
                  onPressed: () => windowService.hideWindow(),
                  isClose: true,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _AiActivityIndicator extends StatelessWidget {
  const _AiActivityIndicator();

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'AI 正在工作',
      child: SizedBox(
        width: 36,
        height: 48,
        child: Center(
          child: SizedBox(
            width: 16,
            height: 16,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              color: AppTheme.accentPrimary,
            ),
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

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      child: GestureDetector(
        onTap: widget.onPressed,
        child: Container(
          width: 48,
          height: 48,
          color: _isHovered
              ? (widget.isClose
                    ? const Color(0xFFE81123)
                    : const Color(0xFF38BDF8).withValues(alpha: 0.1))
              : Colors.transparent,
          child: Icon(
            widget.icon,
            size: 16,
            color: _isHovered
                ? (widget.isClose ? Colors.white : const Color(0xFF38BDF8))
                : const Color(0xFF6C7A89),
          ),
        ),
      ),
    );
  }
}
