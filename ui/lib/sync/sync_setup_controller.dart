import 'dart:async';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;
import 'sync_key_store.dart';
import 'sync_keep_alive.dart';
import 'sync_pairing_api.dart';
import 'sync_join_api.dart';
import 'sync_join_controller.dart';
import 'sync_enrollment_api.dart';
import 'sync_enrollment_controller.dart';

part 'sync_setup_pairing.dart';

abstract interface class SyncSetupApi {
  Future<api.LocalSyncStateDto> state();
  Future<api.SyncKeyRequestDto> prepare({
    String? name,
    String? platform,
    required bool consent,
  });
  Future<api.SyncKeyCommitDto> install(
    String token,
    List<int> key,
    int revision,
  );
  Future<api.LocalSyncStateDto> confirm(String token, int revision);
  Future<bool> release(String token);
  Future<api.LocalSyncStateDto> lock();

  /// owner 改设备显示名（vault 修订号由调用方在改前读取，失败关闭）。
  Future<int> renameDevice(String deviceId, String name, int vaultRevision);
}

class FrbSyncSetupApi implements SyncSetupApi {
  @override
  Future<api.LocalSyncStateDto> state() => api.getLocalSyncState();
  @override
  Future<api.SyncKeyRequestDto> prepare({
    String? name,
    String? platform,
    required bool consent,
  }) => api.prepareSyncKeyRequest(
    name: name,
    platform: platform,
    acknowledgeProviderKeySharing: consent,
  );
  @override
  Future<api.SyncKeyCommitDto> install(
    String token,
    List<int> key,
    int revision,
  ) => api.installSyncKey(token: token, key: key, protectedRevision: revision);
  @override
  Future<api.LocalSyncStateDto> confirm(String token, int revision) =>
      api.confirmSyncKey(token: token, protectedRevision: revision);
  @override
  Future<bool> release(String token) => api.releaseSyncKeyRequest(token: token);
  @override
  Future<api.LocalSyncStateDto> lock() => api.lockLocalSync();
  @override
  Future<int> renameDevice(
    String deviceId,
    String name,
    int vaultRevision,
  ) async {
    final revision = await api.renameSyncDevice(
      deviceId: deviceId,
      newName: name,
      expectedVaultRevision: vaultRevision,
    );
    return revision;
  }
}

/// Settings and startup share one controller; closing an old screen cannot
/// release a newer key operation. Pairing traffic requires an explicit action.
class SyncSetupController extends ChangeNotifier {
  SyncSetupController({
    SyncKeyStore? keyStore,
    SyncSetupApi? bridge,
    SyncPairingApi? pairingBridge,
    SyncJoinApi? joinBridge,
    SyncEnrollmentApi? enrollmentBridge,
    this.autoEnrollment = false,
    this.previewEnabled = // 默认开启（P4d 全链路已完成）；发布门禁前如需关断：--dart-define=ELSEWHEN_SYNC_PREVIEW=false
    const bool.fromEnvironment(
      'ELSEWHEN_SYNC_PREVIEW',
      defaultValue: true,
    ),
  }) : _keys = keyStore ?? SyncKeyStore(),
       _bridge = bridge ?? FrbSyncSetupApi(),
       _pairing = pairingBridge ?? FrbSyncPairingApi(),
       _joinBridge = joinBridge ?? FrbSyncJoinApi(),
       _enrollmentBridge = enrollmentBridge ?? FrbSyncEnrollmentApi();

  final bool autoEnrollment;
  final SyncEnrollmentApi _enrollmentBridge;
  late final SyncEnrollmentController enrollment = SyncEnrollmentController(
    keyStore: _keys,
    bridge: _enrollmentBridge,
    enabled: autoEnrollment,
    isSessionBusy: () => busy || joiner.busy,
    onSessionReleased: (released) {
      if (released.role == 'owner' && released.operationId == _ownedOperation) {
        _ownedOperation = null;
      }
      joiner.enrollmentReleased(released);
    },
    onStateChanged: () => unawaited(_refreshEnrollmentState()),
  );
  final SyncJoinApi _joinBridge;
  late final SyncJoinController joiner = SyncJoinController(
    keyStore: _keys,
    bridge: _joinBridge,
    previewEnabled: previewEnabled,
    enrollment: enrollment,
    isSessionBusy: () => busy,
  );
  final SyncKeyStore _keys;

