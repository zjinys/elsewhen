import '../l10n/notice_messages.dart';

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;
import 'sync_join_api.dart';
import 'sync_keep_alive.dart';
import 'sync_key_store.dart';
import 'sync_enrollment_controller.dart';

/// A join owns only its staging operation, never the active dataset or its worker.
class SyncJoinController extends ChangeNotifier {
  SyncJoinController({
    SyncKeyStore? keyStore,
    SyncJoinApi? bridge,
    this.enrollment,
    this.isSessionBusy,
    this.previewEnabled = // 默认开启（P4d 全链路已完成）；发布门禁前如需关断：--dart-define=ELSEWHEN_SYNC_PREVIEW=false
    const bool.fromEnvironment(
      'ELSEWHEN_SYNC_PREVIEW',
      defaultValue: true,
    ),
  }) : _keys = keyStore ?? SyncKeyStore(),
       _bridge = bridge ?? FrbSyncJoinApi();

  final SyncKeyStore _keys;
  final SyncJoinApi _bridge;
  final SyncEnrollmentController? enrollment;
  final bool Function()? isSessionBusy;
  final bool previewEnabled;
  api.SyncJoinStateDto? state;
  List<api.SyncJoinEntryDto> entries = [];
  String? error;
  String? notice;
  bool busy = false;
  bool _closed = false;
  bool _disposed = false;
  String? _operation;
  Completer<void>? _actionDone;
  bool get active => _operation != null;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  void _requireOpen() {
    if (_closed || _disposed) throw StateError('sync_join_closed');
  }

  void _accept(api.SyncJoinStateDto next) {
    _requireOpen();
    if (next.operationId != _operation) {
      throw StateError('sync_join_request_stale');
    }
    if (state?.phase != next.phase) {
      debugPrint(
        '[sync-pairing] joiner: phase ${state?.phase ?? 'none'} -> ${next.phase} '
        '(op=${next.operationId})',
      );
    }
    state = next;
    _changed();
  }

  void _own(api.SyncJoinStateDto next) {
    // Remember ownership even if close happened while the initial call awaited.
    _operation = next.operationId;
    _accept(next);
  }

  Future<void> refreshFromEnrollment() async {
    final operation = _operation;
    if (_closed || _disposed || busy || operation == null) return;
    try {
      final next = await _bridge.state(operation);
      if (_closed ||
          _disposed ||
          busy ||
          _operation != operation ||
          next.revision < (state?.revision ?? 0)) {
        return;
      }
      _accept(next);
      notice = appNotice('l6TheApprovedHandoffIsBeingCoordinated7e9e2');
      _changed();
    } catch (_) {
      // Keep the displayed operation, never adopt a newer session on a late read.
    }
  }

  void enrollmentReleased(api.SyncEnrollmentStateDto released) {
    if (released.role != 'joiner' || released.operationId != _operation) return;
    _operation = null;
    error = appNotice('l6TheNativeHandoffRevisionCouldNot775d0');
    _changed();
  }

  void _resumeEnrollment() {
    if (!_closed &&
        !_disposed &&
        active &&
        const [
          'awaiting_membership',
          'proof_sent',
          'enrollment',
          'enrollment_confirmation',
          'expired',
        ].contains(state?.phase)) {
      unawaited(enrollment?.arm(operationId: _operation));
    }
  }

  Future<void> preview(String code) => _action(() async {
    if (active) throw StateError('sync_join_already_open');
    debugPrint('[sync-pairing] joiner: preview invitation (offline decode)');
    _own(await _bridge.preview(code.trim()));
    notice = appNotice('l6PreviewDidNotConnectToThed4c19');
  });

  Future<void> join(
    api.SyncJoinStateDto shown,
    String name,
    String platform, {
    required bool consent,
  }) async {
    if (!consent || name.trim().isEmpty) {
      error = appNotice('l6EnterADeviceNameAndExplicitly802c8');
      _changed();
      return;
    }
    await _action(() async {
      if (shown.operationId != _operation) {
        throw StateError('sync_join_request_stale');
      }
      debugPrint(
        '[sync-pairing] joiner: joining as "$name/$platform" relay=${shown.relayUrl}',
      );
      final request = await _bridge.prepare(
        shown.token,
        name.trim(),
        platform,
        consent,
      );
      _accept(request);
      await _activate(request);
      _requireOpen();
      if (state?.phase == 'ready_to_request') {
        _accept(await _bridge.sendRequest(_operation!));
        debugPrint(
          '[sync-pairing] joiner: request delivered, owner fingerprint=${state?.ownerFingerprint}',
        );
        notice = appNotice('l6JoinRequestDeliveredVerifyThisDevice8e98f');
      }
    });
  }

