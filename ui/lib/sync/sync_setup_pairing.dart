part of 'sync_setup_controller.dart';

extension OwnerSyncPairing on SyncSetupController {
  Future<void> createInvitation(
    String relayUrl, {
    required bool consent,
  }) async {
    if (!consent) {
      error = '请确认加入设备将获得全部 Provider 配置和 API Key。';
      _changed();
      return;
    }
    await _pairingAction(() async {
      final offer = await _pairing.issue(relayUrl.trim(), consent);
      _requirePairingOpen();
      invitation = offer;
      pairingPreview = null;
      pendingPairingId = null;
      pairingNotice = '邀请码短期有效，仅通过可信渠道交给要加入的设备。';
    });
  }

  Future<void> inspectInvitation() async {
    final offer = invitation;
    if (offer == null) return;
    await _pairingAction(() async {
      final preview = await _pairing.inspect(offer.invitationId);
      _requirePairingOpen();
      if (invitation?.invitationId != offer.invitationId) return;
      pairingPreview = preview;
      pairingNotice = preview == null ? '尚未收到加入请求。' : '核对设备及指纹后再明确批准。';
    });
  }

  Future<void> approveInvitation(
    api.SyncInvitationPreviewDto preview, {
    required bool consent,
  }) async {
    if (!consent) {
      error = '请明确确认向这台设备共享 Provider API Key。';
      _changed();
      return;
    }
    await _pairingAction(() async {
      final commit = await _pairing.approve(
        preview.invitationId,
        preview.token,
        consent,
      );
      pendingPairingId = preview.invitationId;
      invitation = null;
      pairingPreview = null;
      try {
        _requirePairingOpen();
        if (commit.installationId != state?.installationId) {
          throw StateError('sync_pairing_installation_changed');
        }
        await _keys.saveRevision(commit.installationId, commit.revision);
        final revision = await _keys.readRevision(commit.installationId);
        _requirePairingOpen();
        state = await _bridge.confirm(commit.token, revision);
        _requirePairingOpen();
      } catch (_) {
        final operation = _ownedOperation;
        if (operation != null) {
          try {
            await _bridge.release(operation);
          } catch (_) {
            // Never send if native persistence/confirmation is uncertain.
          }
          if (_ownedOperation == operation) _ownedOperation = null;
        }
        rethrow;
      }
      onReady?.call();
      await _pairing.sendApproval(preview.invitationId);
      _requirePairingOpen();
      pairingNotice = '审批已投递，等待新设备安装确认；这不表示设备已加入或同步完成。';
    });
  }

  Future<void> resendPairingApproval() async {
    final id = pendingPairingId;
    if (id == null) return;
    await _pairingAction(() async {
      await _pairing.sendApproval(id);
      _requirePairingOpen();
      pairingNotice = '审批已重新投递，仍需新设备完成安装确认。';
    });
  }

  Future<void> cancelInvitation() async {
    final offer = invitation;
    if (offer == null) return;
    await _pairingAction(() async {
      try {
        await _pairing.cancel(offer.invitationId);
        pairingNotice = '本次邀请已取消。';
      } finally {
        invitation = null;
        pairingPreview = null;
      }
    });
  }

  void _requirePairingOpen() {
    if (_disposed || _closed) throw StateError('closed');
  }

  Future<void> _pairingAction(Future<void> Function() action) async {
    if (busy || _disposed || _closed) return;
    if (!previewEnabled ||
        state?.state != 'local_ready' ||
        state?.isOwner != true) {
      error = '设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。';
      _changed();
      return;
    }
    busy = true;
    error = null;
    final done = Completer<void>();
    _activationDone = done;
    _changed();
    try {
      await enrollment.pause();
      _requirePairingOpen();
      await action();
    } catch (failure) {
      final text = failure.toString();
      if (failure is SyncKeyStoreException) {
        error = _message(failure);
      } else if (text.contains('expired') || text.contains('approval_stale')) {
        error = '邀请或审阅版本已过期，请重新发起邀请并核对设备。';
      } else if (text.contains('revocation_unconfirmed')) {
        error = '本机已取消邀请，但中继撤销尚未确认；临时发送权限会随短期邀请到期。';
      } else if (text.contains('delivery_unconfirmed')) {
        error = '审批已安全保存，但中继投递未确认。可在邀请到期前重发同一审批。';
      } else {
        error = '配对步骤未完成，请检查本机解锁状态和中继地址；没有自动授权新设备。';
      }
      if (!_disposed && !_closed) {
        try {
          state = await _bridge.state();
        } catch (_) {
          // Retain the last read-only view, never copy an error payload into UI.
        }
      }
    } finally {
      busy = false;
      done.complete();
      if (identical(_activationDone, done)) _activationDone = null;
      _changed();
      _resumeEnrollment();
    }
  }
}
