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
  final _name = TextEditingController(text: '这台设备');
  bool _consent = false;

  @override
  void initState() {
    super.initState();
    // 缺省设备名用真机器名；用户已改过名将保持原值。
    defaultDeviceName().then((name) {
      if (name != null && mounted && _name.text == '这台设备') {
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
        title: const Text('通过邀请码加入'),
        content: TextField(
          controller: _code,
          autofocus: true,
          autocorrect: false,
          enableSuggestions: false,
          maxLength: 4200,
          maxLines: 3,
          decoration: const InputDecoration(
            labelText: '来自可信设备的邀请码',
            counterText: '',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(_code.text),
            child: const Text('预览邀请（不联网）'),
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
          const _SyncExplanation(
            title: '加入现有数据集',
            body: '使用独立暂存库，原记录和 Provider 不会覆盖。明确批准并安装后，'
                '自动协调正式通道、成员名册及初始快照；当前数据集不会切换。',
          ),
          if (phase != null) ...[
            const SizedBox(height: 8),
            SyncPairingSteps(
              steps: const [
                '预览并核对邀请',
                '发送加入请求',
                '等待主设备批准',
                '安装确认',
                '等待成员授权与初始同步',
              ],
              current: joinerStageFor(phase),
            ),
            const SizedBox(height: 8),
            for (final snapshotState in receiving) ...[
              SyncSnapshotProgress(
                received: snapshotState.receivedChunks,
                total: snapshotState.totalChunks,
                label: '正在接收初始快照',
              ),
              const SizedBox(height: 8),
            ],
            if (enrollment?.states.any((s) => s.storageBlocked) == true) ...[
              const SizedBox(height: 8),
              Text(
                '中继信箱已满：对方正在取件腾出空间，稍候会自动继续，无需操作。',
                style: TextStyle(fontSize: 12, color: AppTheme.warning),
              ),
            ],
            if (widget.deliveryStatuses != null) ...[
              const SizedBox(height: 8),
              SyncDeliverStatusLine(statuses: widget.deliveryStatuses!),
            ],
          ],
          if (phase == 'complete') ...[
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
                  label: const Text('通过 Key'),
                ),
                OutlinedButton.icon(
                  onPressed: controller.busy ? null : () => _scanCode(),
                  icon: const Icon(Icons.qr_code_scanner),
                  label: const Text('扫码'),
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
                '邀请设备：${state.ownerName.isEmpty ? '解锁后核验' : state.ownerName}',
              ),
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('数据集：${state.datasetId}'),
                      if (state.relayUrl.isNotEmpty) ...[
                        const SizedBox(height: 8),
                        const Text('将连接的中继'),
                        SelectableText(state.relayUrl),
                      ],
                      if (state.ownerFingerprint.isNotEmpty) ...[
                        const SizedBox(height: 8),
                        const Text('邀请设备公钥指纹'),
                        SelectableText(state.ownerFingerprint),
                      ],
                      if (state.localFingerprint != null) ...[
                        const SizedBox(height: 8),
                        const Text('本机公钥指纹（请在已有设备核对）'),
                        SelectableText(state.localFingerprint!),
                      ],
                      if (state.expiresAtMs > 0)
                        Text(
                          '邀请到期：${DateTime.fromMillisecondsSinceEpoch(state.expiresAtMs).toLocal()}',
                        ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(switch (phase) {
              'preview' => '请核对邀请后明确选择加入。',
              'key_required' || 'key_confirmation' => '正在准备并确认本机受保护密钥。',
              'ready_to_request' => '本机密钥已准备，尚未确认请求投递。',
              'awaiting_approval' => '等待已有设备核对指纹并明确批准。',
              'installation_confirmation' => '等待系统确认，确认前不发送证明。',
              'awaiting_membership' => '安装已保存，尚未确认安装证明投递。',
              'proof_sent' => '安装证明已投递，等待授权与初始同步。',
              'enrollment' => '正在自动协调已批准的初始交接，请查看交接进度。',
              'enrollment_confirmation' => '正在确认交接修订，确认前不继续发送。',
              'expired' => '邀请已过期，需重新邀请。',
              _ => '加入需要重新邀请或恢复。',
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
                  labelText: '新设备名称',
                ).copyWith(counterText: ''),
              ),
              CheckboxListTile(
                value: _consent,
                enabled: !controller.busy,
                contentPadding: EdgeInsets.zero,
                title: const Text(
                  '我已核对邀请和中继，明确同意连接该地址，并接收全部 Provider 配置和 API Key。',
                ),
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
                child: const Text('在独立暂存库发起加入'),
              ),
            ],
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (phase == 'ready_to_request' || phase == 'awaiting_approval')
                  OutlinedButton(
                    onPressed: controller.busy ? null : controller.sendRequest,
                    child: const Text('重发原加入请求'),
                  ),
                if (phase == 'awaiting_approval')
                  FilledButton(
                    onPressed: controller.busy
                        ? null
                        : controller.inspectApproval,
                    child: const Text('检查已有设备审批'),
                  ),
                if (phase == 'awaiting_membership' || phase == 'proof_sent')
                  OutlinedButton(
                    onPressed: controller.busy ? null : controller.sendProof,
                    child: const Text('重发已保存的安装证明'),
                  ),
                TextButton(
                  onPressed: controller.busy ? null : controller.cancel,
                  child: const Text('停止本机加入流程'),
                ),
              ],
            ),
            if (state.approvalAckPending && phase == 'proof_sent')
              const Text('审批邮件的清理尚未确认；重发证明可再次确认清理，不会重新安装。'),
          ],
          if (controller.busy) ...[
            const SizedBox(height: 12),
            const LinearProgressIndicator(),
          ],
          if (controller.error != null) ...[
            const SizedBox(height: 8),
            Semantics(
              liveRegion: true,
              child: Text(
                controller.error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          ],
          if (controller.notice != null) ...[
            const SizedBox(height: 8),
            Text(controller.notice!),
          ],
          if (!controller.active) ...[
            const SizedBox(height: 12),
            TextButton(
              onPressed: controller.busy ? null : controller.refreshEntries,
              child: const Text('查看可恢复的加入暂存项'),
            ),
            for (final entry in controller.entries)
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(entry.name),
                subtitle: Text(
                  '数据集：${entry.datasetId}\n仅恢复已审批检查点；未审批状态重启后需新邀请。',
                ),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TextButton(
                      onPressed: controller.busy
                          ? null
                          : () => controller.resume(entry),
                      child: const Text('解锁恢复'),
                    ),
                    TextButton(
                      onPressed: controller.busy
                          ? null
                          : () => _discardStaging(context, controller, entry),
                      child: const Text('删除'),
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
      title: const Text('删除这条暂存项？'),
      content: Text(
        '「${entry.name}」的暂存数据和本机密钥会被删除，且不可恢复。'
        '如果还想加入这个数据集，之后需要重新扫码并经对方批准。',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(false),
          child: const Text('取消'),
        ),
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(true),
          child: const Text('删除'),
        ),
      ],
    ),
  );
  if (confirmed == true && context.mounted) {
    await controller.discardStaging(entry);
  }
}
