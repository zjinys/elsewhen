import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_object_conflicts_controller.dart';

/// 待办/目标/规则同步冲突的只读审阅列表 + 显式裁决入口（与 wiki 页冲突同款）。
///
/// 双端状态并列（主文本/状态/更新时间）；候选不搬运全文历史。裁决只支持
/// 「保留本端」/「采纳对端」二选一——保留 = 本端现状为准（删除态即裁决，
/// 编辑态重新捕获广播）；采纳 = 强制应用对端候选 op（goal_active_limit
/// 采纳被 Rust 侧显式拒绝）。绑定由 Rust 侧失败关闭。
class SyncObjectConflictsSettings extends StatefulWidget {
  const SyncObjectConflictsSettings({super.key, required this.controller});

  final SyncObjectConflictsController controller;

  @override
  State<SyncObjectConflictsSettings> createState() =>
      _SyncObjectConflictsSettingsState();
}

class _SyncObjectConflictsSettingsState
    extends State<SyncObjectConflictsSettings> {
  bool _requested = false;

  @override
  Widget build(BuildContext context) {
    if (!_requested) {
      _requested = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        widget.controller.refresh();
      });
    }
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final conflicts = controller.conflicts;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 24),
            Text('待办与目标同步冲突', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Text(
              '来自其他设备的待办、目标、规则与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。',
              style: theme.textTheme.bodySmall,
            ),
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
            const SizedBox(height: 8),
            if (controller.busy && conflicts == null)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 8),
                child: Row(
                  children: [
                    SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                    SizedBox(width: 8),
                    Text('正在读取冲突…'),
                  ],
                ),
              )
            else if (conflicts != null && conflicts.isEmpty)
              Text('没有待审阅的待办/目标/规则冲突。', style: theme.textTheme.bodySmall)
            else if (conflicts != null)
              ...conflicts.map(
                (conflict) => _ObjectConflictCard(
                  key: ValueKey(conflict.opId),
                  conflict: conflict,
                  busy: controller.busy,
                  onResolve: (acceptRemote) =>
                      controller.resolve(conflict, acceptRemote: acceptRemote),
                ),
              ),
          ],
        );
      },
    );
  }
}

String _kindText(String kind) => switch (kind) {
  'todo' => '待办',
  'goal' => '目标',
  'rule' => '规则',
  _ => kind,
};

String _reasonText(String reason) => switch (reason) {
  'todo_deleted' => '对端已删除本条，而之后仍有改动要送达',
  'goal_deleted' => '对端已删除本条（目标无删除语义，删除后收到编辑必审）',
  'rule_deleted' => '对端已删除本条，而之后仍有改动要送达',
  'base_revision_changed' => '两端都修改了同一条，各自基于旧版本',
  'goal_active_limit' => '对端目标会使本端活跃目标超出上限',
  _ => reason,
};

class _ObjectConflictCard extends StatelessWidget {
  const _ObjectConflictCard({
    super.key,
    required this.conflict,
    required this.busy,
    required this.onResolve,
  });

  final api.ObjectConflictDto conflict;
  final bool busy;
  final Future<bool> Function(bool acceptRemote) onResolve;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(vertical: 8),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${_kindText(conflict.objectKind)} · 来自 ${conflict.senderDeviceName ?? conflict.senderDeviceId} 的改动',
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 4),
            Text(
              _reasonText(conflict.reason),
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.tertiary,
              ),
            ),
            const SizedBox(height: 8),
            _SideView(
              title: conflict.localHeadDeleted
                  ? '本端（已删除状态）'
                  : '本端当前',
              summary: conflict.localSummary,
              status: conflict.localStatus,
              updatedAt: conflict.localUpdatedAt,
              deleted: conflict.localHeadDeleted,
            ),
            const Divider(height: 24),
            _SideView(
              title: '对端送来的版本',
              summary: conflict.incomingSummary,
              status: conflict.incomingStatus,
              updatedAt: conflict.incomingUpdatedAt,
              deleted: conflict.incomingDeleted,
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  icon: const Icon(Icons.computer_outlined),
                  label: const Text('保留本端'),
                  onPressed: busy ? null : () => onResolve(false),
                ),
                FilledButton.icon(
                  icon: const Icon(Icons.cloud_download_outlined),
                  label: const Text('采纳对端'),
                  onPressed: busy ? null : () => onResolve(true),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _SideView extends StatelessWidget {
  const _SideView({
    required this.title,
    required this.summary,
    required this.status,
    required this.updatedAt,
    required this.deleted,
  });

  final String title;
  final String? summary;
  final String? status;
  final String? updatedAt;
  final bool deleted;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.labelLarge),
        const SizedBox(height: 4),
        Text(
          deleted ? '已删除' : (summary ?? '（空）'),
          style: theme.textTheme.bodyMedium,
        ),
        if (!deleted && status != null)
          Text('状态 $status · 更新 ${updatedAt ?? '-'}',
              style: theme.textTheme.bodySmall),
      ],
    );
  }
}
