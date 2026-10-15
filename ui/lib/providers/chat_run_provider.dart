import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart';
import '../bridge/rust_bridge_repository.dart';
import '../models/conversation.dart';
import 'conversation_provider.dart';
import 'todo_provider.dart';
import 'wiki_provider.dart';
import 'tool_progress_details.dart';

bool chatRunIsActive(String status) =>
    const {'preparing', 'running', 'cancelling'}.contains(status);

class ChatToolProgress {
  final String id;
  final String name;
  final String outcome;
  final String details;
  const ChatToolProgress(this.id, this.name, this.outcome, this.details);
}

class ChatRunView {
  final ChatRunDto? run;
  final String phase;
  final String draft;
  final List<ChatToolProgress> tools;
  final bool restoring;
  final bool starting;
  final bool stopping;
  final String? connectionError;
  final String? pendingInputId;
  final bool historyResynced;

  const ChatRunView({
    this.run,
    this.phase = 'preparing',
    this.draft = '',
    this.tools = const [],
    this.restoring = false,
    this.starting = false,
    this.stopping = false,
    this.connectionError,
    this.pendingInputId,
    this.historyResynced = false,
  });

  bool get active => run != null && chatRunIsActive(run!.status);
  bool get busy =>
      active ||
      starting ||
      restoring ||
      pendingInputId != null ||
      connectionError != null;
  bool get canRetry =>
      !busy &&
      run != null &&
      const {'failed', 'cancelled', 'interrupted'}.contains(run!.status);
}

/// One bounded observer per conversation. Disposing only stops observation;
/// only an explicit stop request may change the durable Rust run.
class ChatRunController extends ChangeNotifier {
  ChatRunController(
    this.repository,
    this.conversationId, {
    this.pollInterval = const Duration(milliseconds: 300),
    this.onTerminal,
    this.onProposal,
  });

  final ConversationRepository repository;
  final String conversationId;
  final Duration pollInterval;
  final VoidCallback? onTerminal;
  final VoidCallback? onProposal;
  ChatRunDto? _run;
  final Map<String, ChatToolProgress> _tools = {};
  String _phase = 'preparing';
  String _draft = '';
  int _draftSequence = 0;
  int _cursor = 0;
  bool _restoring = false;
  bool _starting = false;
  bool _stopping = false;
  bool _polling = false;
  bool _disposed = false;
  bool _historyResynced = false;
  String? _connectionError;
  String? _pendingInputId;
  String? _pendingKey;
  String? _notifiedTerminal;
  Timer? _timer;
  int _epoch = 0;
  int _ticks = 0;

  ChatRunView get state => ChatRunView(
    run: _run,
    phase: _phase,
    draft: _draft,
    tools: List.unmodifiable(_tools.values),
    restoring: _restoring,
    starting: _starting,
    stopping: _stopping,
    connectionError: _connectionError,
    pendingInputId: _pendingInputId,
    historyResynced: _historyResynced,
  );

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  bool _current(int epoch) => !_disposed && epoch == _epoch;

  void _acceptSnapshot(ChatRunDto? snapshot, {bool replace = false}) {
    if (snapshot != null && snapshot.conversationId != conversationId) {
      throw StateError('wrong conversation');
    }
    final previous = _run;
    if (previous != null && snapshot != null) {
      if ((!replace && previous.id != snapshot.id) ||
          (previous.id == snapshot.id &&
              (snapshot.sequence < previous.sequence ||
                  (!chatRunIsActive(previous.status) &&
                      snapshot.status != previous.status)))) {
        return;
      }
    }
    if (snapshot?.id != previous?.id) {
      _cursor = 0;
      _draftSequence = 0;
      _tools.clear();
      _historyResynced = false;
    }
    _run = snapshot;
    if (snapshot == null) {
      _draft = '';
      _tools.clear();
      return;
    }
    if (snapshot.sequence >= _draftSequence) {
      _draft = snapshot.content;
      _draftSequence = snapshot.sequence;
      _phase = snapshot.phase;
    }
    if (!chatRunIsActive(snapshot.status) && _notifiedTerminal != snapshot.id) {
      _notifiedTerminal = snapshot.id;
      onTerminal?.call();
    }
  }

