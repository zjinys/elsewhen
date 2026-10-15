import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_enrollment_controller.dart';

/// Read-only initial-transfer progress; it is not a dataset selection action.
class SyncEnrollmentProgress extends StatelessWidget {
  const SyncEnrollmentProgress({super.key, required this.controller});

  final SyncEnrollmentController controller;

  String _phase(api.SyncEnrollmentStateDto state) {
    if (state.confirmationToken != null) {
      return '等待系统确认；确认前不对外发送。';
    }
    return switch (state.phase) {
      'awaiting_proof' => '等待并验证新设备的安装证明。',
      'preparing_channel' => '正在准备正式通道。',
      'awaiting_roster' => '等待签名成员名册及其确认。',
      'preparing_snapshot' => '正在准备一致的初始快照。',
      'awaiting_snapshot_selection' => '等待初始快照目标确认。',
      'sending_snapshot' => '正在发送初始快照。',
      'receiving_snapshot' => '正在接收并校验初始快照。',
      'awaiting_receipt' => '初始快照应用回执仍待确认。',
      'complete' => state.role == 'joiner' ? '初始快照已应用，已回执确认。' : '新设备已应用初始快照。',
      'expired' => '交接授权已过期，需重新邀请。',
      'revoked' => '授权已撤销，交接已停止。',
      _ => '交接状态需要检查。',
    };
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: controller,
    builder: (context, _) {
      if (controller.states.isEmpty && controller.error == null) {
        return const SizedBox.shrink();
      }
      final theme = Theme.of(context);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Divider(height: 32),
          Text('已批准设备的初始交接', style: theme.textTheme.titleSmall),
          const SizedBox(height: 8),
          const Text('自动处理已批准的交接；初始交接完成后自动持续同步。'),
          for (final state in controller.states) ...[
            const SizedBox(height: 12),
            Text(state.role == 'owner' ? '已有设备 → 新设备' : '新设备暂存库'),
            Text(_phase(state)),
            if (state.totalChunks > 0)
              Text('已确认分块：${state.receivedChunks} / ${state.totalChunks}'),
            if (state.role == 'joiner' &&
                state.localApplied &&
                !state.remoteApplied)
              const Text('本机快照已应用，等待对方回执。'),
            if (state.notificationPending)
              Text(state.role == 'owner' ? '完成通知等待新设备确认。' : '完成通知等待投递。'),
            if (state.phase == 'complete') const Text('数据保留在暂存库，未切换数据集。'),
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
        ],
      );
    },
  );
}