  /// 供诊断探针复用同一个注入的 key store（测试可注入替身）。
  SyncKeyStore get keys => _keys;
  final SyncSetupApi _bridge;
  final SyncPairingApi _pairing;
  api.SyncInvitationOfferDto? invitation;
  api.SyncInvitationPreviewDto? pairingPreview;
  String? pendingPairingId;

  /// 审批是否已获中继投递确认；未确认时由邀请页轮询自动重投。
  bool approvalDelivered = false;
  String? pairingNotice;
  final bool previewEnabled;
  api.LocalSyncStateDto? state;
  String? error;
  bool busy = false;
  bool _disposed = false;
  bool _closed = false;
  String? _ownedOperation;
  Completer<void>? _activationDone;
  VoidCallback? onReady;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> _refreshEnrollmentState() async {
    if (_closed || _disposed) return;
    if (joiner.active) {
      await joiner.refreshFromEnrollment();
      return;
    }
    final operation = _ownedOperation;
    try {
      final next = await _bridge.state();
      if (_closed || _disposed || busy || _ownedOperation != operation) return;
      final changed = state?.vaultRevision != next.vaultRevision;
      state = next;
      _changed();
      if (changed && next.state == 'local_ready') onReady?.call();
    } catch (_) {
      // Keep the last metadata view; never turn a failed read into activation.
    }
  }

  void _resumeEnrollment() {
    // joiner 有自己的驱动器（joiner.active 自会 arm），这里只负责 owner
    // 侧的收尾协调；否则两端会同时空转抢同一个原生工作区。
    if (!_closed && !_disposed && !joiner.active) {
      // 只有本机已就绪（解锁/恢复后 local_ready）才允许驱动。
      if (state?.state == 'local_ready') {
        // 注意不能要求 _ownedOperation 非空：app 重启/恢复只会恢复
        // 原生会话，不会重置这个 Dart 持有标记。若强制要求它，批准后
        // 的收尾协调（取 proof → 授名册 → 发快照）就永远没人驱动，
        // 新设备会卡在「等待成员授权与初始同步」致中继反复重投。
        // arm(operationId: null) 会由控制器从库存采用原生会话的操作号。
        unawaited(enrollment.arm(operationId: _ownedOperation));
      }
    }
  }

  Future<void> refresh() async {
    if (busy || _disposed || _closed) return;
    busy = true;
    error = null;
    _changed();
    try {
      state = await _bridge.state();
    } catch (_) {
      error = '无法读取本机同步状态；原有记录未改动。';
    } finally {
      busy = false;
      _changed();
    }
  }

  Listenable? _backlogSource;
  bool _backlogRefreshing = false;

  /// 订阅「投递进度」信号，让积压口径跟着走。
  ///
  /// 状态快照（`state`）是一次性的；而积压是**动**的——封包发出、对端回执、
  /// 重试都会改 `sync_wire_outbox.state='pending'` 的行数。真机现场：手机侧
  /// 明明有 1 条未回执，主界面状态位却整整 8 分钟不出现，重启 app 才显示
  /// 「对端未确认 1 条 · 2.7 KB」——一个只在重启时正确的指示器，等于没有。
  /// 挂在每拍都会变的 `syncDeliveryStatus` 上，事件驱动就不需要轮询。
  void attachBacklogSource(Listenable changes) {
    if (identical(_backlogSource, changes)) return;
    _backlogSource?.removeListener(_onBacklogTick);
    _backlogSource = changes;
    changes.addListener(_onBacklogTick);
  }

  void _onBacklogTick() => unawaited(refreshBacklog());

