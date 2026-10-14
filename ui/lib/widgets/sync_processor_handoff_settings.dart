import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_processor_handoff_controller.dart';

/// 设置页的加工移交区块（P4d）。
///
/// 只读展示状态机位置 + 两个显式动作（让出 / 接管）。绝不自动推进状态机：
/// 让出需要填写目标设备并二次确认；接管在 Rust 侧 frontier 追平前失败关闭。
class SyncProcessorHandoffSettings extends StatefulWidget {
  const SyncProcessorHandoffSettings({
    super.key,
    required this.controller,
    required this.isProcessor,
    required this.isOwner,
  });

  final SyncProcessorHandoffController controller;

  /// 本机是否是当前加工设备（来自 LocalSyncStateDto）。
  final bool isProcessor;

  /// 本机是否是名册 owner（来自 LocalSyncStateDto）。
  final bool isOwner;

  @override
  State<SyncProcessorHandoffSettings> createState() =>
      _SyncProcessorHandoffSettingsState();
}

class _SyncProcessorHandoffSettingsState
    extends State<SyncProcessorHandoffSettings> {
  bool _requested = false;
  final _targetDevice = TextEditingController();

  @override
  void dispose() {
    _targetDevice.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!_requested) {
      _requested = true;
      // 首次构建后再触发，避免 build 中发起异步。
      WidgetsBinding.instance.addPostFrameCallback((_) {
        widget.controller.refresh();
      });
    }
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final handoff = controller.state;
        final phase = handoff?.state ?? 'unknown';
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 16),
            Text('加工移交', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Text(
              '加工设备负责后台分析/消化。出让后本机不再认领后台任务，目标设备'
              '同步到移交边界并显式确认后才开始工作。',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(switch (phase) {
              'idle' => '本机状态：未参与移交。',
              'relinquished' => '本机已让出加工权，等待目标设备确认接管。',
              'pending_takeover' =>
                handoff!.canConfirm
                    ? '有移交等待本机确认（同步已追平）。'
                    : '有移交等待本机确认，但本机尚未同步到移交边界。',
              'active' => '本机正在加工（此前的一次接管已完成）。',
              _ => '正在读取移交状态…',
            }),
            if (phase == 'pending_takeover' && handoff!.canConfirm) ...[
              const SizedBox(height: 8),
              FilledButton.icon(
                icon: const Icon(Icons.swap_horiz),
                label: const Text('确认接管'),
                onPressed: controller.busy ? null : controller.confirm,
              ),
            ],
            if (phase == 'idle' && widget.isProcessor) ...[
              const SizedBox(height: 8),
              TextField(
                controller: _targetDevice,
                enabled: !controller.busy,
                maxLength: 64,
                decoration: const InputDecoration(
                  labelText: '目标设备 ID',
                  border: OutlineInputBorder(),
                ),
              ),
              const SizedBox(height: 8),
              FilledButton.icon(
                icon: const Icon(Icons.logout),
                label: const Text('让出加工权'),
                onPressed: controller.busy
                    ? null
                    : () => _confirmYield(context, controller),
              ),
            ],
            if (phase == 'relinquished' && !widget.isOwner) ...[
              const SizedBox(height: 8),
              Text(
                '移交证明需要由 owner 设备审批签发新名册后才会生效。',
                style: theme.textTheme.bodySmall,
              ),
            ],
            if (controller.error != null) ...[
              const SizedBox(height: 8),
              Semantics(
                liveRegion: true,
                child: Text(
                  controller.error!,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ),
            ],
            if (controller.busy) ...[
              const SizedBox(height: 8),
              const Row(
                children: [
                  SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                  SizedBox(width: 8),
                  Expanded(child: Text('正在处理移交步骤，请等待…')),
                ],
              ),
            ],
          ],
        );
      },
    );
  }

  Future<void> _confirmYield(
    BuildContext context,
    SyncProcessorHandoffController controller,
  ) async {
    final target = _targetDevice.text.trim();
    if (target.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('让出加工权？'),
        content: Text(
          '将把加工权移交给设备 "$target"。\n\n'
          '本机会立即停止认领后台任务；目标设备需同步到移交边界并显式确认后'
          '才开始加工。确认后本机不能单方面恢复加工。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('让出'),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await controller.yieldTo(
        toDeviceId: target,
        isOwnerDevice: widget.isOwner,
      );
    }
  }
}

/// 供测试/文档理解守门语义：接管按钮只在 canConfirm 且 pending_takeover 展示。
bool handoffCanConfirm(api.ProcessorHandoffStateDto? state) =>
    state != null && state.state == 'pending_takeover' && state.canConfirm;