  Future<void> restore() async {
    if (_disposed || !repository.supportsChatRuns || _starting) return;
    final epoch = ++_epoch;
    _timer?.cancel();
    _restoring = true;
    _notify();
    try {
      final snapshot = await repository.getLatestChatRun(conversationId);
      if (!_current(epoch)) return;
      _acceptSnapshot(snapshot, replace: true);
      _connectionError = null;
    } catch (_) {
      if (_current(epoch)) {
        _connectionError = '无法读取生成状态，请重新连接；不会自动重新生成。';
      }
    } finally {
      if (_current(epoch)) {
        _restoring = false;
        _notify();
        _schedule();
      }
    }
  }

  Future<void> start(String inputMessageId) async {
    if (_disposed || _starting || _stopping || !repository.supportsChatRuns) {
      return;
    }
    if (_run != null && chatRunIsActive(_run!.status) && _pendingKey == null) {
      return;
    }
    if (_pendingInputId != null && _pendingInputId != inputMessageId) return;
    final epoch = ++_epoch;
    _timer?.cancel();
    _restoring = false;
    _starting = true;
    _pendingInputId = inputMessageId;
    _pendingKey ??=
        'chat-${DateTime.now().microsecondsSinceEpoch}-${Random.secure().nextInt(1 << 32)}';
    _connectionError = null;
    _notify();
    try {
      final snapshot = await repository.startChatRun(
        conversationId,
        inputMessageId,
        _pendingKey!,
      );
      if (!_current(epoch)) return;
      if (snapshot.inputMessageId != inputMessageId) {
        throw StateError('wrong input');
      }
      _acceptSnapshot(snapshot, replace: true);
      _pendingInputId = null;
      _pendingKey = null;
    } catch (_) {
      if (_current(epoch)) {
        // The request may have committed. Retain its exact key for re-delivery.
        _connectionError = '启动结果待核实。重新连接会查询同一次请求，不会重复保存输入。';
      }
    } finally {
      if (_current(epoch)) {
        _starting = false;
        _notify();
        _schedule();
      }
    }
  }

  Future<void> reconnect() =>
      _pendingInputId == null ? restore() : start(_pendingInputId!);

  Future<void> retry() async {
    final view = state;
    if (view.canRetry) await start(view.run!.inputMessageId);
  }

  Future<void> stop() async {
    final run = _run;
    if (_disposed || _stopping || run == null || !chatRunIsActive(run.status)) {
      return;
    }
    final epoch = _epoch;
    _stopping = true;
    _notify();
    try {
      final snapshot = await repository.cancelChatRun(conversationId, run.id);
      if (!_current(epoch)) return;
      _acceptSnapshot(snapshot);
      _connectionError = null;
    } catch (_) {
      if (_current(epoch)) {
        _connectionError = '停止请求尚未确认，请重新连接查看状态。';
      }
    } finally {
      if (_current(epoch)) {
        _stopping = false;
        _notify();
        _schedule();
      }
    }
  }

  void _schedule() {
    _timer?.cancel();
    if (_disposed || _pendingKey != null) return;
    if (_connectionError != null ||
        (_run != null &&
            (chatRunIsActive(_run!.status) || _cursor < _run!.sequence))) {
      _timer = Timer(pollInterval, poll);
    }
  }

  /// At most one page and one snapshot per tick; no overlapping polls or
  /// unbounded catch-up loop for a slow consumer.
  Future<void> poll() async {
    if (_disposed || _starting || _restoring) return;
    if (_polling) {
      _schedule();
      return;
    }
    final run = _run;
    if (run == null) return;
    final epoch = _epoch;
    _polling = true;
    try {
      final page = await repository.listChatRunEvents(
        conversationId,
        run.id,
        _cursor,
        limit: 64,
      );
      if (!_current(epoch)) return;
      if (page.runId != run.id) throw StateError('wrong run');
      var needsSnapshot = false;
      var gap = false;
      for (final event in page.events) {
        if (event.runId != run.id) throw StateError('wrong event run');
        if (event.sequence <= _cursor) continue;
        if (event.sequence != _cursor + 1) {
          needsSnapshot = true;
          gap = true;
          _historyResynced = true;
          break;
        }
        _applyEvent(event);
        _cursor = event.sequence;
        if (const {
          'completed',
          'failed',
          'cancelled',
          'interrupted',
        }.contains(event.kind)) {
          needsSnapshot = true;
        }
      }
      if (page.nextSequence != _cursor && !gap) {
        needsSnapshot = true;
        gap = true;
        _historyResynced = true;
      }
      // Periodic snapshots also discover terminal state when its event is lost.
      if (needsSnapshot || ++_ticks % 4 == 0 || page.events.isEmpty) {
        final snapshot = await repository.getChatRun(conversationId, run.id);
        if (!_current(epoch)) return;
        _acceptSnapshot(snapshot);
        if (snapshot != null &&
            (gap ||
                (!chatRunIsActive(snapshot.status) &&
                    page.events.isEmpty &&
                    _cursor < snapshot.sequence))) {
          _cursor = snapshot.sequence;
          _historyResynced = true;
        }
      }
      _connectionError = null;
    } catch (_) {
      if (_current(epoch)) {
        _connectionError = '生成状态暂不可用；已保存的输入不会重发。';
        try {
          final snapshot = await repository.getChatRun(conversationId, run.id);
          if (!_current(epoch)) return;
          _acceptSnapshot(snapshot);
          _cursor = snapshot?.sequence ?? 0;
          _historyResynced = true;
          _connectionError = null;
        } catch (_) {
          // Keep the last authoritative state. A connection failure is not a
          // failed generation and never enables automatic write-tool replay.
        }
      }
    } finally {
      _polling = false;
      if (_current(epoch)) {
        _notify();
        _schedule();
      }
    }
  }

