import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../l10n/localization.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';
import 'adaptive_panel.dart';

/// Only a mounted conversation workspace registers a host. Other screens keep
/// their existing modal presentation, including the mobile title-bar entries.
final _hostProvider = Provider((ref) => _CompanionHost());

class _CompanionHost {
  Future<void> Function(_PanelRequest)? open;
  bool Function()? available;
}

class _PanelRequest {
  _PanelRequest(this.id, this.title, this.builder);
  final String id;
  final String Function(BuildContext) title;
  final Widget Function(BuildContext, bool embedded) builder;
  final done = Completer<void>();
  void complete() {
    if (!done.isCompleted) done.complete();
  }
}

Future<void> showChatCompanion(
  BuildContext context, {
  required String id,
  required String Function(BuildContext) title,
  required Widget Function(BuildContext, bool embedded) builder,
}) async {
  final request = _PanelRequest(id, title, builder);
  final host = ProviderScope.containerOf(
    context,
    listen: false,
  ).read(_hostProvider);
  if (host.open != null && host.available?.call() == true) {
    await host.open!(request);
  } else {
    await showPanelSheet<void>(
      context,
      sheet: (context) => AdaptivePanelSheet(
        title: title(context),
        maxHeightFactor: 0.85,
        builder: (context) => builder(context, false),
      ),
    );
  }
}

/// The chat always stays in the first Row child. A single keyed panel moves
/// between the side and bottom layouts, preserving forms across window resize.
class ChatCompanionWorkspace extends ConsumerStatefulWidget {
  const ChatCompanionWorkspace({super.key, required this.child});
  final Widget child;

  @override
  ConsumerState<ChatCompanionWorkspace> createState() =>
      _ChatCompanionWorkspaceState();
}

class _ChatCompanionWorkspaceState
    extends ConsumerState<ChatCompanionWorkspace> {
  final _panelKey = GlobalKey();
  late final _CompanionHost _host;
  late final Future<void> Function(_PanelRequest) _openCallback;
  _PanelRequest? _request;
  FocusNode? _previousFocus;

  @override
  void initState() {
    super.initState();
    _host = ref.read(_hostProvider);
    _openCallback = _open;
    _host.open = _openCallback;
    _host.available = () =>
        mounted && ModalRoute.of(context)?.isCurrent == true;
  }

  Future<void> _open(_PanelRequest request) async {
    // Mobile opens the standard route, with its usual back/gesture behavior.
    if (isMobilePanel) {
      await showPanelSheet<void>(
        context,
        sheet: (context) => AdaptivePanelSheet(
          title: request.title(context),
          maxHeightFactor: 0.85,
          builder: (context) => request.builder(context, false),
        ),
      );
      return;
    }
    if (_request?.id == request.id) {
      _close();
      return;
    }
    if (_request == null) _previousFocus = FocusManager.instance.primaryFocus;
    _request?.complete();
    setState(() => _request = request);
    await request.done.future;
  }

  void _close() {
    if (_request == null) return;
    _request!.complete();
    setState(() => _request = null);
    final previous = _previousFocus;
    _previousFocus = null;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && previous?.context != null) previous!.requestFocus();
    });
  }

  @override
  void dispose() {
    if (identical(_host.open, _openCallback)) {
      _host.open = null;
      _host.available = null;
    }
    _request?.complete();
    super.dispose();
  }

  Widget _panel(_PanelRequest request) => Material(
    key: _panelKey,
    color: AppTheme.surface1,
    child: Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 8, 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  request.title(context),
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              IconButton(
                tooltip: context.l10n.close,
                onPressed: _close,
                icon: const Icon(Icons.close, size: 18),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: KeyedSubtree(
            key: ValueKey(request.id),
            child: request.builder(context, true),
          ),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    ref.listen(selectedConversationIdProvider, (previous, next) {
      if (previous != next) _close();
    });
    return PopScope(
      canPop: _request == null,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _close();
      },
      child: FocusScope(
        autofocus: true,
        onKeyEvent: (_, event) {
          if (_request != null &&
              event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.escape) {
            _close();
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: LayoutBuilder(
          builder: (context, constraints) {
            final wide = constraints.maxWidth >= 840;
            final request = _request;
            return Stack(
              children: [
                Row(
                  children: [
                    Expanded(
                      child: ExcludeFocus(
                        excluding: !wide && request != null,
                        child: widget.child,
                      ),
                    ),
                    if (wide && request != null) ...[
                      const VerticalDivider(width: 1),
                      SizedBox(
                        width: math.min(420, constraints.maxWidth * 0.44),
                        child: _panel(request),
                      ),
                    ],
                  ],
                ),
                if (!wide && request != null) ...[
                  ModalBarrier(
                    key: const ValueKey("companion-modal-barrier"),
                    color: Colors.black38,
                    dismissible: true,
                    onDismiss: _close,
                  ),
                  Align(
                    alignment: Alignment.bottomCenter,
                    child: SizedBox(
                      height: constraints.maxHeight * 0.85,
                      width: double.infinity,
                      child: _panel(request),
                    ),
                  ),
                ],
              ],
            );
          },
        ),
      ),
    );
  }
}
