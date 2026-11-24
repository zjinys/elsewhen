part of '../scout_view.dart';

class ScoutFindingsPane extends ConsumerWidget {
  const ScoutFindingsPane({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scale = chatTypeScaleOf(ref.watch(settingsProvider));
    final kind = ref.watch(scoutKindFilterProvider);
    final unread = ref.watch(scoutUnreadOnlyProvider);
    final source = ref.watch(scoutSourceFilterProvider);
    final state = ref.watch(scoutFindingsProvider);
    return LayoutBuilder(
      builder: (context, constraints) {
        final columns = ((constraints.maxWidth - 40) / 310).floor().clamp(1, 4);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 18, 12, 8),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      source == null
                          ? context.l10n.scoutLatest
                          : scoutSourceDisplayName(ref, source),
                      style: TextStyle(
                        fontSize: scale.panel + 3,
                        fontWeight: FontWeight.w600,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 12),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  for (final f in ScoutKindFilter.values)
                    ChoiceChip(
                      label: Text(
                        f == ScoutKindFilter.all
                            ? context.l10n.scoutFilterAll
                            : _kindLabel(context, switch (f) {
                                ScoutKindFilter.new_ => 'new',
                                ScoutKindFilter.changed => 'changed',
                                _ => 'gone',
                              }),
                        style: TextStyle(fontSize: scale.chip),
                      ),
                      selected: kind == f,
                      onSelected: (_) =>
                          ref.read(scoutKindFilterProvider.notifier).set(f),
                      visualDensity: VisualDensity.compact,
                    ),
                  FilterChip(
                    label: Text(context.l10n.scoutFilterUnread),
                    selected: unread,
                    onSelected: (v) =>
                        ref.read(scoutUnreadOnlyProvider.notifier).set(v),
                    visualDensity: VisualDensity.compact,
                  ),
                  TextButton(
                    onPressed:
                        (state.value?.items.any((i) => i.isUnread) ?? false)
                        ? () => _markVisible(context, ref)
                        : null,
                    child: Text(
                      context.l10n.scoutReadVisible,
                      style: TextStyle(fontSize: scale.chip),
                    ),
                  ),
                ],
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: state.when(
                loading: () => const Center(child: CircularProgressIndicator()),
                error: (e, _) => Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(context.errorMessage(context.l10n.scoutFindings, e)),
                      TextButton(
                        onPressed: () => ref.invalidate(scoutFindingsProvider),
                        child: Text(context.l10n.scoutRetry),
                      ),
                    ],
                  ),
                ),
                data: (s) => s.isEmpty
                    ? _ScoutMessage(text: context.l10n.scoutEmpty)
                    : ContentFontScope(
                        child: CustomScrollView(
                          key: PageStorageKey(
                            'scout-grid-$source-$kind-$unread',
                          ),
                          slivers: [
                            for (final day in _groupDays(s.items).entries) ...[
                              SliverPadding(
                                padding: const EdgeInsets.fromLTRB(
                                  20,
                                  18,
                                  20,
                                  10,
                                ),
                                sliver: SliverToBoxAdapter(
                                  child: Text(
                                    day.key,
                                    style: TextStyle(
                                      fontSize: scale.meta,
                                      color: AppTheme.textTertiary,
                                    ),
                                  ),
                                ),
                              ),
                              SliverPadding(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 20,
                                ),
                                sliver: SliverGrid(
                                  gridDelegate:
                                      SliverGridDelegateWithFixedCrossAxisCount(
                                        crossAxisCount: columns,
                                        crossAxisSpacing: 14,
                                        mainAxisSpacing: 14,
                                        mainAxisExtent:
                                            250 +
                                            (scale.base - 15).clamp(0, 20) *
                                                10 +
                                            (MediaQuery.textScalerOf(context)
                                                            .scale(15) -
                                                        15)
                                                    .clamp(0, 40) *
                                                12,
                                      ),
                                  delegate: SliverChildBuilderDelegate(
                                    (context, index) => _FindingCard(
                                      item: day.value[index],
                                      scale: scale,
                                    ),
                                    childCount: day.value.length,
                                  ),
                                ),
                              ),
                            ],
                            SliverToBoxAdapter(
                              child: Padding(
                                padding: const EdgeInsets.all(20),
                                child: Column(
                                  children: [
                                    if (s.error != null)
                                      Text(context.l10n.scoutActionFailed),
                                    if (s.hasMore)
                                      TextButton(
                                        onPressed: s.loadingMore
                                            ? null
                                            : () => ref
                                                  .read(
                                                    scoutFindingsProvider
                                                        .notifier,
                                                  )
                                                  .loadMore(),
                                        child: Text(
                                          s.loadingMore
                                              ? '…'
                                              : context.l10n.scoutLoadMore,
                                        ),
                                      ),
                                  ],
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
              ),
            ),
          ],
        );
      },
    );
  }

  Future<void> _markVisible(BuildContext context, WidgetRef ref) async {
    final ids =
        ref
            .read(scoutFindingsProvider)
            .value
            ?.items
            .where((i) => i.isUnread)
            .map((i) => i.id)
            .toList() ??
        [];
    if (ids.isEmpty) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(context.l10n.scoutAllReadConfirmTitle),
        content: Text(
          '${context.l10n.scoutAllReadConfirmBody(ids.length)}\n${context.l10n.scoutReadVisibleHelp}',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: Text(context.l10n.confirm),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) return;
    try {
      await ref.read(scoutRepositoryProvider).markRead(ref, ids);
    } catch (e) {
      if (context.mounted) {
        FeedbackToast.of(context).show(
          SnackBar(
            content: Text(context.errorMessage(context.l10n.scoutMarkRead, e)),
          ),
        );
      }
    }
  }
}

