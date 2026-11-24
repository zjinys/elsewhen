import 'dart:async';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../theme/app_theme.dart';

/// All operation feedback uses the root overlay, independent of the current
/// scaffold, dialog or keyboard. SnackBar is retained only as a message payload
/// for existing callers (content, actions, duration); it is never laid out.
class FeedbackToast {
  FeedbackToast._(this._overlay, this._themes);

  final OverlayState _overlay;
  final CapturedThemes _themes;
  static final _active = Expando<_ToastEntry>();

  static FeedbackToast of(BuildContext context) {
    final toast = maybeOf(context);
    assert(toast != null, 'Operation feedback requires a Navigator overlay.');
    return toast!;
  }

  static FeedbackToast? maybeOf(BuildContext context) {
    final overlay = Overlay.maybeOf(context, rootOverlay: true);
    if (overlay == null) return null;
    return FeedbackToast._(
      overlay,
      InheritedTheme.capture(from: context, to: overlay.context),
    );
  }

  void dismiss() => _active[_overlay]?.close();

  Future<void> show(SnackBar message, {FeedbackKind? kind}) {
    if (!_overlay.mounted) return Future.value();
    dismiss();
    final entry = _ToastEntry();
    entry.overlay = OverlayEntry(
      builder: (_) => _themes.wrap(
        _FeedbackSurface(
          message: message,
          kind:
              kind ??
              (message.backgroundColor == AppTheme.error
                  ? FeedbackKind.error
                  : FeedbackKind.info),
          entry: entry,
        ),
      ),
    );
    _active[_overlay] = entry;
    _overlay.insert(entry.overlay);
    return entry.closed.future;
  }
}

class _ToastEntry {
  late final OverlayEntry overlay;
  final closed = Completer<void>();
  bool _removed = false;

  void close() {
    if (_removed) return;
    _removed = true;
    overlay.remove();
    overlay.dispose();
    if (!closed.isCompleted) closed.complete();
  }
}

/// Unified text-only entry point with semantic styling.
Future<void> showFeedbackToast(
  BuildContext context, {
  required String message,
  FeedbackKind kind = FeedbackKind.info,
  Duration? duration,
}) => FeedbackToast.of(context).show(
  SnackBar(
    content: Text(message),
    duration: duration ?? Duration(seconds: kind == FeedbackKind.error ? 4 : 3),
  ),
  kind: kind,
);

enum FeedbackKind { success, error, info }

class _FeedbackSurface extends StatefulWidget {
  const _FeedbackSurface({
    required this.message,
    required this.kind,
    required this.entry,
  });
  final SnackBar message;
  final FeedbackKind kind;
  final _ToastEntry entry;

  @override
  State<_FeedbackSurface> createState() => _FeedbackSurfaceState();
}

class _FeedbackSurfaceState extends State<_FeedbackSurface> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.message.onVisible?.call();
      _startTimer();
    });
  }

  void _startTimer() {
    _timer?.cancel();
    // Action feedback remains until explicitly dismissed. Screen-reader users
    // also need time to inspect its content, which may itself contain buttons.
    if (widget.message.persist || MediaQuery.accessibleNavigationOf(context)) {
      return;
    }
    _timer = Timer(widget.message.duration, widget.entry.close);
  }

  @override
  void dispose() {
    _timer?.cancel();
    if (!widget.entry.closed.isCompleted) widget.entry.closed.complete();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final color = switch (widget.kind) {
      FeedbackKind.success => AppTheme.success,
      FeedbackKind.error => colors.error,
      FeedbackKind.info => colors.primary,
    };
    final icon = switch (widget.kind) {
      FeedbackKind.success => Icons.check_circle_outline,
      FeedbackKind.error => Icons.error_outline,
      FeedbackKind.info => Icons.info_outline,
    };
    final action = widget.message.action;
    return Positioned.fill(
      child: SafeArea(
        minimum: const EdgeInsets.all(12),
        child: Align(
          alignment: Alignment.topRight,
          child: LayoutBuilder(
            builder: (context, constraints) => SizedBox(
              width: math.min(400, constraints.maxWidth),
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxHeight: math.min(240, constraints.maxHeight),
                ),
                child: Semantics(
                  liveRegion: true,
                  child: MouseRegion(
                    onEnter: (_) => _timer?.cancel(),
                    onExit: (_) => _startTimer(),
                    child: Listener(
                      onPointerDown: (_) => _timer?.cancel(),
                      onPointerUp: (_) => _startTimer(),
                      onPointerCancel: (_) => _startTimer(),
                      child: Material(
                        key: const ValueKey('feedback-toast'),
                        color: colors.surfaceContainerHigh,
                        elevation: 6,
                        shadowColor: colors.shadow.withValues(alpha: .2),
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(12),
                          side: BorderSide(color: colors.outlineVariant),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.only(
                            left: 14,
                            top: 4,
                            bottom: 4,
                            right: 4,
                          ),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Padding(
                                padding: const EdgeInsets.only(top: 12),
                                child: Icon(icon, size: 18, color: color),
                              ),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(
                                    vertical: 10,
                                  ),
                                  child: SingleChildScrollView(
                                    child: DefaultTextStyle(
                                      style: Theme.of(context)
                                          .textTheme
                                          .bodyMedium!
                                          .copyWith(
                                            color: colors.onSurface,
                                            fontSize: 13,
                                            height: 1.5,
                                          ),
                                      child: Column(
                                        mainAxisSize: MainAxisSize.min,
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          widget.message.content,
                                          if (action != null)
                                            TextButton(
                                              onPressed: () {
                                                widget.entry.close();
                                                action.onPressed();
                                              },
                                              child: Text(action.label),
                                            ),
                                        ],
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                              IconButton(
                                tooltip: MaterialLocalizations.of(context)
                                    .closeButtonTooltip,
                                onPressed: widget.entry.close,
                                icon: const Icon(Icons.close, size: 16),
                                constraints: const BoxConstraints(
                                  minWidth: 44,
                                  minHeight: 44,
                                ),
                                padding: const EdgeInsets.all(12),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
