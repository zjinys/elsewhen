import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:flutter/services.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_setup_controller.dart';
import 'sync_pairing_steps.dart';
import 'sync_tick_status.dart';
import '../utils/desktop_prefs.dart';
import '../theme/app_theme.dart';

class OwnerSyncPairingSettings extends StatefulWidget {
  const OwnerSyncPairingSettings({
    super.key,
    required this.controller,
    this.deliveryStatuses,
  });
  final SyncSetupController controller;

  /// 每对端的投递状态（R6-2）。为 null 时不显示 —— 面板不自己造进度，
  /// 拿不到就少一行，而不是编一个「同步中」。
  final ValueListenable<List<api.SyncDeliveryStatusDto>>? deliveryStatuses;

  @override
  State<OwnerSyncPairingSettings> createState() =>
      _OwnerSyncPairingSettingsState();
}

class _OwnerSyncPairingSettingsState extends State<OwnerSyncPairingSettings> {
  final _relay = TextEditingController();
  bool _consent = false;
  Timer? _joinPoll;

  @override
  void initState() {
    super.initState();
    // 新设备扫码后自行发起加入请求；owner 端不必手动点「检查」——
    // 邀请存续期间自动收件（收审批界面上可见即停）。
    _joinPoll = Timer.periodic(const Duration(seconds: 3), (_) {
      if (!mounted) return;
      final c = widget.controller;
      if (c.busy) return;
      // 已批准但投递未确认：自动重投，直到中继确认。
      if (c.pendingPairingId != null && !c.approvalDelivered) {
        c.autoRedeliverPendingApproval();
        return;
      }
      if (c.invitation == null || c.pairingPreview != null) return;
      c.inspectInvitation();
    });
    try {
      final prefs = const DesktopPrefsStore();
      final last = prefs.loadLastOwnerRelayUrl();
      if (last != null && last.isNotEmpty) {
        _relay.text = last;
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _joinPoll?.cancel();
    _relay.dispose();
    super.dispose();
  }

  Future<void> _review(api.SyncInvitationPreviewDto preview) async {
    var consent = false;
    final approved = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, update) => AlertDialog(
          title: Text(context.l10n.l6ConfirmDeviceEnrollment159c2),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  context.l10n.l6DeviceArg0d7706(
                    (preview.deviceName).toString(),
                  ),
                ),
                Text(
                  context.l10n.l6PlatformArg0e4ee7(
                    (preview.devicePlatform).toString(),
                  ),
                ),
                const SizedBox(height: 12),
                Text(context.l10n.l6CompareThePublicKeyFingerprintWitha360c),
                SelectableText(preview.fingerprint),
                const SizedBox(height: 12),
                CheckboxListTile(
                  contentPadding: EdgeInsets.zero,
                  value: consent,
                  title: Text(
                    context.l10n.l6ConfirmThatYouTrustThisDevice20292,
                  ),
                  onChanged: (value) => update(() => consent = value ?? false),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(context.l10n.l6Back11d02),
            ),
            FilledButton(
              onPressed: consent ? () => Navigator.pop(context, true) : null,
              child: Text(context.l10n.l6ApproveThisDeviced11af),
            ),
          ],
        ),
      ),
    );
    if (approved == true && mounted) {
      await widget.controller.approveInvitation(preview, consent: true);
    }
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    return ListenableBuilder(
      listenable: Listenable.merge([controller, controller.enrollment]),
      builder: (context, _) {
        final offer = controller.invitation;
        final preview = controller.pairingPreview;
        final sending = controller.enrollment.states
            .where(
              (s) =>
                  s.role == 'owner' &&
                  const [
                    'preparing_snapshot',
                    'awaiting_snapshot_selection',
                    'sending_snapshot',
                  ].contains(s.phase) &&
                  s.totalChunks > 0,
            )
            .toList();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Divider(height: 32),
            Text(
              context.l10n.l6InviteADevice9f3fa,
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 8),
            Text(context.l10n.l6NewDevicesCanRequestToJoinbe25a),
            if (offer == null) ...[
              const SizedBox(height: 12),
              TextField(
                controller: _relay,
                enabled: !controller.busy,
                keyboardType: TextInputType.url,
                autocorrect: false,
                style: AppTheme.fieldTextStyle,
                cursorColor: AppTheme.accentPrimary,
                decoration: AppTheme.fieldDecoration(
                  labelText: context.l10n.l6YourRelayUrl9ff3a,
                  hintText: 'https://…',
                ),
              ),
              CheckboxListTile(
                value: _consent,
                enabled: !controller.busy,
                contentPadding: EdgeInsets.zero,
                title: Text(
                  context.l10n.l6IUnderstandThatCreatingAnInvitation5f3db,
                ),
                onChanged: (value) => setState(() => _consent = value ?? false),
              ),
              FilledButton.icon(
                onPressed: controller.busy || !_consent
                    ? null
                    : () async {
                        try {
                          const DesktopPrefsStore().saveLastOwnerRelayUrl(
                            _relay.text.trim(),
                          );
                        } catch (_) {}
                        await controller.createInvitation(
                          _relay.text,
                          consent: true,
                        );
                      },
                icon: const Icon(Icons.add_link),
                label: Text(context.l10n.l6CreateATemporaryInvitationa2809),
              ),
            ] else ...[
              const SizedBox(height: 12),
              SyncPairingSteps(
                steps: [
                  context.l10n.l6InvitationCreatedShowTheQrCode15986,
                  context.l10n.l6ANewDeviceHasRequestedTo3129b,
                  context.l10n.l6ApprovedDeliveringApprovalc4bcf,
                  context.l10n.l6ApprovalDeliveredWaitingForTheNewaeaa7,
                  context.l10n.l6NewDeviceJoinsCheckTheDevice29220,
                ],
                current: ownerPairingStage(
                  invitationId: offer.invitationId,
                  hasPreview: preview != null,
                  pendingPairingId: controller.pendingPairingId,
                  approvalDelivered: controller.approvalDelivered,
                  // 终态：enrollment 视图里这台新设备的交接已经 `complete`（对端
                  // 已回执/已被确认）。没有它，第五步会永远转圈。
                  deviceJoined: controller.enrollment.states.any(
                    (s) => s.role == 'owner' && s.phase == 'complete',
                  ),
                ),
              ),
              const SizedBox(height: 8),
              for (final snapshotState in sending) ...[
                SyncSnapshotProgress(
                  received: snapshotState.receivedChunks,
                  total: snapshotState.totalChunks,
                  label: switch (snapshotState.phase) {
                    'preparing_snapshot' =>
                      context.l10n.l6PreparingTheInitialSnapshotb1613,
                    'awaiting_snapshot_selection' =>
                      context.l10n.l6WaitingForInitialSnapshotSelectione32c8,
                    _ => context.l10n.l6SendingTheInitialSnapshotfa3fd,
                  },
                ),
                const SizedBox(height: 8),
              ],
              // 交接轮询失联/失败时必须有可见原因：否则主设备也会停在同一帧转圈，
              // 用户无法区分「还在搬」和「已经卡死」。
              if (controller.enrollment.error != null) ...[
                AppErrorMessage(
                  error: controller.enrollment.error!,
                  action: context.l10n.load,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
                const SizedBox(height: 8),
              ],
              // relay 信箱打满（507）时，发送被主动停拍退避：给一个明确提示，
              // 避免用户把「在等空间」当成「卡死」。
              if (controller.enrollment.states.any(
                (s) => s.storageBlocked,
              )) ...[
                Text(
                  context
                      .l10n
                      .l6RelayMailboxFullDeliveryResumesAutomatically4a933,
                  style: TextStyle(fontSize: 12, color: AppTheme.warning),
                ),
                const SizedBox(height: 8),
              ],
              if (widget.deliveryStatuses != null) ...[
                SyncDeliverStatusLine(statuses: widget.deliveryStatuses!),
                const SizedBox(height: 8),
              ],
              Text(context.l10n.l6ShareInvitationsOnlyWithTrustedDevices47059),
              Text(
                context.l10n.l6ExpiresArg0a7daf(
                  (DateTime.fromMillisecondsSinceEpoch(
                    offer.expiresAtMs,
                  ).toLocal()).toString(),
                ),
              ),
              const SizedBox(height: 8),
              Center(
                child: Container(
                  color: Colors.white,
                  padding: const EdgeInsets.all(8),
                  child: QrImageView(
                    data: offer.code,
                    version: QrVersions.auto,
                    size: 220,
                    backgroundColor: Colors.white,
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  OutlinedButton(
                    onPressed: controller.busy
                        ? null
                        : () => Clipboard.setData(
                            ClipboardData(text: offer.code),
                          ),
                    child: Text(context.l10n.l6CopyInvitationf7293),
                  ),
                  TextButton(
                    onPressed: controller.busy
                        ? null
                        : controller.cancelInvitation,
                    child: Text(context.l10n.l6CancelInvitation17e4c),
                  ),
                ],
              ),
            ],
            if (preview != null) ...[
              const SizedBox(height: 12),
              Text(
                context.l10n.l6AwaitingReviewArg0Arg17fa78(
                  (preview.deviceName).toString(),
                  (preview.devicePlatform).toString(),
                ),
              ),
              FilledButton(
                onPressed: controller.busy ? null : () => _review(preview),
                child: Text(context.l10n.l6VerifyAndApprove5c161),
              ),
            ],
            if (controller.pendingPairingId != null) ...[
              const SizedBox(height: 8),
              TextButton(
                onPressed: controller.busy
                    ? null
                    : controller.resendPairingApproval,
                child: Text(context.l10n.l6ResendSavedApproval48def),
              ),
            ],
            if (controller.pairingNotice != null) ...[
              const SizedBox(height: 8),
              Text(context.notice(controller.pairingNotice!)),
            ],
          ],
        );
      },
    );
  }
}