  /// 静默重读快照：只更新积压相关字段，不动 `busy` / `error`。
  ///
  /// 不用 [refresh]：那个会把 `busy` 翻一遍，而它每拍都可能被触发，
  /// 设置页的按钮会跟着闪。值没变就不 notify，避免无意义的整树重建。
  Future<void> refreshBacklog() async {
    if (_disposed || _closed || _backlogRefreshing || busy || joiner.active) {
      return;
    }
    _backlogRefreshing = true;
    try {
      final next = await _bridge.state();
      if (_disposed || _closed) return;
      final previous = state;
      if (previous != null &&
          previous.state == next.state &&
          previous.pendingWireEnvelopes == next.pendingWireEnvelopes &&
          previous.pendingBytes == next.pendingBytes) {
        return;
      }
      state = next;
      _changed();
    } catch (_) {
      // 读不到就保留上一次（与 refresh() 同策略：别把失败显示成清零）。
    } finally {
      _backlogRefreshing = false;
    }
  }

  Future<void> restoreIfConfigured() async {
    await refresh();
    if (!_disposed && !_closed && state?.state == 'locked') {
      await unlock();
    }
    // 启动/重启自愈：若重启时配对协调还在收尾（批准已投递但名册未落），
    // 恢复会话后必须把 enrollment 驱动器重新装上去，否则新设备会无限
    // 等待「成员授权与初始同步」。
    if (!_disposed && !_closed) _resumeEnrollment();
  }

  /// owner 改设备显示名。成功返回 null 并刷新状态；失败返回可读原因。
  /// 修订号在改前读取，由 Rust 端失败关闭防盲写。
  Future<String?> renameLocalDevice(String deviceId, String name) async {
    final current = state;
    if (current == null || _disposed || _closed) return '本机同步尚未就绪。';
    final trimmed = name.trim();
    if (trimmed.isEmpty || trimmed.length > 256) return '设备名需为 1–256 个字符。';
    try {
      await _bridge.renameDevice(deviceId, trimmed, current.vaultRevision);
    } catch (error) {
      final text = error.toString();
      if (text.contains('sync_vault_revision_changed')) {
        return '状态刚被其他操作更新，请刷新后重试。';
      }
      if (text.contains('sync_rename_requires_unlock')) {
        return '本机密钥已锁定，请先解锁再改名。';
      }
      if (text.contains('pairing_device_name_invalid')) {
        return '设备名包含无效字符，请换一个。';
      }
      return '改名失败：$text';
    }
    await refresh();
    return null;
  }

  Future<void> createOwner(
    String name,
    String platform, {
    required bool consent,
  }) async {
    if (!previewEnabled || !consent || name.trim().isEmpty) {
      error = !previewEnabled
          ? '设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。'
          : '填写设备名称，并确认配对将共享 Provider API Key。';
      _changed();
      return;
    }
    await _activate(name: name.trim(), platform: platform, consent: true);
  }

  Future<void> unlock() => _activate(consent: false);

  Future<void> _activate({
    String? name,
    String? platform,
    required bool consent,
  }) async {
    if (busy || _disposed || _closed) return;
    if (joiner.active || joiner.busy) {
      error = '请先停止本机加入流程，再准备或解锁当前数据集。';
      _changed();
      return;
    }
    busy = true;
    error = null;
    _changed();
    String? operation;
    final done = Completer<void>();
    _activationDone = done;
    SyncMasterKey? key;
    // 准备/解锁会写密钥库与 vault 修订；被冻结在中间会留下未确认的会话。
    SyncKeepAlive.hold('sync-setup', text: '正在准备本机同步身份');
    try {
      await enrollment.pause();
      if (_disposed || _closed) throw StateError('closed');
      final request = await _bridge.prepare(
        name: name,
        platform: platform,
        consent: consent,
      );
      operation = request.token;
      _ownedOperation = operation;
      if (_disposed || _closed) throw StateError('closed');
      int revision;
      if (request.creating) {
        key = await _keys.createForNewInstallation(request.installationId);
        if (_disposed || _closed) throw StateError('closed');
        await _keys.saveRevision(request.installationId, 1);
        revision = 1;
      } else {
        key = await _keys.load(request.installationId);
        revision = await _keys.readRevision(request.installationId);
      }
      if (_disposed || _closed) throw StateError('closed');
      final bytes = key.copyBytesForRust();
      late final api.SyncKeyCommitDto commit;
      try {
        commit = await _bridge.install(request.token, bytes, revision);
      } finally {
        bytes.fillRange(0, bytes.length, 0);
      }
      if (_disposed || _closed) throw StateError('closed');
      await _keys.saveRevision(commit.installationId, commit.revision);
      final storedRevision = await _keys.readRevision(commit.installationId);
      if (_disposed || _closed) throw StateError('closed');
      state = await _bridge.confirm(commit.token, storedRevision);
      if (_disposed || _closed) throw StateError('closed');
      onReady?.call();
    } catch (failure) {
      if (operation != null) {
        try {
          await _bridge.release(operation);
        } catch (_) {
          /* Fail closed; never reset keys. */
        }
        if (_ownedOperation == operation) _ownedOperation = null;
      }
      error = _message(failure);
      if (!_disposed) {
        try {
          state = await _bridge.state();
        } catch (_) {
          /* Keep the last read-only view. */
        }
      }
    } finally {
      key?.dispose();
      busy = false;
      done.complete();
      if (identical(_activationDone, done)) _activationDone = null;
      _changed();
      SyncKeepAlive.release('sync-setup');
      _resumeEnrollment();
    }
  }

