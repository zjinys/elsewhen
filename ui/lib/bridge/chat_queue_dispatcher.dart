import 'dart:async';

/// Process-lifetime polling of explicitly activated queues. The callback must
/// only renew existing ownership, never grant it or resume a paused queue.
class ChatQueueDispatcher {
  ChatQueueDispatcher({
    required this.poll,
    this.interval = const Duration(seconds: 1),
  });

  final Future<bool> Function(String conversationId) poll;
  final Duration interval;
  final Map<String, int> _watched = {};
  Timer? _timer;
  bool _running = false;
  bool _disposed = false;
  int _revision = 0;

  void watch(String conversationId) {
    if (_disposed) return;
    _watched[conversationId] = ++_revision;
    _schedule(Duration.zero);
  }

  void forget(String conversationId) {
    _watched.remove(conversationId);
    if (_watched.isEmpty) {
      _timer?.cancel();
      _timer = null;
    }
  }

  void _schedule(Duration delay) {
    if (_disposed || _running || _timer != null || _watched.isEmpty) return;
    _timer = Timer(delay, _tick);
  }

  Future<void> _tick() async {
    _timer = null;
    if (_disposed || _running) return;
    _running = true;
    try {
      for (final entry in List.of(_watched.entries)) {
        if (_disposed) break;
        if (_watched[entry.key] != entry.value) continue;
        try {
          final active = await poll(entry.key);
          // A concurrent explicit resume supersedes an older paused snapshot.
          if (!active && _watched[entry.key] == entry.value) {
            _watched.remove(entry.key);
          }
        } catch (_) {
          // Keep observing after an uncertain response. Rust's stable input
          // identity prevents replay, and an expired lease is never resumed.
        }
      }
    } finally {
      _running = false;
      _schedule(interval);
    }
  }

  void dispose() {
    _disposed = true;
    _watched.clear();
    _timer?.cancel();
    _timer = null;
  }
}
