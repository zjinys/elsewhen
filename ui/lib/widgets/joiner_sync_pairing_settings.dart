import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_dataset_switch_api.dart';
import '../sync/sync_join_controller.dart';
import '../utils/device_name.dart';
import '../theme/app_theme.dart';
import 'joiner_qr_scan_page.dart';
import 'dataset_switch_card.dart';
import 'sync_pairing_steps.dart';
import 'sync_tick_status.dart';

class JoinerSyncPairingSettings extends StatefulWidget {
  const JoinerSyncPairingSettings({
    super.key,
    required this.controller,
    this.scannerBuilder,
    this.datasetApi = const FrbSyncDatasetSwitchApi(),
    this.deliveryStatuses,
  });
  final SyncJoinController controller;

  /// 每对端的投递状态（R6-2）。为 null 时不显示（见 owner 面板同名字段）。
  final ValueListenable<List<api.SyncDeliveryStatusDto>>? deliveryStatuses;

  /// 扫码页取景器注入缝（测试替换相机）；见 JoinerQrScanPage。
  final Widget Function(BuildContext context, void Function(String code))?
  scannerBuilder;

  /// 数据集目录切换注入缝（测试替换原生调用）。
  final SyncDatasetSwitchApi datasetApi;

  @override
  State<JoinerSyncPairingSettings> createState() =>
      _JoinerSyncPairingSettingsState();
}

class _JoinerSyncPairingSettingsState extends State<JoinerSyncPairingSettings> {
  final _code = TextEditingController();
  final _name = TextEditingController(text: 'This device');
  bool _consent = false;

  @override
  void initState() {
    super.initState();
    // 缺省设备名用真机器名；用户已改过名将保持原值。
    defaultDeviceName().then((name) {
      if (name != null && mounted && _name.text == 'This device') {
        setState(() => _name.text = name);
      }
    });
  }

  @override
  void dispose() {
    _code.dispose();
    _name.dispose();
    super.dispose();
  }

  Future<void> _scanCode() async {
    final code = await Navigator.of(context).push<String>(
      MaterialPageRoute(
        builder: (_) => JoinerQrScanPage(
          onDetected: (code) => Navigator.of(context).pop(code),
          scannerBuilder: widget.scannerBuilder,
        ),
      ),
    );
    if (!mounted || code == null || code.isEmpty) return;
    setState(() => _consent = false);
    await widget.controller.preview(code);
    if (mounted && widget.controller.active) _code.clear();
  }

