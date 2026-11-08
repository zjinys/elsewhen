import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_processor_handoff_controller.dart';
import '../theme/app_theme.dart';

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
            Text(
              context.l10n.l6TransferAiAnalysisResponsibilityfe457,
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 8),
            Text(switch (phase) {
              'idle' => context.l10n.l6ThisDeviceIsNotInvolvedInaf62a,
              'relinquished' =>
                context.l10n.l6ResponsibilityHasBeenHandedOffWaitingeac50,
              'pending_takeover' =>
                handoff!.canConfirm
                    ? context.l10n.l6AHandoffAwaitsThisDeviceSdd081
                    : context.l10n.l6AHandoffAwaitsConfirmationButThis0f114,
              'active' => context.l10n.l6ThisDeviceRunsAiAnalysisThe54d4b,
              _ => context.l10n.l6LoadingHandoffStatus19264,
            }),
            if (phase == 'pending_takeover' && handoff!.canConfirm) ...[
              const SizedBox(height: 8),
              FilledButton.icon(
                icon: const Icon(Icons.swap_horiz),
                label: Text(context.l10n.l6AcceptResponsibility6c220),
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
                Text(
                  context.l10n.l6NoEligibleTargetDevicesAuthorizeAaba52,
                  style: theme.textTheme.bodySmall,
                )
              else ...[
                DropdownButtonFormField<String>(
                  initialValue: _targetDevice,
                  decoration: AppTheme.fieldDecoration(
                    labelText: context.l10n.l6TargetDevice1b73f,
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
                  label: Text(
                    context.l10n.l6TransferAiAnalysisResponsibility2e5ba,
                  ),
                  onPressed: controller.busy || _targetDevice == null
                      ? null
                      : () => _confirmYield(context, controller),
                ),
              ],
            ],
            if (phase == 'relinquished' && !widget.isOwner) ...[
              const SizedBox(height: 8),
              Text(
                context.l10n.l6WaitingForTheOwnerToApprovebf73f,
                style: theme.textTheme.bodySmall,
              ),
            ],
            if (controller.error != null) ...[
              const SizedBox(height: 8),
              Semantics(
                liveRegion: true,
                child: AppErrorMessage(
                  error: controller.error!,
                  action: context.l10n.load,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ),
            ],
            if (controller.busy) ...[
              const SizedBox(height: 8),
              Row(
                children: [
                  SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                  SizedBox(width: 8),
                  Expanded(child: Text(context.l10n.l6ProcessingHandoffed58d)),
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
        title: Text(context.l10n.l6TransferAiAnalysisResponsibility5d2ad),
        content: Text(
          context.l10n.l6TransferAiAnalysisResponsibilityToArg05bc00(
            (targetLabel).toString(),
            (target).toString(),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text(context.l10n.l6Transferc79cc),
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
