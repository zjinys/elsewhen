import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_enrollment_controller.dart';

/// Read-only initial-transfer progress; it is not a dataset selection action.
class SyncEnrollmentProgress extends StatelessWidget {
  const SyncEnrollmentProgress({super.key, required this.controller});

  final SyncEnrollmentController controller;

  String _phase(api.SyncEnrollmentStateDto state) {
    if (state.confirmationToken != null) {
      return '等待系统修订确认；确认前不会继续对外发送。';
    }
    return switch (state.phase) {
      'awaiting_proof' => '等待并验证新设备的安装证明。',
      'preparing_channel' => '正在准备正式通道并收回临时发送权限。',
      'awaiting_roster' => '等待签名成员名册及其确认。',
      'preparing_snapshot' => '正在准备一致的初始快照。',
      'awaiting_snapshot_selection' => '等待初始快照目标确认；不会提前发送分块。',
      'sending_snapshot' => '正在发送初始快照。',
      'receiving_snapshot' => '正在接收并校验初始快照。',
      'awaiting_receipt' => '初始快照应用回执仍待确认。',
      'complete' =>
        state.role == 'joiner' ? '初始快照已应用，旧设备已确认应用回执。' : '新设备已应用初始快照。',
      'expired' => '本次交接的授权已过期，需要恢复或重新邀请。',
      'revoked' => '成员授权已撤销，已停止交接。',
      _ => '交接状态需要检查；不会重置密钥或覆盖原库。',
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
          const Text('自动处理已明确批准的交接，不会批准新的加入请求。当前数据集不会切换，持续增量同步尚未开放。'),
          for (final state in controller.states) ...[
            const SizedBox(height: 12),
            Text(state.role == 'owner' ? '旧设备 → 新设备' : '新设备暂存库'),
            Text(_phase(state)),
            if (state.totalChunks > 0)
              Text('已确认分块：${state.receivedChunks} / ${state.totalChunks}'),
            if (state.role == 'joiner' &&
                state.localApplied &&
                !state.remoteApplied)
              const Text('本机快照已应用；尚不能声称旧设备已收到应用回执。'),
            if (state.notificationPending)
              Text(
                state.role == 'owner'
                    ? '完成通知仍待新设备确认，将继续重试。'
                    : '完成通知的回执仍待投递，将继续重试。',
              ),
            if (state.phase == 'complete')
              const Text('暂存库保留，可在后续数据集选择功能中切换；本次没有切换或接管后台加工。'),
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
