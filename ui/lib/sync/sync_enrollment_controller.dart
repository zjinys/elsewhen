import 'dart:async';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;
import 'sync_enrollment_api.dart';
import 'sync_keep_alive.dart';
import 'sync_key_store.dart';

class _EnrollmentCancelled implements Exception {}

class _EnrollmentStateChanged implements Exception {}

/// Drives only already-approved enrollment records. Construction does no I/O;
/// the repository explicitly arms it after native unlock or pairing progress.
class SyncEnrollmentController extends ChangeNotifier {
  SyncEnrollmentController({
    required SyncKeyStore keyStore,
    SyncEnrollmentApi? bridge,
    this.enabled = true,
    this.interval = const Duration(seconds: 2),
    this.stalledTick = const Duration(seconds: 45),
    this.isSessionBusy,
    this.onSessionReleased,
    this.onStateChanged,
  }) : _keys = keyStore,
       _bridge = bridge ?? FrbSyncEnrollmentApi();

  final SyncKeyStore _keys;
  final SyncEnrollmentApi _bridge;
  final bool enabled;
  final Duration interval;

  /// 单轮 tick 的存活上限。正常一轮（一次原生命令 + 一次中继往返）在秒级完成，
  /// 超过这个时长只剩两种可能：中继半开连接，或原生侧卡在锁上不返回。
  /// 没有这道看门狗时，轮询链会被一次不返回的调用**永久掐断**：
  /// `_schedule()` 只在 tick 的 finally 里排下一轮，卡住就等于整条链停摆——
  /// 界面停在最后一帧（例如「等待成员授权与初始同步 0/174」），
  /// `pause()`/`close()` 也会卡在 `await _flight.future` 上，导致前台服务
  /// 通知永远撤不掉。实测 2026-10-24 的一次配对就是被它拖死的。
  final Duration stalledTick;
  final bool Function()? isSessionBusy;
  final void Function(api.SyncEnrollmentStateDto state)? onSessionReleased;
  final VoidCallback? onStateChanged;
  List<api.SyncEnrollmentStateDto> states = [];
  String? error;
  bool paused = false;
  bool _armed = false;
  bool _closed = false;
  bool _disposed = false;
  int _epoch = 0;
  int _failures = 0;
  int _cursor = 0;
  String? _operationId;
  Timer? _timer;
  Completer<void>? _flight;
  bool get busy => _flight != null;

  static const String _keepAliveReason = 'sync-enrollment';

  /// 交接期间挂上前台服务：初始快照是分多轮 tick 搬几十 MB，等待对端批准
  /// 也要靠 2s 轮询才能感知；进程被 ROM 冻结就永远停在半路。只要还有未
  /// 完成的交接记录就挂着，全部结束/过期/需修复时立刻撤掉
  /// （见 sync_keep_alive.dart 的说明）。
  void _syncKeepAlive() {
    if (_armed && states.any(_needsWork)) {
      SyncKeepAlive.hold(_keepAliveReason, text: '正在完成设备交接与初始同步');
    } else {
      SyncKeepAlive.release(_keepAliveReason);
    }
  }

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  bool _needsWork(api.SyncEnrollmentStateDto state) {
    if (state.confirmationToken != null) return true;
    if (const ['expired', 'revoked', 'requires_repair'].contains(state.phase)) {
      return false;
    }
    return state.phase != 'complete' || state.notificationPending;
  }

  void _requireCurrent(int epoch) {
    if (_closed || !_armed || epoch != _epoch) throw _EnrollmentCancelled();
  }

  void _validate(
    api.SyncEnrollmentStateDto prior,
    api.SyncEnrollmentStateDto next,
  ) {
    if (prior.role != next.role ||
        prior.operationId != next.operationId ||
        prior.invitationId != next.invitationId ||
        prior.installationId != next.installationId ||
        next.revision < prior.revision ||
        (next.revision > prior.revision && next.confirmationToken == null)) {
      throw _EnrollmentStateChanged();
    }
  }

  void _replace(api.SyncEnrollmentStateDto next) {
    states = [
      for (final state in states)
        if (state.role == next.role &&
            state.operationId == next.operationId &&
            state.invitationId == next.invitationId)
          next
        else
          state,
    ];
    _changed();
  }