  Future<void> refreshEntries() => _action(() async {
    final pending = await _bridge.pending();
    _requireOpen();
    entries = pending;
    notice = pending.isEmpty
        ? appNotice('l6NoUnfinishedEnrollmentStagingEntriesaa853')
        : appNotice('l6RecoveryReadsTheOriginalInstallationS770c9');
  });

  /// 删除一个可恢复的加入暂存项：Rust 端只受理 staging（ready 数据集
  /// fail-closed），本机密钥随后从安全存储抹除。成功后刷新列表。
  Future<void> discardStaging(api.SyncJoinEntryDto entry) => _action(() async {
    await _bridge.discardStaging(entry.entryId);
    _requireOpen();
    try {
      await _keys.deleteInstallation(entry.installationId);
    } catch (_) {
      // 密钥抹除失败不回滚 staging 删除：数据目录已清，残键无害孤儿。
      notice = appNotice('l6StagingEntryDeletedButKeyCleanup3ce20');
      final pending = await _bridge.pending();
      _requireOpen();
      entries = pending;
      return;
    }
    final pending = await _bridge.pending();
    _requireOpen();
    entries = pending;
    notice = appNotice('l6StagingEntryArg0Deleted0f067', [
      (entry.name).toString(),
    ]);
  });

  Future<void> resume(api.SyncJoinEntryDto entry) => _action(() async {
    if (active) throw StateError('sync_join_already_open');
    final request = await _bridge.resume(entry.entryId);
    _own(request);
    if (request.entryId != entry.entryId ||
        request.installationId != entry.installationId ||
        request.datasetId != entry.datasetId ||
        request.creating) {
      await _releaseOwned();
      throw StateError('sync_join_staging_changed');
    }
    await _activate(request);
    _requireOpen();
    if (state?.phase == 'awaiting_membership') {
      _accept(await _bridge.sendProof(_operation!));
      notice = appNotice('l6OriginalInstallationProofResentWaitingForee8e7');
    }
  });

  Future<void> _activate(api.SyncJoinStateDto request) async {
    SyncMasterKey? key;
    try {
      final installation = request.installationId;
      if (installation == null) throw StateError('sync_join_staging_changed');
      final int revision;
      if (request.creating) {
        key = await _keys.createForNewInstallation(installation);
        _requireOpen();
        await _keys.saveRevision(installation, 1);
        revision = await _keys.readRevision(installation);
      } else {
        key = await _keys.load(installation);
        revision = await _keys.readRevision(installation);
      }
      _requireOpen();
      final bytes = key.copyBytesForRust();
      late final api.SyncJoinStateDto commit;
      try {
        commit = await _bridge.install(request.token, bytes, revision);
      } finally {
        bytes.fillRange(0, bytes.length, 0);
      }
      _validateCommit(request, commit);
      _accept(commit);
      await _confirm(commit);
    } catch (_) {
      try {
        await _releaseOwned();
      } catch (_) {
        /* Never send after uncertain native persistence. */
      }
      rethrow;
    } finally {
      key?.dispose();
    }
  }

  void _validateCommit(api.SyncJoinStateDto prior, api.SyncJoinStateDto next) {
    if (prior.operationId != next.operationId ||
        prior.entryId != next.entryId ||
        prior.installationId != next.installationId ||
        prior.datasetId != next.datasetId) {
      throw StateError('sync_join_staging_changed');
    }
  }

  Future<void> _confirm(api.SyncJoinStateDto commit) async {
    try {
      _requireOpen();
      final installation = commit.installationId;
      if (installation == null) throw StateError('sync_join_staging_changed');
      await _keys.saveRevision(installation, commit.revision);
      final stored = await _keys.readRevision(installation);
      _requireOpen();
      final confirmed = await _bridge.confirm(commit.token, stored);
      _validateCommit(commit, confirmed);
      _accept(confirmed);
    } catch (_) {
      try {
        await _releaseOwned();
      } catch (_) {
        /* Runtime remains unconfirmed. */
      }
      rethrow;
    }
  }