  /// 粘贴邀请码：常驻的大文本框占屏且不常用，改成按钮 + 弹窗。
  /// 弹窗里的确认就是原来的「预览邀请（不联网）」：在本地解析邀请码、
  /// 展示主机名/中继/指纹供核对，不联网、不写库；真正的加入还需要在
  /// 卡片里额外勾选同意并点「在独立暂存库发起加入」。
  Future<void> _pasteCode() async {
    final code = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(context.l10n.l6JoinWithAnInvitation78bca),
        content: TextField(
          controller: _code,
          autofocus: true,
          autocorrect: false,
          enableSuggestions: false,
          maxLength: 4200,
          maxLines: 3,
          decoration: InputDecoration(
            labelText: context.l10n.l6InvitationFromATrustedDeviceade9c,
            counterText: '',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(_code.text),
            child: Text(context.l10n.l6PreviewInvitationOfflinee39bc),
          ),
        ],
      ),
    );
    if (!mounted || code == null) return;
    setState(() => _consent = false);
    await widget.controller.preview(code);
    if (mounted && widget.controller.active) _code.clear();
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([widget.controller]),
    builder: (context, _) {
      final controller = widget.controller;
      final state = controller.state;
      final phase = state?.phase;
      final enrollment = controller.enrollment;
      final receiving =
          enrollment?.states
              .where(
                (s) => s.role == 'joiner' && s.phase == 'receiving_snapshot',
              )
              .toList() ??
          const <api.SyncEnrollmentStateDto>[];
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Divider(height: 32),
          // 这一段只是背景说明，日常操作时不需要读；折叠起来，让输入框和
          // 进度直接暴露，需要时再点开。
          _SyncExplanation(
            title: context.l10n.l6JoinAnExistingDatasetb3b50,
            body: context.l10n.l6EnrollmentUsesASeparateStagingDatabase0ddf7,
          ),
          if (phase != null) ...[
            const SizedBox(height: 8),
            SyncPairingSteps(
              steps: [
                context.l10n.l6PreviewAndVerifyInvitation1afde,
                context.l10n.l6SendJoinRequestd5d20,
                context.l10n.l6WaitingForOwnerApproval9e900,
                context.l10n.l6ConfirmInstallation555cc,
                context.l10n.l6WaitingForAuthorizationAndInitialSync6cedf,
              ],
              current: joinerStageFor(phase),
            ),
            const SizedBox(height: 8),
            for (final snapshotState in receiving) ...[
              SyncSnapshotProgress(
                received: snapshotState.receivedChunks,
                total: snapshotState.totalChunks,
                label: context.l10n.l6ReceivingTheInitialSnapshot032cf,
              ),
              const SizedBox(height: 8),
            ],
            if (enrollment?.states.any((s) => s.storageBlocked) == true) ...[
              const SizedBox(height: 8),
              Text(
                context
                    .l10n
                    .l6RelayMailboxFullDeliveryResumesAutomatically4a933,
                style: TextStyle(fontSize: 12, color: AppTheme.warning),
              ),
            ],
            if (widget.deliveryStatuses != null) ...[
              const SizedBox(height: 8),
              SyncDeliverStatusLine(statuses: widget.deliveryStatuses!),
            ],
          ],
          // 加入流程的终态相位叫 `joined`（Rust `Phase::name()`）。这里原本写的是
          // `'complete'` —— 那个字符串从来没有被产出过，于是这张带「进入数据集」
          // 的卡在真机上从未显示，用户除了一个永远转圈的第五步什么都没有。
          if (phase == 'joined') ...[
            DatasetSwitchCard(
              api: widget.datasetApi,
              catalogChanges: controller,
            ),
          ],
          if (!controller.active) ...[
            const SizedBox(height: 12),
            // 邀请码不再常驻一个 3 行文本框：两个入口，码走弹窗。
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FilledButton.icon(
                  onPressed: controller.busy ? null : _pasteCode,
                  icon: const Icon(Icons.key_outlined),
                  label: Text(context.l10n.l6UsingKey99422),
                ),
                OutlinedButton.icon(
                  onPressed: controller.busy ? null : () => _scanCode(),
                  icon: const Icon(Icons.qr_code_scanner),
                  label: Text(context.l10n.l6ScanQrCodeb8a8f),
                ),
              ],
            ),
          ],
          if (state != null && controller.active) ...[
            const SizedBox(height: 12),
            // 邀请细节（数据集 ID、中继地址、两串公钥指纹、到期时间）很长但
            // 不常看：折叠起来，只在需要逐项核对指纹的「预览」阶段默认展开。
            ExpansionTile(
              key: ValueKey('joiner-invite-details-${phase == 'preview'}'),
              tilePadding: EdgeInsets.zero,
              childrenPadding: const EdgeInsets.only(bottom: 8),
              initiallyExpanded: phase == 'preview',
              title: Text(
                context.l10n.l6InvitingDeviceArg0d440c(
                  (state.ownerName.isEmpty
                          ? context.l10n.l6UnlockToVerifycd78c
                          : state.ownerName)
                      .toString(),
                ),
              ),
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        context.l10n.l6DatasetArg085f96(
                          (state.datasetId).toString(),
                        ),
                      ),
                      if (state.relayUrl.isNotEmpty) ...[
                        const SizedBox(height: 8),
                        Text(context.l10n.l6RelayToConnectTo06ef4),
                        SelectableText(state.relayUrl),
                      ],
                      if (state.ownerFingerprint.isNotEmpty) ...[
                        const SizedBox(height: 8),
                        Text(
                          context
                              .l10n
                              .l6InvitingDeviceSPublicKeyFingerprint283a5,
                        ),
                        SelectableText(state.ownerFingerprint),
                      ],
                      if (state.localFingerprint != null) ...[
                        const SizedBox(height: 8),
                        Text(
                          context.l10n.l6ThisDeviceSPublicKeyFingerprintb7e3a,
                        ),
                        SelectableText(state.localFingerprint!),
                      ],
                      if (state.expiresAtMs > 0)
                        Text(
                          context.l10n.l6InvitationExpiresArg0abd8a(
                            (DateTime.fromMillisecondsSinceEpoch(
                              state.expiresAtMs,
                            ).toLocal()).toString(),
                          ),
                        ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(switch (phase) {
              'preview' =>
                context.l10n.l6ReviewTheInvitationThenExplicitlyChoosece487,
              'key_required' || 'key_confirmation' =>
                context.l10n.l6PreparingAndVerifyingProtectedDeviceKeys60dc8,
              'ready_to_request' =>
                context.l10n.l6DeviceKeysAreReadyRequestDeliverye8427,
              'awaiting_approval' =>
                context.l10n.l6WaitingForAnExistingDeviceTo330c1,
              'installation_confirmation' =>
                context.l10n.l6WaitingForSystemConfirmationNoProof188b0,
              'awaiting_membership' =>
                context.l10n.l6InstallationIsSavedProofDeliveryIsf28de,
              'proof_sent' =>
                context
                    .l10n
                    .l6InstallationProofDeliveredWaitingForAuthorization5d65b,
              'enrollment' =>
                context
                    .l10n
                    .l6CoordinatingTheApprovedHandoffAutomaticallyCheck9b863,
              'enrollment_confirmation' =>
                context
                    .l10n
                    .l6ConfirmingHandoffRevisionsSendingResumesAftera5068,
              'expired' => context.l10n.l6TheInvitationExpiredRequestANewf2a42,
              _ => context.l10n.l6JoiningRequiresANewInvitationOr2a99d,
            }),
            if (phase == 'preview') ...[
              const SizedBox(height: 12),
              TextField(
                controller: _name,
                enabled: !controller.busy,
                maxLength: 64,
                style: AppTheme.fieldTextStyle,
                cursorColor: AppTheme.accentPrimary,
                decoration: AppTheme.fieldDecoration(
                  labelText: context.l10n.l6NewDeviceNamed42db,
                ).copyWith(counterText: ''),
              ),
              CheckboxListTile(
                value: _consent,
                enabled: !controller.busy,
                contentPadding: EdgeInsets.zero,
                title: Text(context.l10n.l6IHaveCheckedTheInvitationAnd15ccd),
                onChanged: (value) => setState(() => _consent = value ?? false),
              ),
              FilledButton(
                onPressed: controller.busy || !_consent
                    ? null
                    : () => controller.join(
                        state,
                        _name.text,
                        Platform.operatingSystem,
                        consent: _consent,
                      ),
                child: Text(context.l10n.l6RequestToJoinUsingASeparate835e5),
              ),
            ],
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (phase == 'ready_to_request' || phase == 'awaiting_approval')
                  OutlinedButton(
                    onPressed: controller.busy ? null : controller.sendRequest,
                    child: Text(context.l10n.l6ResendOriginalJoinRequest97725),
                  ),
                if (phase == 'awaiting_approval')
                  FilledButton(
                    onPressed: controller.busy
                        ? null
                        : controller.inspectApproval,
                    child: Text(context.l10n.l6CheckForApproval58c9a),
                  ),
                if (phase == 'awaiting_membership' || phase == 'proof_sent')
                  OutlinedButton(
                    onPressed: controller.busy ? null : controller.sendProof,
                    child: Text(
                      context.l10n.l6ResendSavedInstallationProof3d80a,
                    ),
                  ),
                TextButton(
                  onPressed: controller.busy ? null : controller.cancel,
                  child: Text(context.l10n.l6StopThisDeviceSEnrollmentb3a98),
                ),
              ],
            ),
            if (state.approvalAckPending && phase == 'proof_sent')
              Text(context.l10n.l6ApprovalCleanupIsNotConfirmedResending9fd47),
          ],
          if (controller.busy) ...[
            const SizedBox(height: 12),
            const LinearProgressIndicator(),
          ],
          if (controller.error != null) ...[
            const SizedBox(height: 8),
            Semantics(
              liveRegion: true,
              child: AppErrorMessage(
                error: controller.error!,
                action: context.l10n.load,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          ],
          if (controller.notice != null) ...[
            const SizedBox(height: 8),
            Text(context.notice(controller.notice!)),
          ],
          if (!controller.active) ...[
            const SizedBox(height: 12),
            TextButton(
              onPressed: controller.busy ? null : controller.refreshEntries,
              child: Text(
                context.l10n.l6ViewRecoverableEnrollmentStagingEntriesf095c,
              ),
            ),
            for (final entry in controller.entries)
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(entry.name),
                subtitle: Text(
                  context.l10n.l6DatasetArg0OnlyApprovedCheckpointsCand6435(
                    (entry.datasetId).toString(),
                  ),
                ),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TextButton(
                      onPressed: controller.busy
                          ? null
                          : () => controller.resume(entry),
                      child: Text(context.l10n.l6UnlockAndRecovere9f7c),
                    ),
                    TextButton(
                      onPressed: controller.busy
                          ? null
                          : () => _discardStaging(context, controller, entry),
                      child: Text(context.l10n.delete),
                    ),
                  ],
                ),
              ),
          ],
        ],
      );
    },
  );
}

/// 折叠式说明块：标题常显，正文默认收起（用户反馈这些说明文案
/// 平时不需要读，占位又长）。
class _SyncExplanation extends StatelessWidget {
  const _SyncExplanation({required this.title, required this.body});

  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ExpansionTile(
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: 8),
      title: Text(title, style: theme.textTheme.titleSmall),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: Text(body, style: theme.textTheme.bodySmall),
        ),
      ],
    );
  }
}

/// 删除前确认：不可恢复（暂存数据 + 本机密钥一并抹除）。
Future<void> _discardStaging(
  BuildContext context,
  SyncJoinController controller,
  api.SyncJoinEntryDto entry,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: Text(context.l10n.l6DeleteThisStagingEntrya06bf),
      content: Text(
        context.l10n.l6StagedDataAndDeviceKeysFor0f456((entry.name).toString()),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(false),
          child: Text(context.l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(true),
          child: Text(context.l10n.delete),
        ),
      ],
    ),
  );
  if (confirmed == true && context.mounted) {
    await controller.discardStaging(entry);
  }
}
