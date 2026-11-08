import '../l10n/localization.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_wiki_conflicts_controller.dart';
import 'sync_conflict_section.dart';

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
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final conflicts = controller.conflicts;
        return SyncConflictSection(
          icon: Icons.menu_book_outlined,
          title: context.l10n.l6KnowledgeSyncConflicts67265,
          explainer:
              context.l10n.l6KnowledgeChangesFromAnotherDeviceConflict5a183,
          loaded: conflicts != null,
          busy: controller.busy,
          error: controller.error,
          emptyText: context.l10n.l6NoKnowledgeSyncConflictsNeedReview2246e,
          cards: [
            for (final conflict in conflicts ?? const [])
              _WikiConflictCard(
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

String _reasonText(BuildContext context, String reason) => switch (reason) {
  'wiki_page_deleted' => context.l10n.l6TheIncomingVersionDeletesThisPagec27d8,
  'wiki_page_rename_diverged' =>
    context.l10n.l6BothDevicesRenamedThisPageDifferently200f1,
  'slug_owned_by_other_id' =>
    context.l10n.l6TheIncomingPathConflictsWithAnother49464,
  'base_revision_changed' => context.l10n.l6TheIncomingEditIsBasedOnf0ddf,
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
              context.l10n.l6ChangesFromArg0710df(
                (conflict.senderDeviceName ?? conflict.senderDeviceId)
                    .toString(),
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
                      ? context.l10n.l6LocalVersionDeleted25fe7
                      : context.l10n.syncLocalCurrent,
                  slug: conflict.local?.slug,
                  title_: conflict.local?.title,
                  summary: null,
                  contentMdLen: conflict.local?.contentMdLen,
                  updatedAt: conflict.local?.updatedAt,
                  deleted: conflict.localHeadDeleted,
                ),
                _SideView(
                  title: context.l10n.syncIncomingVersion,
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
          Text(
            context.l10n.l6ThisPageDoesNotExistLocallya6853,
            style: theme.textTheme.bodySmall,
          ),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.labelLarge),
        const SizedBox(height: 4),
        Text(
          deleted
              ? context.l10n.syncDeleted
              : (title_ ?? context.l10n.l6Untitledd8f9b),
          style: theme.textTheme.bodyMedium,
        ),
        if (!deleted && slug != null)
          SelectableText(slug!, style: theme.textTheme.bodySmall),
        if (renamedFrom != null)
          Text(
            context.l10n.l6RenamedFromArg0e3178((renamedFrom).toString()),
            style: theme.textTheme.bodySmall,
          ),
        if (summary != null && summary!.isNotEmpty)
          Text(
            summary!,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        if (!deleted && contentMdLen != null)
          Text(
            context.l10n.l6ContentLengthArg0CharactersUpdatedArg1f8788(
              (contentMdLen!).toString(),
              (updatedAt ?? '-').toString(),
            ),
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}