  Future<void> sendRequest() => _action(() async {
    final operation = _operation;
    if (operation == null) throw StateError('sync_join_session_missing');
    _accept(await _bridge.sendRequest(operation));
    notice = appNotice('l6OriginalRequestResentWaitingForExplicitb70c0');
  });

  Future<void> inspectApproval() => _action(() async {
    final operation = _operation;
    if (operation == null) throw StateError('sync_join_session_missing');
    final next = await _bridge.inspect(operation);
    if (state != null) _validateCommit(state!, next);
    _accept(next);
    if (next.phase == 'installation_confirmation') {
      await _confirm(next);
      _requireOpen();
      _accept(await _bridge.sendProof(operation));
      notice = appNotice(
        'l6InstallationProofDeliveredWaitingForAuthorization69cc4',
      );
    } else {
      notice = appNotice('l6NoUsableApprovalYetVerifyAnd0b6c4');
    }
  });

  Future<void> sendProof() => _action(() async {
    final operation = _operation;
    if (operation == null) throw StateError('sync_join_session_missing');
    _accept(await _bridge.sendProof(operation));
    notice = appNotice('l6TheSameInstallationProofWasResent1abc4');
  });

  Future<void> _releaseOwned() async {
    final operation = _operation;
    if (operation == null) return;
    await _bridge.release(operation);
    if (_operation == operation) _operation = null;
  }

  Future<void> cancel() => _action(() async {
    await _releaseOwned();
    state = null;
    notice = appNotice('l6EnrollmentStoppedStagedDataAndKeys9b648');
  });

  Future<void> close() async {
    _closed = true;
    await enrollment?.pause();
    await _actionDone?.future;
    try {
      await _releaseOwned();
    } catch (_) {
      /* Process exit also releases the lease. */
    }
  }

  String _message(Object failure) {
    if (failure is SyncKeyStoreException) {
      return switch (failure.code) {
        SyncKeyStoreError.missing => appNotice(
          'l6DeviceKeysOrProtectedRevisionsAre5de96',
        ),
        SyncKeyStoreError.revisionRollback => appNotice(
          'l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35',
        ),
        SyncKeyStoreError.corrupt => appNotice(
          'l6DeviceKeyRecordsAreDamagedEnrollment76c86',
        ),
        _ => appNotice('l6SecureKeyStorageCouldNotSave6f761'),
      };
    }
    final text = failure.toString();
    if (text.contains('runtime_in_use') ||
        text.contains('already_open') ||
        text.contains('processing_must_finish')) {
      return appNotice('l6ThisWorkspaceHasAnActiveSync69142');
    }
    if (text.contains('expired') ||
        text.contains('requires_new_invitation') ||
        text.contains('not_recoverable')) {
      return appNotice('l6TheInvitationExpiredOrHasNo6c654');
    }
    if (text.contains('delivery_unconfirmed') ||
        text.contains('relay_unavailable')) {
      return appNotice('l6TheRelayStepIsUnconfirmedCheck92aea');
    }
    return appNotice('l6EnrollmentIsIncompleteAndHasStopped1c448');
  }

  Future<void> _action(Future<void> Function() action) async {
    if (busy || _closed || _disposed || isSessionBusy?.call() == true) return;
    if (!previewEnabled) {
      error = appNotice('l6DeviceSyncIsDisabledInThise1b42');
      _changed();
      return;
    }
    busy = true;
    error = null;
    final done = Completer<void>();
    _actionDone = done;
    _changed();
    // 配对请求/核对审批/投递证明都是中继上的网络往返；本步没做完就
    // 被系统冻结会退化成「发了但永远没回」。
    SyncKeepAlive.hold('sync-join');
    try {
      final coordinator = enrollment;
      if (coordinator != null && coordinator.enabled) {
        await coordinator.pause();
      }
      _requireOpen();
      await action();
    } catch (failure) {
      error = _message(failure);
      final operation = _operation;
      if (!_closed && !_disposed && operation != null) {
        try {
          _accept(await _bridge.state(operation));
        } catch (_) {
          /* Retain the last view. */
        }
      }
    } finally {
      busy = false;
      done.complete();
      if (identical(_actionDone, done)) _actionDone = null;
      _changed();
      SyncKeepAlive.release('sync-join');
      _resumeEnrollment();
    }
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    unawaited(close());
    super.dispose();
  }
}
