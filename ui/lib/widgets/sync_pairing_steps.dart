import '../l10n/localization.dart';

import 'package:material_ui/material_ui.dart';
import '../theme/design_tokens.dart';

/// 配对进度步骤条：owner/joiner 两个视角共用（用户要求绑定页面能看出
/// 「现在执行到哪一步」，而不是只有终态成功/失败文案）。
class SyncPairingSteps extends StatelessWidget {
  const SyncPairingSteps({
    super.key,
    required this.steps,
    required this.current,
  });

  /// 有序步骤文案。
  final List<String> steps;

  /// 当前所处步骤下标；小于它的显示完成，大于它的未开始。
  /// 传 -1 表示尚未开始任何步骤（全灰）。
  ///
  /// 传 `steps.length` 表示**全部完成**：没有任何一步是「进行中」，因此不会
  /// 出现转圈图标，也不会出现 ◀ 指向标记。终态必须能表达出来 —— 否则最后一步
  /// 会永远转圈（真机 2026-10-28 10:10 现场，见 roadmap C11）。
  final int current;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest.withValues(alpha: 0.3),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var i = 0; i < steps.length; i++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(
                children: [
                  Icon(
                    i < current
                        ? Icons.check_circle
                        : i == current
                        ? Icons.radio_button_checked
                        : Icons.radio_button_unchecked,
                    size: DesignTokens.iconMicro,
                    color: i <= current
                        ? theme.colorScheme.primary
                        : theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      steps[i],
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: i <= current
                            ? theme.textTheme.bodyMedium?.color
                            : theme.colorScheme.onSurfaceVariant,
                        fontWeight: i == current ? FontWeight.w600 : null,
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Owner progress uses only the current invitation's handoff facts.
int ownerPairingStage({
  required String? invitationId,
  required bool hasPreview,
  required String? pendingPairingId,
  required bool approvalDelivered,
  bool deviceJoined = false,
}) {
  if (deviceJoined) return 3;
  if (pendingPairingId != null) return 2;
  if (hasPreview) return 1;
  return invitationId == null ? -1 : 0;
}

/// Three user stages, independent of the native protocol's internal phases.
int joinerStageFor(String phase) => switch (phase) {
  'preview' => 0,
  'key_required' ||
  'key_confirmation' ||
  'ready_to_request' ||
  'awaiting_approval' ||
  'installation_confirmation' => 1,
  'proof_sent' ||
  'awaiting_membership' ||
  'enrollment' ||
  'enrollment_confirmation' => 2,
  'joined' => 3,
  _ => -1,
};

/// 初始快照传输进度条：owner「正在发送」与 joiner「正在接收」共用。
/// 数据源：enrollment 视图的 received/total chunks（快照分片级进度）。
/// 未知总量显示准备状态，不伪造百分比。
class SyncSnapshotProgress extends StatelessWidget {
  const SyncSnapshotProgress({
    super.key,
    required this.received,
    required this.total,
    this.label,
  });

  final int received;
  final int total;
  final String? label;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final value = total > 0 ? (received / total).clamp(0.0, 1.0) : null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                label ?? context.l10n.initialSnapshotTransfer,
                style: theme.textTheme.bodySmall?.copyWith(
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              total > 0 ? '${(value! * 100).floor()}% · $received/$total' : '—',
              style: theme.textTheme.labelSmall?.copyWith(
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
        const SizedBox(height: 4),
        ClipRRect(
          borderRadius: BorderRadius.circular(4),
          child: LinearProgressIndicator(value: value, minHeight: 6),
        ),
      ],
    );
  }
}
