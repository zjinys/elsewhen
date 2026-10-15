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
    this.candidates = const [],
    this.onChanged,
  });

  final SyncProcessorHandoffController controller;

  /// 本机是否是当前加工设备（来自 LocalSyncStateDto）。
  final bool isProcessor;

  /// 本机是否是名册 owner（来自 LocalSyncStateDto）。
  final bool isOwner;

  /// 可接收加工权的设备（活跃、非本机成员）；为空时禁用让出。
  final List<api.SyncRosterMemberDto> candidates;

  /// 移交动作成功后回调（供外层刷新名册等关联视图）。
  final VoidCallback? onChanged;

  @override
  State<SyncProcessorHandoffSettings> createState() =>
      _SyncProcessorHandoffSettingsState();
}

class _SyncProcessorHandoffSettingsState
    extends State<SyncProcessorHandoffSettings> {
  bool _requested = false;
  String? _targetDevice;

  @override
  void dispose() {
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
            Text('AI 分析职责移交', style: theme.textTheme.titleSmall),
            const SizedBox(height: 8),
            Text(switch (phase) {
              'idle' => '本机状态：未参与移交。',
              'relinquished' => '本机已让出 AI 分析职责，等待目标设备确认接管。',
              'pending_takeover' =>
                handoff!.canConfirm
                    ? '有移交等待本机确认（同步已追平）。'
                    : '有移交等待本机确认，但本机尚未同步到移交边界。',
              'active' => '本机正在负责 AI 分析（此前的一次接管已完成）。',
              _ => '正在读取移交状态…',
            }),
            if (phase == 'pending_takeover' && handoff!.canConfirm) ...[
              const SizedBox(height: 8),
              FilledButton.icon(
                icon: const Icon(Icons.swap_horiz),
                label: const Text('确认接管'),
                onPressed: controller.busy
                    ? null
                    : () async {
                        await controller.confirm();
                        widget.onChanged?.call();
                      },
              ),
            ],
            if (phase == 'idle' && widget.isProcessor) ...[
              const SizedBox(height: 8),
              if (widget.candidates.isEmpty)
                Text('暂无可接收设备，请先授权设备加入。', style: theme.textTheme.bodySmall)
              else ...[
                DropdownButtonFormField<String>(
                  initialValue: _targetDevice,
                  decoration: const InputDecoration(
                    labelText: '目标设备',
                    border: OutlineInputBorder(),
                  ),
                  items: widget.candidates
                      .map(
                        (member) => DropdownMenuItem(
                          value: member.deviceId,
                          child: Text('${member.name}（${member.deviceId}）'),
                        ),
                      )
                      .toList(),
                  onChanged: controller.busy
                      ? null
                      : (value) => setState(() => _targetDevice = value),
                ),
                const SizedBox(height: 8),
                FilledButton.icon(
                  icon: const Icon(Icons.logout),
                  label: const Text('让出 AI 分析职责'),
                  onPressed: controller.busy || _targetDevice == null
                      ? null
                      : () => _confirmYield(context, controller),
                ),
              ],
            ],
            if (phase == 'relinquished' && !widget.isOwner) ...[
              const SizedBox(height: 8),
              Text('等待主设备审批移交证明。', style: theme.textTheme.bodySmall),
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
    final target = _targetDevice;
    if (target == null) return;
    final matches = widget.candidates
        .where((candidate) => candidate.deviceId == target)
        .toList();
    final targetLabel = matches.isEmpty ? target : matches.first.name;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('让出 AI 分析职责？'),
        content: Text(
          '将把 AI 分析职责移交给 "$targetLabel"（$target）。\n\n'
          '本机会立即停止认领后台任务；目标设备需同步到移交边界并显式确认后'
          '才开始 AI 分析。确认后本机不能单方面恢复。',
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
      widget.onChanged?.call();
    }
  }
}

/// 供测试/文档理解守门语义：接管按钮只在 canConfirm 且 pending_takeover 展示。
bool handoffCanConfirm(api.ProcessorHandoffStateDto? state) =>
    state != null && state.state == 'pending_takeover' && state.canConfirm;
