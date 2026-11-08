part of 'sync_setup_controller.dart';

extension OwnerSyncPairing on SyncSetupController {
  Future<void> createInvitation(
    String relayUrl, {
    required bool consent,
  }) async {
    if (!consent) {
      error = appNotice('l6ConfirmThatTheJoiningDeviceMay8297c');
      _changed();
      return;
    }
    await _pairingAction(() async {
      final offer = await _pairing.issue(relayUrl.trim(), consent);
      _requirePairingOpen();
      invitation = offer;
      pairingPreview = null;
      pendingPairingId = null;
      approvalDelivered = false;
      debugPrint(
        '[sync-pairing] owner: invitation created id=${offer.invitationId} '
        'expires=${DateTime.fromMillisecondsSinceEpoch(offer.expiresAtMs).toLocal()}',
      );
      pairingNotice = appNotice('l6TheInvitationIsValidFor245be17');
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
      pairingNotice = preview == null
          ? appNotice('l6NoJoinRequestReceivedYet10542')
          : appNotice('l6VerifyTheDeviceAndFingerprintBefore5e3e6');
      debugPrint(
        '[sync-pairing] owner: inspected id=${offer.invitationId} '
        'request=${preview == null ? 'none' : '${preview.deviceName}/${preview.devicePlatform}'}',
      );
    });
  }

  Future<void> approveInvitation(
    api.SyncInvitationPreviewDto preview, {
    required bool consent,
  }) async {
    if (!consent) {
      error = appNotice('l6ExplicitlyConfirmSharingProviderApiKeysd0cdd');
      _changed();
      return;
    }
    await _pairingAction(() async {
      // 后台网络同步 tick 短暂占用工作区时会以 sync_runtime_in_use 拒绝
      // 任何改动 vault 修订的配对写入（这是防止并发写坏密钥修订的硬约束）。
      // 该错误发生在任何写入之前，安全做法是稍等后重试同一次审批，而不是
      // 把用户刚核对完的设备指纹丢掉。
      final commit = await _retryWhileRuntimeBusy(
        () => _pairing.approve(preview.invitationId, preview.token, consent),
      );
      pendingPairingId = preview.invitationId;
      invitation = null;
      pairingPreview = null;
      debugPrint(
        '[sync-pairing] owner: approved device=${preview.deviceName} '
        'id=${preview.invitationId} (keys persisted locally)',
      );
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
      approvalDelivered = true;
      debugPrint(
        '[sync-pairing] owner: approval delivered id=${preview.invitationId}',
      );
      _requirePairingOpen();
      pairingNotice = appNotice(
        'l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8',
      );
    });
  }

  Future<void> resendPairingApproval() async {
    final id = pendingPairingId;
    if (id == null) return;
    await _pairingAction(() async {
      await _pairing.sendApproval(id);
      approvalDelivered = true;
      _requirePairingOpen();
      pairingNotice = appNotice(
        'l6ApprovalDeliveredWaitingForInstallationConfirmation9b218',
      );
    });
  }

  Future<void> cancelInvitation() async {
    final offer = invitation;
    if (offer == null) return;
    await _pairingAction(() async {
      try {
        await _pairing.cancel(offer.invitationId);
        pairingNotice = appNotice('l6InvitationCanceled35e1b');
      } finally {
        invitation = null;
        pairingPreview = null;
      }
    });
  }

  /// 自动重投未确认的审批（邀请页 3 秒轮询调用）。成功后更新提示；
  /// 失败保持现状静默，由下一轮继续。
  Future<void> autoRedeliverPendingApproval() async {
    final id = pendingPairingId;
    if (id == null || approvalDelivered) return;
    try {
      await _pairing.sendApproval(id);
      _requirePairingOpen();
      debugPrint('[sync-pairing] owner: auto-redelivery confirmed id=$id');
      if (pendingPairingId == id && !approvalDelivered) {
        approvalDelivered = true;
        pairingNotice = appNotice('l6ApprovalDeliveredWaitingForTheNew361e9');
        _changed();
      }
    } catch (_) {
      // 投递仍未确认：保持原状态，下一轮继续。
    }
  }

  void _requirePairingOpen() {
    if (_disposed || _closed) throw StateError('closed');
  }

  /// `sync_runtime_in_use` now means the native side already waited out its
  /// bounded budget (in-flight background worker) and still could not run.
  /// A single spaced retry covers a stale runtime snapshot only; do not stack
  /// long waits on top of the native wait.
  Future<T> _retryWhileRuntimeBusy<T>(Future<T> Function() action) async {
    const attempts = 2;
    for (var attempt = 1; ; attempt++) {
      try {
        return await action();
      } catch (failure) {
        if (!failure.toString().contains('sync_runtime_in_use') ||
            attempt >= attempts) {
          rethrow;
        }
        _requirePairingOpen();
        await Future<void>.delayed(const Duration(seconds: 1));
        _requirePairingOpen();
      }
    }
  }

  Future<void> _pairingAction(Future<void> Function() action) async {
    if (busy || _disposed || _closed) return;
    if (!previewEnabled ||
        state?.state != 'local_ready' ||
        state?.isOwner != true) {
      error = appNotice('l6DeviceSyncIsDisabledInThise1b42');
      _changed();
      return;
    }
    busy = true;
    error = null;
    final done = Completer<void>();
    _activationDone = done;
    _changed();
    // 发起/批准/重投审批都是中继往返，批准还要写密钥库：中途被冻结会让
    // 新设备那边一直停在「等待主设备批准」。
    SyncKeepAlive.hold('sync-pairing-owner');
    try {
      await enrollment.pause();
      _requirePairingOpen();
      await action();
    } catch (failure) {
      debugPrint('[sync-pairing] owner: action failed: $failure');
      final text = failure.toString();
      if (failure is SyncKeyStoreException) {
        error = _message(failure);
      } else if (text.contains('expired') || text.contains('approval_stale')) {
        error = appNotice('l6TheInvitationOrReviewVersionExpireda53ec');
      } else if (text.contains('revocation_unconfirmed')) {
        error = appNotice('l6TheInvitationWasCanceledLocallyRelay360d8');
      } else if (text.contains('delivery_unconfirmed')) {
        error = appNotice('l6ApprovalIsSavedButHasNot99508');
        debugPrint(
          '[sync-pairing] owner: approval delivery unconfirmed (auto-retry armed): $failure',
        );
      } else if (text.contains('sync_runtime_in_use')) {
        error = appNotice('l6BackgroundSyncIsUsingTheRuntime0a21c');
      } else {
        error = appNotice('l6PairingIsIncompleteCheckDeviceUnlock85108');
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
      SyncKeepAlive.release('sync-pairing-owner');
      _resumeEnrollment();
    }
  }
}