  Future<void> arm({String? operationId}) async {
    if (!enabled || _closed) return;
    if (operationId != null && operationId != _operationId) {
      _epoch++;
      _operationId = operationId;
      states = [];
    }
    _armed = true;
    paused = false;
    _failures = 0;
    _timer?.cancel();
    _timer = null;
    final running = _flight;
    if (running != null) {
      await running.future;
      return;
    }
    await _tick();
  }

  void _schedule() {
    _timer?.cancel();
    if (!_armed || _closed) return;
    final milliseconds = (interval.inMilliseconds * (1 << _failures))
        .clamp(1, 60000)
        .toInt();
    _timer = Timer(Duration(milliseconds: milliseconds), () {
      _timer = null;
      unawaited(_tick());
    });
  }

  Future<void> _tick() async {
    if (!_armed || _closed || busy) return;
    if (isSessionBusy?.call() == true) {
      _schedule();
      return;
    }
    final epoch = _epoch;
    final done = Completer<void>();
    _flight = done;
    // 原生调用没法取消，但**卡住不许把整条轮询链带走**：到点后丢弃这一轮
    // （epoch 前进让迟到的回包作废）并重新排队。
    final watchdog = Timer(stalledTick, () => _onStalled(done));
    api.SyncEnrollmentStateDto? target;
    error = null;
    _changed();
    try {
      final inventory = await _bridge.list();
      _requireCurrent(epoch);
      if (inventory.length > 16) throw _EnrollmentStateChanged();
      if (inventory.isNotEmpty) {
        final scope = inventory.first;
        if (_operationId != null && scope.operationId != _operationId) {
          throw _EnrollmentStateChanged();
        }
        _operationId ??= scope.operationId;
        final invitations = <String>{};
        for (final state in inventory) {
          if (!const ['owner', 'joiner'].contains(state.role) ||
              state.role != scope.role ||
              state.operationId.isEmpty ||
              state.operationId != scope.operationId ||
              state.installationId.isEmpty ||
              state.installationId != scope.installationId ||
              state.revision <= 0 ||
              state.revision != scope.revision ||
              state.invitationId.isEmpty ||
              !invitations.add(state.invitationId)) {
            throw _EnrollmentStateChanged();
          }
        }
      }
      states = List.of(inventory);
      final pending = inventory.where(_needsWork).toList();
      if (pending.isEmpty) {
        _armed = false;
        return;
      }
      // 本轮 advance 可能就是搬快照分片的那一次，所以先挂保活再做网络。
      SyncKeepAlive.hold(_keepAliveReason, text: '正在完成设备交接与初始同步');
      // 备选原始规则总是先取第一条 awaiting_proof；留有多条交接时
      //（例如换邀请重试后旧的还挂着）旧记录永远占住队首，新的会被
      // 饿死。改为：提交确认最优先，其余按游标轮转，卡住的记录只
      // 消耗少量轮次。
      target = pending.firstWhere(
        (state) => state.confirmationToken != null,
        orElse: () => pending[_cursor++ % pending.length],
      );
      var next = target;
      if (next.confirmationToken == null) {
        final before = next.phase;
        next = await _bridge.advance(target);
        _requireCurrent(epoch);
        _validate(target, next);
        _replace(next);
        // worker 日志纪律：高频 tick 只在「有事发生」时 debugPrint。
        // 悬空邀请（如 joiner 永不回 proof 的历史邀请）会让 phase
        // 原地不动地被无限 advance——实测三个悬空邀请一天墨 1.3 万
        // 行 advance/advanced。phase 无变化就不打。
        if (next.phase != before) {
          debugPrint(
            '[sync-pairing] enrollment: advanced role=${next.role} '
            'inv=${next.invitationId} phase=$before -> ${next.phase}',
          );
        }
      }
      if (next.confirmationToken != null) {
        final commit = next;
        _requireCurrent(epoch);
        debugPrint(
          '[sync-pairing] enrollment: commit-invite role=${commit.role} '
          'inv=${commit.invitationId} rev=${commit.revision}',
        );
        await _keys.saveRevision(commit.installationId, commit.revision);
        final protected = await _keys.readRevision(commit.installationId);
        _requireCurrent(epoch);
        if (protected != commit.revision) throw _EnrollmentStateChanged();
        final confirmed = await _bridge.confirm(commit, protected);
        _requireCurrent(epoch);
        _validate(commit, confirmed);
        if (confirmed.revision != commit.revision ||
            confirmed.confirmationToken != null) {
          throw _EnrollmentStateChanged();
        }
        next = confirmed;
      }
      _replace(next);
      _failures = 0;
      if (!states.any(_needsWork)) _armed = false;
      onStateChanged?.call();
    } on _EnrollmentCancelled {
      // Parent pause/close waits for native writes, then releases its own session.
    } catch (failure) {
      debugPrint('[sync-pairing] enrollment: tick failed: $failure');
      final nativeFailure =
          failure is SyncKeyStoreException ||
          failure is _EnrollmentStateChanged;
      if (failure.toString().contains(
        'sync_enrollment_authorization_changed_requires_repair',
      )) {
        paused = true;
        _armed = false;
        error = '正式通道的授权或修订已改变，已停止自动交接。请检查原设备与中继授权；不会读取新修订后覆盖权限。';
      } else if (nativeFailure) {
        paused = true;
        _armed = false;
        error = '交接的系统修订或身份未能确认，已暂停。请解锁本机或恢复原暂存项；不会重置密钥。';
        if (target != null && !_closed && epoch == _epoch) {
          try {
            final released = await _bridge.release(target);
            if (released) onSessionReleased?.call(target);
            onStateChanged?.call();
          } catch (_) {
            error = '系统修订未确认，且无法确认本机会话已释放。请保留数据并关闭、重开应用后恢复。';
          }
        }
      } else {
        _failures = (_failures + 1).clamp(0, 5);
        error = '交接步骤暂未完成，稍后按原请求重试；不会重新审批或更换密钥。';
      }
    } finally {
      watchdog.cancel();
      if (identical(_flight, done)) _flight = null;
      // 看门狗可能已经完成过它（那一轮被判定失联），重复 complete 会抛
      // StateError，把 finally 自己也炸掉。
      if (!done.isCompleted) done.complete();
      _changed();
      _syncKeepAlive();
      _schedule();
    }
  }