  Future<void> lock() async {
    if (busy || _disposed || _closed || joiner.active) return;
    busy = true;
    error = null;
    final done = Completer<void>();
    _activationDone = done;
    _changed();
    try {
      await enrollment.pause();
      if (_disposed || _closed) throw StateError('closed');
      final operation = _ownedOperation;
      if (operation != null) await _bridge.release(operation);
      state = await _bridge.state();
      invitation = null;
      pairingPreview = null;
      pairingNotice = null;
      _ownedOperation = null;
    } catch (_) {
      error = '锁定操作未确认，请重试；不会重新生成密钥。';
    } finally {
      busy = false;
      done.complete();
      if (identical(_activationDone, done)) _activationDone = null;
      _changed();
    }
  }

  Future<void> close() async {
    _closed = true;
    invitation = null;
    pairingPreview = null;
    await enrollment.close();
    await joiner.close();
    await _activationDone?.future;
    SyncKeepAlive.releaseAll();
    final token = _ownedOperation;
    _ownedOperation = null;
    if (token != null) {
      try {
        await _bridge.release(token);
      } catch (_) {
        /* Process exit also releases its lease. */
      }
    }
  }

  String _message(Object error) {
    if (error is SyncKeyStoreException) {
      return switch (error.code) {
        SyncKeyStoreError.missing => '缺少本机密钥或受保护的修订记录。请通过原设备恢复；不会生成替代密钥。',
        SyncKeyStoreError.revisionRollback => '密钥修订发生回退，已停止解锁。请检查恢复的数据，原记录保留。',
        SyncKeyStoreError.corrupt => '系统密钥记录损坏，已停止解锁。原数据和密钥不会被覆盖。',
        _ => '系统密钥库不可用或已锁定。解锁系统密钥库后重试，仍可保存本地记录。',
      };
    }
    final text = error.toString();
    if (text.contains('sync_runtime_in_use') ||
        text.contains('sync_runtime_request_already_open')) {
      return '另一个窗口或后台任务正在使用此工作区，请等待结束后重试。';
    }
    if (text.contains('sync_background_processing_must_finish')) {
      return '后台任务仍在执行，完成后再准备同步；本次未切换加工归属。';
    }
    if (text.contains('sync_runtime_setup_requires_repair') ||
        text.contains('sync_vault_restored_revision')) {
      return '本机同步身份需要恢复，已停止自动初始化。请保留原库并通过授权设备重新加入。';
    }
    return '本机同步准备未完成，已保持锁定。请重试；原记录保留。';
  }

  @override
  void dispose() {
    _backlogSource?.removeListener(_onBacklogTick);
    _backlogSource = null;
    if (_disposed) return;
    _disposed = true;
    onReady = null;
    enrollment.dispose();
    joiner.dispose();
    unawaited(close());
    super.dispose();
  }
}
