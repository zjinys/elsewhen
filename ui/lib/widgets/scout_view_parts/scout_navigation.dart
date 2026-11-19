part of '../scout_view.dart';

/// 宽侧栏与窄屏来源菜单复用同一份导航。
class ScoutSourceNavigation extends ConsumerWidget {
  final VoidCallback? onSelected;
  const ScoutSourceNavigation({super.key, this.onSelected});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final selected = ref.watch(scoutSourceFilterProvider);
    final summaries = ref.watch(scoutSourceSummariesProvider);
    final live = ref.watch(scoutSourcesProvider);
    final settings = ref.watch(scoutSettingsProvider).value;
    final names = {
      for (final s in live.value ?? <api.ScoutSourceDto>[]) s.id: s.name,
    };
    final rows = {
      for (final s in summaries.value ?? <ScoutSourceSummary>[]) s.sourceId: s,
    };
    final ids = {...rows.keys, ...names.keys}.toList()..sort();
    void select(String? id, {bool? unreadOnly}) {
      ref.read(scoutSourceFilterProvider.notifier).set(id);
      ref.read(scoutWorkspaceProvider.notifier).set(ScoutWorkspace.discover);
      if (unreadOnly != null) {
        ref.read(scoutUnreadOnlyProvider.notifier).set(unreadOnly);
        ref.read(scoutKindFilterProvider.notifier).set(ScoutKindFilter.all);
      }
      onSelected?.call();
    }

    Widget entry(
      String title,
      IconData icon,
      bool active,
      VoidCallback tap, [
      int? count,
    ]) => ListTile(
      dense: true,
      selected: active,
      leading: Icon(icon, size: 18),
      title: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: count != null && count > 0
          ? Text(
              '$count',
              style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
            )
          : null,
      onTap: tap,
    );
    return Material(
      color: Colors.transparent,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 20, 16, 12),
            child: Text(
              context.l10n.scoutTab,
              style: TextStyle(
                fontSize: 18,
                fontWeight: FontWeight.w600,
                color: AppTheme.textPrimary,
              ),
            ),
          ),
          for (final w in ScoutWorkspace.values)
            entry(
              switch (w) {
                ScoutWorkspace.discover => context.l10n.scoutDiscover,
                ScoutWorkspace.studies => context.l10n.scoutStudies,
                ScoutWorkspace.activity => context.l10n.scoutActivityLog,
              },
              switch (w) {
                ScoutWorkspace.discover => Icons.explore_outlined,
                ScoutWorkspace.studies => Icons.science_outlined,
                ScoutWorkspace.activity => Icons.history,
              },
              ref.watch(scoutWorkspaceProvider) == w,
              () {
                ref.read(scoutWorkspaceProvider.notifier).set(w);
                onSelected?.call();
              },
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
            child: Text(
              context.l10n.scoutSources,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          ),
          Expanded(
            child: ListView(
              children: [
                if (summaries.isLoading) const LinearProgressIndicator(),
                if (summaries.hasError)
                  TextButton(
                    onPressed: () =>
                        ref.invalidate(scoutSourceSummariesProvider),
                    child: Text(context.l10n.scoutRetry),
                  ),
                for (final id in ids)
                  ListTile(
                    dense: true,
                    selected: selected == id,
                    leading: _SourceIcon(
                      source: _sourceIdentity(context, id, names[id]),
                    ),
                    title: Text(
                      _sourceIdentity(context, id, names[id]).platform,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(
                      _sourceIdentity(context, id, names[id]).topic,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11.5,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                    trailing: (rows[id]?.unread ?? 0) > 0
                        ? Text('${rows[id]!.unread}')
                        : null,
                    onTap: () => select(id),
                  ),
              ],
            ),
          ),
          if (live.hasError)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text(
                context.l10n.scoutSourceUnavailable,
                style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
              ),
            ),
          if (settings != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
              child: Text(
                !settings.enabled
                    ? context.l10n.scoutAutoPaused
                    : settings.lastPolledAt == null
                    ? context.l10n.scoutSourceNeverRun
                    : context.l10n.scoutLastPolled(
                        _formatTime(context, settings.lastPolledAt!),
                      ),
                style: TextStyle(color: AppTheme.textTertiary, fontSize: 11.5),
              ),
            ),
        ],
      ),
    );
  }
}

String _sourceName(WidgetRef ref, String id) {
  for (final s
      in ref.watch(scoutSourcesProvider).value ?? <api.ScoutSourceDto>[]) {
    if (s.id == id) return s.name;
  }
  return id;
}
