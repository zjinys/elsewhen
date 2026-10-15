import 'dart:async';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;
import 'sync_key_store.dart';
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
    if (!_closed &&
        !_disposed &&
        _ownedOperation != null &&
        state?.state == 'local_ready') {
      unawaited(enrollment.arm(operationId: _ownedOperation));
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

  Future<void> restoreIfConfigured() async {
    await refresh();
    if (!_disposed && !_closed && state?.state == 'locked') await unlock();
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
    if (_disposed) return;
    _disposed = true;
    onReady = null;
    enrollment.dispose();
    joiner.dispose();
    unawaited(close());
    super.dispose();
  }
}
