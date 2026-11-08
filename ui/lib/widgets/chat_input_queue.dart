import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart';
import '../providers/conversation_provider.dart';
import '../l10n/localization.dart';

/// Read-only queue observation. Dispatch belongs to the repository lifecycle,
/// so changing pages neither stops an authorized queue nor resumes a paused one.
class ChatInputQueue extends ConsumerStatefulWidget {
  const ChatInputQueue({super.key, required this.conversationId});
  final String conversationId;

  @override
  ConsumerState<ChatInputQueue> createState() => _ChatInputQueueState();
}

enum _QueueError { unavailable, actionUnconfirmed }

String _supplementLabel(BuildContext context, String? reason) =>
    switch (reason) {
      'unsupported_context' => context.l10n.supplementUnsupported,
      'input_changed' => context.l10n.supplementInputChanged,
      'context_changed' => context.l10n.supplementContextChanged,
      'budget_or_protocol' => context.l10n.supplementBudget,
      'target_closed' => context.l10n.supplementClosed,
      _ => context.l10n.supplementWaiting,
    };

class _ChatInputQueueState extends ConsumerState<ChatInputQueue> {
  ChatInputQueueDto? _queue;
  _QueueError? _error;
  bool _loading = false;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    unawaited(_tick());
  }

  @override
  void didUpdateWidget(ChatInputQueue oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.conversationId != widget.conversationId) {
      _queue = null;
      _error = null;
      _timer?.cancel();
      unawaited(_tick());
    }
  }

  Future<void> _tick() async {
    if (_loading || !mounted) return;
    final id = widget.conversationId;
    final repository = ref.read(conversationRepositoryProvider);
    if (!repository.supportsChatQueue) return;
    _loading = true;
    try {
      final queue = await repository.getChatInputQueue(id);
      if (!mounted || widget.conversationId != id) return;
      setState(() {
        _queue = queue;
        _error = null;
      });
    } catch (_) {
      if (mounted && widget.conversationId == id) {
        setState(() => _error = _QueueError.unavailable);
      }
    } finally {
      _loading = false;
      if (mounted) {
        _timer?.cancel();
        _timer = Timer(const Duration(seconds: 1), _tick);
      }
    }
  }

  Future<void> _change(Future<void> Function() action) async {
    try {
      await action();
      await _tick();
    } catch (_) {
      if (mounted) setState(() => _error = _QueueError.actionUnconfirmed);
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final repository = ref.watch(conversationRepositoryProvider);
    if (!repository.supportsChatQueue) return const SizedBox.shrink();
    final queue = _queue;
    if (_error != null) {
      return Text(
        _error == _QueueError.unavailable
            ? context.l10n.queueUnavailable
            : context.l10n.queueActionUnconfirmed,
      );
    }
    if (queue == null ||
        (queue.items.isEmpty && queue.recentInjections.isEmpty)) {
      return const SizedBox.shrink();
    }
    final id = widget.conversationId;
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 8,
            children: [
              Text(
                queue.paused
                    ? context.l10n.queuePaused
                    : context.l10n.queueOrdered,
              ),
              TextButton(
                onPressed: () => _change(
                  () => queue.paused
                      ? repository.resumeChatInputQueue(id)
                      : repository.pauseChatInputQueue(id),
                ),
                child: Text(
                  queue.paused
                      ? context.l10n.resumeQueue
                      : context.l10n.pauseQueue,
                ),
              ),
            ],
          ),
          if (queue.items.isNotEmpty) Text(context.l10n.queueInputDescription),
          for (final item in queue.items.take(10))
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        item.preview,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      if (item.mode == 'steer')
                        Text(
                          _supplementLabel(context, item.deferReason),
                          style: const TextStyle(fontSize: 12),
                        ),
                    ],
                  ),
                ),
                TextButton(
                  onPressed: () => _change(() async {
                    await repository.withdrawChatInput(id, item.id);
                  }),
                  child: Text(context.l10n.withdrawGeneration),
                ),
              ],
            ),
          if (queue.items.length > 10) Text(context.l10n.moreQueuedInputs),
          if (queue.recentInjections.isNotEmpty)
            ExpansionTile(
              title: Text(context.l10n.injectedSupplements),
              children: [
                Text(context.l10n.injectedSupplementsHint),
                for (final item in queue.recentInjections)
                  ListTile(
                    dense: true,
                    title: Text(
                      item.preview,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}
