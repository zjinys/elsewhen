import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_object_conflicts_controller.dart';
import 'sync_conflict_section.dart';
import '../l10n/localization.dart';
import '../utils/pretty_time.dart';

/// 待办/目标/计划/规则同步冲突的只读审阅列表 + 显式裁决入口（与 wiki 页冲突同款）。
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
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final conflicts = controller.conflicts;
        return SyncConflictSection(
          icon: Icons.checklist_outlined,
          title: context.l10n.syncObjectConflictsTitle,
          explainer: context.l10n.syncObjectConflictsHelp,
          loaded: conflicts != null,
          busy: controller.busy,
          error: controller.error,
          emptyText: context.l10n.syncObjectConflictsEmpty,
          cards: [
            for (final conflict in conflicts ?? const [])
              _ObjectConflictCard(
                key: ValueKey(conflict.opId),
                conflict: conflict,
                busy: controller.busy,
                onResolve: (acceptRemote) =>
                    controller.resolve(conflict, acceptRemote: acceptRemote),
              ),
          ],
        );
      },
    );
  }
}

String _kindText(BuildContext context, String kind) => switch (kind) {
  'todo' => context.l10n.syncKindTodo,
  'goal' => context.l10n.syncKindGoal,
  'plan' => context.l10n.syncKindPlan,
  'rule' => context.l10n.syncKindRule,
  'knowledge_language' => context.l10n.knowledgeLanguage,
  _ => kind,
};

String _reasonText(BuildContext context, String reason) => switch (reason) {
  'todo_deleted' => context.l10n.syncDeletedReason,
  'goal_deleted' => context.l10n.syncGoalDeletedReason,
  'plan_deleted' => context.l10n.syncDeletedReason,
  'rule_deleted' => context.l10n.syncDeletedReason,
  'base_revision_changed' => context.l10n.syncBaseChangedReason,
  'goal_active_limit' => context.l10n.syncGoalLimitReason,
  'knowledge_language_generation_changed' =>
    context.l10n.syncLanguageChangedReason,
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
              context.l10n.syncConflictFrom(
                _kindText(context, conflict.objectKind),
                conflict.senderDeviceName ?? conflict.senderDeviceId,
              ),
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 4),
            Text(
              _reasonText(context, conflict.reason),
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.tertiary,
              ),
            ),
            const SizedBox(height: 8),
            SyncConflictSides(
              children: [
                _SideView(
                  title: conflict.localHeadDeleted
                      ? context.l10n.syncLocalDeleted
                      : context.l10n.syncLocalCurrent,
                  summary: conflict.localSummary,
                  status: conflict.localStatus,
                  updatedAt: conflict.localUpdatedAt,
                  deleted: conflict.localHeadDeleted,
                ),
                _SideView(
                  title: context.l10n.syncIncomingVersion,
                  summary: conflict.incomingSummary,
                  status: conflict.incomingStatus,
                  updatedAt: conflict.incomingUpdatedAt,
                  deleted: conflict.incomingDeleted,
                ),
              ],
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  icon: const Icon(Icons.computer_outlined),
                  label: Text(context.l10n.syncKeepLocal),
                  onPressed: busy ? null : () => onResolve(false),
                ),
                FilledButton.icon(
                  icon: const Icon(Icons.cloud_download_outlined),
                  label: Text(context.l10n.syncAcceptIncoming),
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
          deleted
              ? context.l10n.syncDeleted
              : (summary ?? context.l10n.syncEmptyValue),
          style: theme.textTheme.bodyMedium,
        ),
        if (!deleted && status != null)
          Text(
            context.l10n.syncStatusUpdated(switch (status) {
              'open' => context.l10n.todoStatusOpen,
              'done' => context.l10n.completed,
              'archived' => context.l10n.todoStatusArchived,
              'active' => context.l10n.active,
              'paused' => context.l10n.pausedGoalState,
              _ => status!,
            }, prettyTimestamp(updatedAt, strings: context.l10n) ?? '-'),
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}
