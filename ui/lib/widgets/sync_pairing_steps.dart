import 'package:flutter/material.dart';

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
                        : (i == current
                              ? Icons.radio_button_checked
                              : Icons.radio_button_unchecked),
                    size: 14,
                    color: i < current
                        ? theme.colorScheme.primary
                        : (i == current
                              ? theme.colorScheme.primary
                              : theme.colorScheme.outline),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      steps[i],
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: i <= current
                            ? theme.textTheme.bodyMedium?.color
                            : theme.colorScheme.outline,
                        fontWeight: i == current ? FontWeight.w600 : null,
                      ),
                    ),
                  ),
                  // 不用 CircularProgressIndicator：它是不确定时长动画，
                  // 会让测试 pumpAndSettle 永不 idle；label 加粗已足够强调。
                  if (i == current)
                    Text('◀', style: theme.textTheme.labelSmall),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// owner 视角：当前推进到第几步（步骤数组见 owner 卡接线处）。
int ownerPairingStage({
  required String? invitationId,
  required bool hasPreview,
  required String? pendingPairingId,
  required bool approvalDelivered,
}) {
  if (pendingPairingId != null) {
    // 审批已持久化；投递由轮询自动重试，确认送达即停在「等待安装」。
    return approvalDelivered ? 4 : 3;
  }
  if (hasPreview) return 2;
  if (invitationId != null) return 1;
  return 0;
}

/// joiner 视角：phase 名 → 步骤下标。
int joinerStageFor(String phase) => switch (phase) {
  'preview' => 0,
  'key_required' || 'key_confirmation' || 'ready_to_request' => 1,
  'awaiting_approval' => 2,
  'installation_confirmation' || 'proof_sent' => 3,
  'awaiting_membership' || 'enrollment' || 'enrollment_confirmation' => 4,
  _ => -1,
};