  void _applyEvent(ChatRunEventDto event) {
    final payload = jsonDecode(event.payloadJson);
    if (payload is! Map<String, dynamic>) throw StateError('invalid event');
    final active = _run != null && chatRunIsActive(_run!.status);
    if (active && event.sequence > _draftSequence) {
      if (event.kind == 'draft_reset') _draft = '';
      if (event.kind == 'text_delta' && payload['text'] is String) {
        _draft += payload['text'] as String;
      }
      if (event.kind == 'phase' && payload['phase'] is String) {
        _phase = payload['phase'] as String;
      }
      if (event.kind == 'request_started') _phase = 'requesting';
      if (event.kind == 'cancelling') _phase = 'cancelling';
      _draftSequence = event.sequence;
    }
    if (event.kind == 'tool_started' ||
        event.kind == 'tool_result' ||
        event.kind == 'tool_uncertain') {
      final id = payload['call_id'];
      final name = payload['name'];
      if (id is! String || name is! String) return;
      final outcome = event.kind == 'tool_started'
          ? 'running'
          : payload['outcome'] as String? ?? 'unknown';
      _tools[id] = ChatToolProgress(
        id,
        name,
        outcome,
        toolProgressDetails(name, outcome, payload['content']),
      );
      while (_tools.length > 20) {
        _tools.remove(_tools.keys.first);
      }
      if (outcome == 'proposal_ready') onProposal?.call();
    }
    if (event.kind == 'proposal_ready') onProposal?.call();
  }

  @override
  void dispose() {
    _disposed = true;
    _epoch++;
    _timer?.cancel();
    super.dispose();
  }
}

final wikiChatMessagesProvider = FutureProvider.autoDispose
    .family<List<Message>, String>((ref, id) {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.listMessages(id);
    });

final chatRunControllerProvider = Provider.autoDispose
    .family<ChatRunController, String>((ref, id) {
      final repository = ref.watch(conversationRepositoryProvider);
      final controller = ChatRunController(
        repository,
        id,
        onTerminal: () {
          ref.invalidate(messagesProvider);
          ref.invalidate(wikiChatMessagesProvider(id));
          ref.invalidate(conversationsProvider);
          ref.invalidate(pendingActionsProvider);
          ref.invalidate(wikiPagesProvider);
          ref.invalidate(wikiPageProvider);
          ref.invalidate(wikiDerivativesProvider);
          ref.invalidate(pageRelationsProvider);
          ref.invalidate(todosProvider);
          ref.invalidate(activeAiProviderProvider);
          ref.invalidate(todayTokenUsageProvider);
        },
        onProposal: () => ref.invalidate(pendingActionsProvider),
      );
      void queuedRunStarted() {
        if (repository.chatQueueRunStarted.value?.conversationId == id) {
          unawaited(controller.restore());
        }
      }

      repository.chatQueueRunStarted.addListener(queuedRunStarted);
      ref.onDispose(() {
        repository.chatQueueRunStarted.removeListener(queuedRunStarted);
        controller.dispose();
      });
      Future.microtask(controller.restore);
      return controller;
    });

final chatRunStateProvider = StreamProvider.autoDispose
    .family<ChatRunView, String>((ref, id) {
      final controller = ref.watch(chatRunControllerProvider(id));
      return Stream<ChatRunView>.multi((sink) {
        void changed() => sink.add(controller.state);
        controller.addListener(changed);
        changed();
        sink.onCancel = () => controller.removeListener(changed);
      });
    });
