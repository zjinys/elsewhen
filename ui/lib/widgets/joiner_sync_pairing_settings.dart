import 'dart:io';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_join_controller.dart';
import '../utils/device_name.dart';
import 'joiner_qr_scan_page.dart';
import 'sync_pairing_steps.dart';

class JoinerSyncPairingSettings extends StatefulWidget {
  const JoinerSyncPairingSettings({
    super.key,
    required this.controller,
    this.scannerBuilder,
  });
  final SyncJoinController controller;

  /// 扫码页取景器注入缝（测试替换相机）；见 JoinerQrScanPage。
  final Widget Function(BuildContext context, void Function(String code))?
  scannerBuilder;

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

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.controller,
    builder: (context, _) {
      final controller = widget.controller;
      final state = controller.state;
      final phase = state?.phase;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Divider(height: 32),
          Text('加入现有数据集', style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 8),
          const Text(
            '使用独立暂存库，原记录和 Provider 不会覆盖。明确批准并安装后，自动协调正式通道、成员名册及初始快照；当前数据集不会切换。',
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
          ],
          if (!controller.active) ...[
            const SizedBox(height: 12),
            TextField(
              controller: _code,
              enabled: !controller.busy,
              autocorrect: false,
              enableSuggestions: false,
              maxLength: 4200,
              maxLines: 3,
              decoration: const InputDecoration(
                labelText: '来自可信设备的邀请码',
                border: OutlineInputBorder(),
              ),
            ),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton(
                  onPressed: controller.busy
                      ? null
                      : () async {
                          setState(() => _consent = false);
                          await controller.preview(_code.text);
                          if (mounted && controller.active) _code.clear();
                        },
                  child: const Text('预览邀请（不联网）'),
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
            Text('邀请设备：${state.ownerName.isEmpty ? '解锁后核验' : state.ownerName}'),
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
                decoration: const InputDecoration(
                  labelText: '新设备名称',
                  border: OutlineInputBorder(),
                ),
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
