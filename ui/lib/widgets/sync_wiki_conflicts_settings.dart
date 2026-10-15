import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_wiki_conflicts_controller.dart';

/// Wiki 页同步冲突的只读审阅列表 + 显式裁决入口（与 Provider 冲突同款）。
///
/// 双端状态并列（标题/路径/正文长度/更新时间）；候选正文不全文搬运。裁决
/// 只支持「保留本端」/「采纳对端」二选一——保留 = 本端现状为准（删除态则
/// 物理删除，编辑态则重新捕获广播）；采纳 = 强制应用对端候选 op。绑定由
/// Rust 侧失败关闭。
class SyncWikiConflictsSettings extends StatefulWidget {
  const SyncWikiConflictsSettings({super.key, required this.controller});

  final SyncWikiConflictsController controller;

  @override
  State<SyncWikiConflictsSettings> createState() =>
      _SyncWikiConflictsSettingsState();
}

class _SyncWikiConflictsSettingsState extends State<SyncWikiConflictsSettings> {
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
            const SizedBox(height: 16),
            Text('知识库同步冲突', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Text(
              '来自其他设备的知识页改动与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。',
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
              Text('没有待审阅的知识库同步冲突。', style: theme.textTheme.bodySmall)
            else if (conflicts != null)
              ...conflicts.map(
                (conflict) => _WikiConflictCard(
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

String _reasonText(String reason) => switch (reason) {
  'wiki_page_deleted' => '对端已删除本页，而本端之后仍有改动要送达',
  'wiki_page_rename_diverged' => '两端把本页改名到了不同标题',
  'slug_owned_by_other_id' => '对端的新路径与本端另一页重名',
  'base_revision_changed' => '对端的修改基于旧版本，本页已先变过',
  _ => reason,
};

class _WikiConflictCard extends StatelessWidget {
  const _WikiConflictCard({
    super.key,
    required this.conflict,
    required this.busy,
    required this.onResolve,
  });

  final api.WikiPageConflictDto conflict;
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
              '来自 ${conflict.senderDeviceName ?? conflict.senderDeviceId} 的改动',
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
              title: conflict.localHeadDeleted ? '本端（已删除 State）' : '本端当前',
              slug: conflict.local?.slug,
              title_: conflict.local?.title,
              summary: null,
              contentMdLen: conflict.local?.contentMdLen,
              updatedAt: conflict.local?.updatedAt,
              deleted: conflict.localHeadDeleted,
            ),
            const Divider(height: 24),
            _SideView(
              title: '对端送来的版本',
              slug: conflict.incomingSlug,
              title_: conflict.incomingTitle,
              summary: conflict.incomingSummary.isEmpty
                  ? null
                  : conflict.incomingSummary,
              contentMdLen: conflict.incomingContentMdLen,
              updatedAt: conflict.incomingUpdatedAt,
              deleted: conflict.incomingDeleted,
              renamedFrom: conflict.incomingRenamedFrom,
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
    required this.slug,
    required this.title_,
    required this.summary,
    required this.contentMdLen,
    required this.updatedAt,
    required this.deleted,
    this.renamedFrom,
  });

  final String title;
  final String? slug;
  final String? title_;
  final String? summary;
  final int? contentMdLen;
  final String? updatedAt;
  final bool deleted;
  final String? renamedFrom;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (slug == null && !deleted) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: theme.textTheme.labelLarge),
          const SizedBox(height: 4),
          Text('（本端没有该页）', style: theme.textTheme.bodySmall),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.labelLarge),
        const SizedBox(height: 4),
        Text(
          deleted ? '已删除' : (title_ ?? '（无标题）'),
          style: theme.textTheme.bodyMedium,
        ),
        if (!deleted && slug != null)
          SelectableText(slug!, style: theme.textTheme.bodySmall),
        if (renamedFrom != null)
          Text('由「$renamedFrom」改名而来', style: theme.textTheme.bodySmall),
        if (summary != null && summary!.isNotEmpty)
          Text(
            summary!,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        if (!deleted && contentMdLen != null)
          Text(
            '正文 ${contentMdLen!} 字 · 更新 ${updatedAt ?? '-'}',
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}