String _day(String iso) {
  final t = DateTime.tryParse(iso)?.toLocal();
  return t == null ? iso : DateFormat('yyyy-MM-dd').format(t);
}

Map<String, List<ScoutFindingItem>> _groupDays(List<ScoutFindingItem> items) {
  final days = <String, List<ScoutFindingItem>>{};
  for (final item in items) {
    (days[_day(item.detectedAt)] ??= []).add(item);
  }
  return days;
}

class _FindingCard extends ConsumerWidget {
  final ScoutFindingItem item;
  final ChatTypeScale scale;
  const _FindingCard({required this.item, required this.scale});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Card(
    key: ValueKey('scout-card-${item.id}'),
    margin: EdgeInsets.zero,
    clipBehavior: Clip.antiAlias,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(12),
      side: BorderSide(color: AppTheme.surface3),
    ),
    color: AppTheme.surface1,
    child: InkWell(
      onTap: () {
        _showFindingSheet(context, item);
        if (item.isUnread) {
          ref.read(scoutRepositoryProvider).markRead(ref, [item.id]).catchError(
            (Object e) {
              if (context.mounted) {
                FeedbackToast.of(context).show(
                  SnackBar(
                    content: Text(
                      context.errorMessage(context.l10n.scoutMarkRead, e),
                    ),
                  ),
                );
              }
            },
          );
        }
      },
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(child: ScoutSourceBadge(sourceId: item.sourceId)),
                if (item.isUnread)
                  Tooltip(
                    message: context.l10n.scoutFilterUnread,
                    child: Icon(
                      Icons.circle,
                      size: 7,
                      color: AppTheme.accentPrimary,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 14),
            Text(
              item.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: scale.bubble,
                height: 1.35,
                fontWeight: FontWeight.w600,
                color: AppTheme.textPrimary,
              ),
            ),
            const SizedBox(height: 9),
            Text(
              _changeSummary(context, item),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: scale.bubbleSecondary,
                height: 1.4,
                color: item.kind == 'changed'
                    ? AppTheme.accentPrimary
                    : AppTheme.textSecondary,
              ),
            ),
            if (item.subtitle != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(
                  item.subtitle!,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: scale.meta,
                    height: 1.5,
                    color: AppTheme.textSecondary,
                  ),
                ),
              ),
            const Spacer(),
            Row(
              children: [
                Text(
                  _kindLabel(context, item.kind),
                  style: TextStyle(
                    fontSize: scale.chip,
                    color: _kindColor(item.kind),
                  ),
                ),
                const Spacer(),
                Text(
                  _formatTime(context, item.detectedAt),
                  style: TextStyle(
                    fontSize: scale.meta,
                    color: AppTheme.textTertiary,
                  ),
                ),
                const SizedBox(width: 6),
                Icon(
                  Icons.chevron_right,
                  size: DesignTokens.iconMicro,
                  color: AppTheme.textTertiary,
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

Future<void> _showFindingSheet(BuildContext context, ScoutFindingItem item) {
  final reduceMotion = MediaQuery.disableAnimationsOf(context);
  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    barrierColor: Colors.black.withValues(alpha: .35),
    transitionDuration: Duration(milliseconds: reduceMotion ? 0 : 220),
    transitionBuilder: (context, animation, secondaryAnimation, child) =>
        SlideTransition(
          position: Tween<Offset>(begin: const Offset(1, 0), end: Offset.zero)
              .animate(
                CurvedAnimation(parent: animation, curve: Curves.easeOutCubic),
              ),
          child: child,
        ),
    pageBuilder: (sheetContext, _, secondary) => Align(
      alignment: Alignment.centerRight,
      child: SizedBox(
        width: MediaQuery.sizeOf(sheetContext).width.clamp(0, 620),
        height: double.infinity,
        child: Material(
          key: const ValueKey('scout-detail-sheet'),
          elevation: 16,
          color: AppTheme.surface1,
          child: SafeArea(
            child: ScoutFindingDetail(
              item: item,
              onClose: () => Navigator.of(sheetContext).pop(),
            ),
          ),
        ),
      ),
    ),
  );
}

String _changeSummary(BuildContext context, ScoutFindingItem item) {
  if (item.kind != 'changed' || item.delta.isEmpty) {
    return _kindLabel(context, item.kind);
  }
  return item.delta.entries
      .take(2)
      .map((e) {
        final d = e.value;
        return d is Map
            ? '${_fieldLabel(context, e.key)} ${_value(d['from'])} → ${_value(d['to'])}'
            : _fieldLabel(context, e.key);
      })
      .join(' · ');
}

String _value(dynamic value) => value == null
    ? '—'
    : value is String
    ? value
    : jsonEncode(value);
String _fieldLabel(BuildContext context, String key) => switch (key) {
  'stargazers_count' => context.l10n.scoutMetricStars,
  'forks_count' => context.l10n.scoutMetricForks,
  'rating_count' || 'ratings_count' => context.l10n.scoutMetricReviews,
  'rating' || 'ratings_average' => context.l10n.scoutMetricRating,
  'rank' => context.l10n.scoutMetricRank,
  'price_cents' => context.l10n.scoutMetricPrice,
  'open_issues_count' => 'Issues',
  _ => key,
};
