import 'dart:async';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;
import 'sync_join_api.dart';
import 'sync_key_store.dart';
import 'sync_enrollment_controller.dart';

/// A join owns only its staging operation, never the active dataset or its worker.
class SyncJoinController extends ChangeNotifier {
  SyncJoinController({
    SyncKeyStore? keyStore,
    SyncJoinApi? bridge,
    this.enrollment,
    this.isSessionBusy,
    this.previewEnabled = const bool.fromEnvironment('ELSEWHEN_SYNC_PREVIEW'),
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
      notice = '已批准的交接正在自动协调；初始快照、回执和当前数据集切换是不同阶段。';
      _changed();
    } catch (_) {
      // Keep the displayed operation, never adopt a newer session on a late read.
    }
  }

  void enrollmentReleased(api.SyncEnrollmentStateDto released) {
    if (released.role != 'joiner' || released.operationId != _operation) return;
    _operation = null;
    error = '交接的原生修订未能确认，已停止本机会话。请解锁恢复原暂存项，不要重新加入。';
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
    _own(await _bridge.preview(code.trim()));
    notice = '预览未连接中继。请核对邀请来源、地址和设备指纹。';
  });

  Future<void> join(
    api.SyncJoinStateDto shown,
    String name,
    String platform, {
    required bool consent,
  }) async {
    if (!consent || name.trim().isEmpty) {
      error = '填写设备名称，并明确同意连接显示的中继、接收 Provider 配置和 API Key。';
      _changed();
      return;
    }
    await _action(() async {
      if (shown.operationId != _operation) {
        throw StateError('sync_join_request_stale');
      }
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
        notice = '加入请求已投递。请在旧设备核对本机指纹并明确批准。';
      }
    });
  }

  Future<void> refreshEntries() => _action(() async {
    final pending = await _bridge.pending();
    _requireOpen();
    entries = pending;
    notice = pending.isEmpty ? '没有未完成的加入暂存项。' : '恢复只读取原安装的密钥，不会创建替代密钥。';
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
      notice = '已重发原安装证明，仍等待成员授权和初始同步；没有重新批准或重新生成密钥。';
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
    notice = '已按原请求重发，等待旧设备明确批准。';
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
      notice = '安装证明已投递；仍等待成员授权和初始同步，当前数据集没有切换。';
    } else {
      notice = '尚未收到可用审批，请在旧设备核对并批准。';
    }
  });

  Future<void> sendProof() => _action(() async {
    final operation = _operation;
    if (operation == null) throw StateError('sync_join_session_missing');
    _accept(await _bridge.sendProof(operation));
    notice = '已重发同一安装证明；投递不表示设备已加入或同步完成。';
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
    notice = '已停止本机加入流程，暂存库和密钥保留。临时投递权限会到期；本操作不撤销已发出的请求或可能已获批准的成员资格。';
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
        SyncKeyStoreError.missing => '缺少本机密钥或受保护修订，不能恢复；不会生成替代密钥。',
        SyncKeyStoreError.revisionRollback => '受保护修订发生回退，已停止加入；原库和暂存数据保留。',
        SyncKeyStoreError.corrupt => '本机密钥记录损坏，已停止加入；不会覆盖已有密钥。',
        _ => '系统密钥库未能安全保存或回读，已停止加入；解锁系统密钥库后恢复暂存项。',
      };
    }
    final text = failure.toString();
    if (text.contains('runtime_in_use') ||
        text.contains('already_open') ||
        text.contains('processing_must_finish')) {
      return '工作区已有同步会话或后台任务，请先结束任务或锁定当前同步，再发起加入。';
    }
    if (text.contains('expired') ||
        text.contains('requires_new_invitation') ||
        text.contains('not_recoverable')) {
      return '本次邀请已过期或没有可恢复的已审批检查点。请停止本次流程并获取新邀请；已有数据保留。';
    }
    if (text.contains('delivery_unconfirmed') ||
        text.contains('relay_unavailable')) {
      return '中继步骤未确认。可检查审批或重发原请求/已保存的证明，不要重新生成密钥。';
    }
    return '加入步骤未完成，已停止继续推进。请检查原邀请、解锁状态和暂存项；当前数据集未切换。';
  }

  Future<void> _action(Future<void> Function() action) async {
    if (busy || _closed || _disposed || isSessionBusy?.call() == true) return;
    if (!previewEnabled) {
      error = '加入功能仅在同步开发预览中开放。';
      _changed();
      return;
    }
    busy = true;
    error = null;
    final done = Completer<void>();
    _actionDone = done;
    _changed();
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
