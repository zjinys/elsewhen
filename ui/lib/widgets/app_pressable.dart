import 'package:flutter/material.dart';

/// A tactile pressable container that adds subtle active scale feedback (0.97)
/// with a 130ms ease-out curve, making buttons, cards and chips feel responsive
/// to touches and clicks without sluggishness.
///
/// Follows Emil Kowalski's design engineering principle:
/// "Buttons must feel responsive: Add transform: scale(0.97) on :active.
/// This gives instant feedback, making the UI feel like it is truly listening."
class AppPressable extends StatefulWidget {
  final Widget child;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final double scaleOnPress;
  final Duration duration;
  final Curve curve;
  final HitTestBehavior behavior;
  final bool enabled;

  const AppPressable({
    super.key,
    required this.child,
    this.onTap,
    this.onLongPress,
    this.scaleOnPress = 0.97,
    this.duration = const Duration(milliseconds: 130),
    this.curve = Curves.easeOutCubic,
    this.behavior = HitTestBehavior.translucent,
    this.enabled = true,
  });

  @override
  State<AppPressable> createState() => _AppPressableState();
}

class _AppPressableState extends State<AppPressable> {
  bool _isPressed = false;

  void _handleTapDown(TapDownDetails _) {
    if (widget.enabled) {
      setState(() => _isPressed = true);
    }
  }

  void _handleTapUp(TapUpDetails _) {
    if (_isPressed) {
      setState(() => _isPressed = false);
    }
  }

  void _handleTapCancel() {
    if (_isPressed) {
      setState(() => _isPressed = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final hasHandler = widget.onTap != null || widget.onLongPress != null;
    return GestureDetector(
      behavior: widget.behavior,
      onTapDown: widget.enabled ? _handleTapDown : null,
      onTapUp: widget.enabled ? _handleTapUp : null,
      onTapCancel: widget.enabled ? _handleTapCancel : null,
      onTap: widget.enabled && hasHandler ? widget.onTap : null,
      onLongPress: widget.enabled && hasHandler ? widget.onLongPress : null,
      child: AnimatedScale(
        scale: _isPressed ? widget.scaleOnPress : 1.0,
        duration: widget.duration,
        curve: widget.curve,
        child: widget.child,
      ),
    );
  }
}
