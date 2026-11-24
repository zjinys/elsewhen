import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:flutter/services.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_setup_controller.dart';
import 'sync_pairing_steps.dart';
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
  Timer? _clock;

  @override
  void initState() {
    super.initState();
    _clock = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted && widget.controller.invitation != null) setState(() {});
    });
    _relay.text = widget.controller.state?.relayUrl ?? '';
    try {
      final prefs = const DesktopPrefsStore();
      final last = prefs.loadLastOwnerRelayUrl();
      if (_relay.text.isEmpty && last != null && last.isNotEmpty) {
        _relay.text = last;
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _clock?.cancel();
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
              child: Text(context.l10n.pairingAllow),
            ),
          ],
        ),
      ),
    );
    if (approved == true && mounted) {
      await widget.controller.approveInvitation(preview, consent: true);
    }
  }

  Future<void> _create() async {
    try {
      const DesktopPrefsStore().saveLastOwnerRelayUrl(_relay.text.trim());
    } catch (_) {}
    await widget.controller.createInvitation(_relay.text, consent: true);
  }

  @override
  Widget build(BuildContext context) {
    final c = widget.controller;
    return ListenableBuilder(
      listenable: Listenable.merge([c, c.enrollment]),
      builder: (context, _) {
        final offer = c.invitation;
        final preview = c.pairingPreview;
        final id = c.currentPairingId;
        final states = c.enrollment.states
            .where((s) => s.role == 'owner' && s.invitationId == id)
            .toList();
        final complete = states.any((s) => s.phase == 'complete');
        final approved = c.pendingPairingId != null;
        final expired =
            !approved &&
            offer != null &&
            DateTime.now().millisecondsSinceEpoch >= offer.expiresAtMs;
        final device = preview ?? c.approvedDevice;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (id == null) ...[
              Text(context.l10n.pairingConnectHint),
              const SizedBox(height: 16),
              if (_relay.text.isEmpty) Text(context.l10n.pairingNoRelay),
              ExpansionTile(
                tilePadding: EdgeInsets.zero,
                initiallyExpanded: _relay.text.isEmpty,
                title: Text(context.l10n.pairingConnectionSettings),
                children: [
                  TextField(
                    controller: _relay,
                    enabled: !c.busy,
                    keyboardType: TextInputType.url,
                    autocorrect: false,
                    decoration: AppTheme.fieldDecoration(
                      labelText: context.l10n.l6YourRelayUrl9ff3a,
                      hintText: 'https://…',
                    ),
                  ),
                  const SizedBox(height: 12),
                ],
              ),
              CheckboxListTile(
                value: _consent,
                enabled: !c.busy,
                contentPadding: EdgeInsets.zero,
                title: Text(
                  context.l10n.l6IUnderstandThatCreatingAnInvitation5f3db,
                ),
                onChanged: (value) => setState(() => _consent = value ?? false),
              ),
              FilledButton.icon(
                onPressed: c.busy || !_consent ? null : _create,
                icon: const Icon(Icons.add_link),
                label: Text(context.l10n.l6CreateATemporaryInvitationa2809),
              ),
            ] else ...[
              SyncPairingSteps(
                steps: [
                  context.l10n.pairingIdentify,
                  context.l10n.pairingConfirm,
                  context.l10n.pairingSyncData,
                ],
                current: complete
                    ? 3
                    : approved
                    ? 2
                    : preview != null
                    ? 1
                    : 0,
              ),
              const SizedBox(height: 16),
              if (device != null) ...[
                ListTile(
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.devices_outlined),
                  title: Text(device.deviceName),
                  subtitle: Text(device.devicePlatform),
                ),
                const SizedBox(height: 8),
              ],
              Semantics(
                liveRegion: true,
                child: Text(
                  complete
                      ? context.l10n.pairingOwnerDone
                      : approved
                      ? context.l10n.pairingTransferring
                      : expired
                      ? context.l10n.pairingExpired
                      : preview != null
                      ? context.l10n.l6VerifyTheDeviceAndFingerprintBefore5e3e6
                      : context.l10n.pairingWaitingScan,
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              const SizedBox(height: 12),
              if (approved && !complete) ...[
                for (final s in states.where(
                  (s) => const [
                    'preparing_snapshot',
                    'awaiting_snapshot_selection',
                    'sending_snapshot',
                  ].contains(s.phase),
                ))
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: SyncSnapshotProgress(
                      received: s.receivedChunks,
                      total: s.totalChunks,
                      label: s.totalChunks > 0
                          ? context.l10n.l6SendingTheInitialSnapshotfa3fd
                          : context.l10n.pairingDataPrepared,
                    ),
                  ),
                if (states.isEmpty) Text(context.l10n.pairingDataPrepared),
                if (c.enrollment.errorFor(invitationId: id) != null)
                  AppErrorMessage(
                    error: c.enrollment.errorFor(invitationId: id)!,
                    action: context.l10n.load,
                  ),
                if (states.any((s) => s.storageBlocked))
                  Text(
                    context
                        .l10n
                        .l6RelayMailboxFullDeliveryResumesAutomatically4a933,
                    style: TextStyle(color: AppTheme.warning),
                  ),
              ],
              if (!approved &&
                  !expired &&
                  preview == null &&
                  offer != null) ...[
                Center(
                  child: Container(
                    color: Colors.white,
                    padding: const EdgeInsets.all(12),
                    child: QrImageView(
                      data: offer.code,
                      version: QrVersions.auto,
                      size: 196,
                      backgroundColor: Colors.white,
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Text(
                  context.l10n.pairingRemaining(
                    (((offer.expiresAtMs -
                                    DateTime.now().millisecondsSinceEpoch) /
                                60000)
                            .ceil()
                            .clamp(1, 1440))
                        .toString(),
                  ),
                ),
                const SizedBox(height: 8),
                OutlinedButton.icon(
                  onPressed: c.busy
                      ? null
                      : () =>
                            Clipboard.setData(ClipboardData(text: offer.code)),
                  icon: const Icon(Icons.copy_outlined),
                  label: Text(context.l10n.l6CopyInvitationf7293),
                ),
              ],
              if (!approved && preview != null && !expired)
                FilledButton(
                  onPressed: c.busy ? null : () => _review(preview),
                  child: Text(context.l10n.l6VerifyAndApprove5c161),
                ),
              if (complete || expired)
                FilledButton.icon(
                  onPressed: c.busy ? null : _create,
                  icon: const Icon(Icons.add_link),
                  label: Text(
                    complete
                        ? context.l10n.pairingAnother
                        : context.l10n.pairingRegenerate,
                  ),
                ),
              if (!approved)
                TextButton(
                  onPressed: c.busy ? null : c.cancelInvitation,
                  child: Text(context.l10n.l6CancelInvitation17e4c),
                ),
              if (approved && c.approvalReady && c.error != null)
                OutlinedButton(
                  onPressed: c.busy ? null : c.resendPairingApproval,
                  child: Text(context.l10n.pairingRetry),
                ),
              if (approved && c.enrollment.paused && states.isNotEmpty)
                OutlinedButton(
                  onPressed: c.busy ? null : () => c.enrollment.arm(),
                  child: Text(context.l10n.pairingRetry),
                ),
              if (device != null)
                ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  title: Text(context.l10n.pairingDetails),
                  children: [SelectableText(device.fingerprint)],
                ),
            ],
          ],
        );
      },
    );
  }
}