  /// 看门狗时长的中文描述（单测用毫秒级阈值，别写成「0 秒」）。
  String get _stalledLabel => stalledTick.inSeconds >= 1
      ? '${stalledTick.inSeconds} 秒'
      : '${stalledTick.inMilliseconds} 毫秒';

  /// 一次 tick 被判定失联后的自救：只丢弃结果并重排下一轮，不试图取消
  /// 那个卡住的原生调用（做不到）。原因写进 [error]，界面就能从「一直在转圈」
  /// 变成一句能看懂的话。
  void _onStalled(Completer<void> done) {
    if (_closed || !identical(_flight, done)) return;
    _epoch++;
    _flight = null;
    _failures = (_failures + 1).clamp(0, 5);
    error = '同步步骤超过 $_stalledLabel没有响应，已重新排队重试；数据没有丢失。';
    debugPrint(
      '[sync-pairing] enrollment: tick stalled > $stalledTick, '
      'requeued (phase=${states.isEmpty ? 'none' : states.first.phase})',
    );
    if (!done.isCompleted) done.complete();
    _changed();
    _schedule();
  }

  /// Cancels future ticks and waits for the in-flight native write or RPC. It does
  /// not revoke remote membership and can be armed again after local unlock.
  Future<void> pause() async {
    _armed = false;
    _epoch++;
    _timer?.cancel();
    _timer = null;
    final flight = _flight;
    try {
      // 只等有限时间：原生调用失联时，无限等下去会让关闭/锁定一起挂住，
      // 后面的 SyncKeepAlive.release 永远执行不到（通知栏撤不掉）。
      await flight?.future.timeout(stalledTick);
    } on TimeoutException {
      debugPrint(
        '[sync-pairing] enrollment: pause 放弃等待失联的 tick '
        '(${stalledTick.inSeconds}s)',
      );
    } finally {
      SyncKeepAlive.release(_keepAliveReason);
    }
  }

  Future<void> close() async {
    _closed = true;
    await pause();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    unawaited(close());
    super.dispose();
  }
}
