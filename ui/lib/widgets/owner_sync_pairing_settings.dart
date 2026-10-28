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
          title: const Text('确认加入设备'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('设备：${preview.deviceName}'),
                Text('平台：${preview.devicePlatform}'),
                const SizedBox(height: 12),
                const Text('请与新设备核对公钥指纹：'),
                SelectableText(preview.fingerprint),
                const SizedBox(height: 12),
                CheckboxListTile(
                  contentPadding: EdgeInsets.zero,
                  value: consent,
                  title: const Text('确认这台设备可信，并允许其获得全部 Provider 配置和 API Key。'),
                  onChanged: (value) => update(() => consent = value ?? false),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('返回'),
            ),
            FilledButton(
              onPressed: consent ? () => Navigator.pop(context, true) : null,
              child: const Text('批准这台设备'),
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
      listenable: Listenable.merge([
        controller,
        controller.enrollment,
      ]),
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
        Text('邀请设备', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 8),
        const Text('新设备可通过“加入现有数据集”发起加入；批准后自动完成交接。'),
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
              labelText: '你已部署的中继 URL',
              hintText: 'https://…',
            ),
          ),
          CheckboxListTile(
            value: _consent,
            enabled: !controller.busy,
            contentPadding: EdgeInsets.zero,
            title: const Text('我理解：创建邀请会连接此中继；批准加入后将共享 Provider 配置和 API Key。'),
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
            label: const Text('创建短期邀请'),
          ),
        ] else ...[
          const SizedBox(height: 12),
          SyncPairingSteps(
            steps: const [
              '邀请已生成（出示二维码或复制邀请）',
              '新设备已发起加入请求',
              '已批准，正在投递审批',
              '审批已送达，等待新设备完成安装',
              '新设备接入（在「管理」确认设备列表）',
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
                'preparing_snapshot' => '正在准备初始快照',
                'awaiting_snapshot_selection' => '等待选择初始快照',
                _ => '正在发送初始快照',
              },
            ),
            const SizedBox(height: 8),
          ],
          // 交接轮询失联/失败时必须有可见原因：否则主设备也会停在同一帧转圈，
          // 用户无法区分「还在搬」和「已经卡死」。
          if (controller.enrollment.error != null) ...[
            Text(
              controller.enrollment.error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
            const SizedBox(height: 8),
          ],
          // relay 信箱打满（507）时，发送被主动停拍退避：给一个明确提示，
          // 避免用户把「在等空间」当成「卡死」。
          if (controller.enrollment.states.any((s) => s.storageBlocked)) ...[
            Text(
              '中继信箱已满：对方正在取件腾出空间，稍候会自动继续，无需操作。',
              style: TextStyle(fontSize: 12, color: AppTheme.warning),
            ),
            const SizedBox(height: 8),
          ],
          if (widget.deliveryStatuses != null) ...[
            SyncDeliverStatusLine(statuses: widget.deliveryStatuses!),
            const SizedBox(height: 8),
          ],
          const Text('邀请码仅交给可信设备。'),
          Text(
            '到期时间：${DateTime.fromMillisecondsSinceEpoch(offer.expiresAtMs).toLocal()}',
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
                    : () => Clipboard.setData(ClipboardData(text: offer.code)),
                child: const Text('复制邀请'),
              ),
              TextButton(
                onPressed: controller.busy ? null : controller.cancelInvitation,
                child: const Text('取消邀请'),
              ),
            ],
          ),
        ],
        if (preview != null) ...[
          const SizedBox(height: 12),
          Text('待审阅：${preview.deviceName}（${preview.devicePlatform}）'),
          FilledButton(
            onPressed: controller.busy ? null : () => _review(preview),
            child: const Text('核对并审批'),
          ),
        ],
        if (controller.pendingPairingId != null) ...[
          const SizedBox(height: 8),
          TextButton(
            onPressed: controller.busy
                ? null
                : controller.resendPairingApproval,
            child: const Text('重发已保存的审批'),
          ),
        ],
        if (controller.pairingNotice != null) ...[
          const SizedBox(height: 8),
          Text(controller.pairingNotice!),
        ],
      ],
        );
      },
    );
  }
}
