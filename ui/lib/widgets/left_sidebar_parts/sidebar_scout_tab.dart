part of '../left_sidebar.dart';

//! 情报（scout）来源筛选面板：一级入口「情报」的上下文面板。

/// 情报来源列表：来源数量会持续增长，平铺 chip 挤不下，收进侧栏
/// 上下文面板自然滚动。选中语义跟随当前工作区：
/// - 总览（产品网格）：多选 toggle（scoutProductSourcesProvider）；
/// - 原始动态（findings）：单选（scoutSourceFilterProvider），再点一次取消。
/// 「全部」行清空对应筛选。
class _ScoutSourcePanel extends ConsumerWidget {
  const _ScoutSourcePanel();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workspace = ref.watch(scoutWorkspaceProvider);
    final multi = workspace == ScoutWorkspace.discover;
    final selectedIds = multi
        ? ref.watch(scoutProductSourcesProvider)
        : switch (ref.watch(scoutSourceFilterProvider)) {
            final id? => {id},
            null => const <String>{},
          };
    final summaries = ref.watch(scoutSourceSummariesProvider);
    final live = ref.watch(scoutSourcesProvider);
    final names = {
      for (final s in live.value ?? <api.ScoutSourceDto>[]) s.id: s.name,
    };
    final unread = {
      for (final s in summaries.value ?? <ScoutSourceSummary>[])
        s.sourceId: s.unread,
    };
    final ids = {...names.keys, ...selectedIds}.toList()..sort();

    void select(String? id) {
      if (multi) {
        final notifier = ref.read(scoutProductSourcesProvider.notifier);
        if (id == null) {
          notifier.clear();
        } else {
          notifier.toggle(id);
        }
      } else {
        ref
            .read(scoutSourceFilterProvider.notifier)
            .set(ref.read(scoutSourceFilterProvider) == id ? null : id);
        // 来源筛选只在动态工作区生效：点了来源就切过去，与原抽屉导航一致。
        ref.read(scoutWorkspaceProvider.notifier).set(ScoutWorkspace.activity);
      }
      _closeModalDrawer(context);
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
          child: Text(
            context.l10n.scoutSources,
            style: TextStyle(
              color: AppTheme.textTertiary,
              fontSize: 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
        Expanded(
          child: summaries.isLoading && ids.isEmpty
              ? const Center(child: CircularProgressIndicator())
              : summaries.hasError && ids.isEmpty
              ? Center(
                  child: TextButton(
                    onPressed: () => ref.invalidate(scoutSourceSummariesProvider),
                    child: Text(context.l10n.scoutRetry),
                  ),
                )
              : ListView(
                  padding: const EdgeInsets.only(bottom: 8),
                  children: [
                    ListTile(
                      key: const ValueKey('scout-source-all'),
                      dense: true,
                      selected: selectedIds.isEmpty,
                      leading: const Icon(Icons.filter_list, size: 18),
                      title: Text(context.l10n.scoutFilterAll),
                      onTap: () => select(null),
                    ),
                    for (final id in ids)
                      ListTile(
                        key: ValueKey('scout-source-$id'),
                        dense: true,
                        selected: selectedIds.contains(id),
                        leading: ScoutSourceIcon(
                          source: scoutSourceIdentity(
                            context,
                            id,
                            names[id],
                          ),
                        ),
                        title: Text(
                          scoutSourceIdentity(context, id, names[id]).platform,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle: scoutSourceIdentity(
                                  context,
                                  id,
                                  names[id],
                                ).topic ==
                                scoutSourceIdentity(context, id, names[id])
                                    .platform
                            ? null
                            : Text(
                                scoutSourceIdentity(
                                  context,
                                  id,
                                  names[id],
                                ).topic,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: TypographyTokens.secondary,
                                  color: AppTheme.textTertiary,
                                ),
                              ),
                        trailing: multi && selectedIds.contains(id)
                            ? Icon(
                                Icons.check,
                                size: DesignTokens.iconMicro,
                                color: AppTheme.accentPrimary,
                              )
                            : (unread[id] ?? 0) > 0
                            ? Text(
                                '${unread[id]}',
                                style: TextStyle(
                                  color: AppTheme.textTertiary,
                                  fontSize: 12,
                                ),
                              )
                            : null,
                        onTap: () => select(id),
                      ),
                  ],
                ),
        ),
      ],
    );
  }
}
