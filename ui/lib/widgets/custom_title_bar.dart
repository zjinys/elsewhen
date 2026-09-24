import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:nativeapi/nativeapi.dart';

import 'dart:ui';

import '../theme/app_theme.dart';

/// 当前平台窗口（nativeapi 统一接口，取代 window_manager 单例调用）。
Window? get _window => WindowManager.instance.getCurrent();

/// Custom title bar for frameless window
class CustomTitleBar extends StatelessWidget {
  final String title;
  final List<Widget>? actions;

  const CustomTitleBar({super.key, required this.title, this.actions});

  @override
  Widget build(BuildContext context) {
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
                  onPanStart: (details) {
                    // Linux 原生层已实现（gdk_window_begin_move_drag_for_device）。
                    _window?.startDragging();
                  },
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
              ...?actions,
              const SizedBox(width: 8),
              _WindowButton(
                icon: Icons.minimize,
                onPressed: () => _window?.minimize(),
              ),
              _WindowButton(
                icon: Icons.crop_square,
                onPressed: () {
                  final w = _window;
                  if (w == null) return;
                  if (w.isMaximized) {
                    w.unmaximize();
                  } else {
                    w.maximize();
                  }
                },
              ),
              _WindowButton(
                icon: Icons.close,
                // 关闭即隐藏：窗口 isClosable=false 已拦截原生关闭，
                // 这里直接 hide（应用继续驻留后台，热键再唤出）。
                onPressed: () => _window?.hide(),
                isClose: true,
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
