part of '../left_sidebar.dart';

//! 空态/回顾导航/拒绝来源折叠

class _EmptyState extends StatelessWidget {
  final IconData icon;
  final String message;

  const _EmptyState({required this.icon, required this.message});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space6),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 48, color: AppTheme.textTertiary),
            const SizedBox(height: AppTheme.space3),
            Text(
              message,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 13,
                height: 1.6,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 待办 tab：手写待办视图（列表按进行中/已完成分组）

/// 回顾页左栏：近 7 天日期入口（含今天）。点击即切换锚点日期到该日并
/// 落到日粒度——是与日视图最自然的联动，周/月切换在主工作区顶部完成。
class _ReviewNavSummary extends ConsumerWidget {
  const _ReviewNavSummary();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final anchor = ref.watch(reviewAnchorDateProvider);
    final today = DateTime.now();
    final days = List<DateTime>.generate(
      7,
      (i) => today.subtract(Duration(days: i)),
    );
    return ListView(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space2),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
          child: Text(
            '近 7 天',
            style: TextStyle(
              fontSize: 11.5,
              fontWeight: FontWeight.w600,
              letterSpacing: 0.4,
              color: AppTheme.textTertiary,
            ),
          ),
        ),
        for (final day in days)
          Builder(
            builder: (context) {
              final key =
                  '${day.year}-${day.month.toString().padLeft(2, '0')}-${day.day.toString().padLeft(2, '0')}';
              final selected = key == anchor;
              return ListTile(
                dense: true,
                selected: selected,
                selectedTileColor: AppTheme.surface2,
                title: Text(
                  key ==
                          '${today.year}-${today.month.toString().padLeft(2, '0')}-${today.day.toString().padLeft(2, '0')}'
                      ? '今天'
                      : key,
                  style: TextStyle(
                    fontSize: 13,
                    color: selected
                        ? AppTheme.textPrimary
                        : AppTheme.textSecondary,
                    fontFeatures: const [FontFeature.tabularFigures()],
                  ),
                ),
                onTap: () {
                  ref.read(reviewAnchorDateProvider.notifier).set(key);
                  ref
                      .read(reviewGranularityProvider.notifier)
                      .set(ReviewGranularity.day);
                },
              );
            },
          ),
      ],
    );
  }
}

/// 「已否定」折叠区（负知识一等公民，提案 2026-10-19）：身份 + 否定原因
/// 只读列表，懒加载（展开才查询）。与孵化分组同色系，不放主列表。
class _RejectedSourcesTile extends ConsumerStatefulWidget {
  const _RejectedSourcesTile();

  @override
  ConsumerState<_RejectedSourcesTile> createState() =>
      _RejectedSourcesTileState();
}

class _RejectedSourcesTileState extends ConsumerState<_RejectedSourcesTile> {
  Future<List<api.RejectedSourceRow>>? _future;

  @override
  Widget build(BuildContext context) {
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
        childrenPadding: const EdgeInsets.only(
          left: AppTheme.space4,
          right: AppTheme.space2,
          bottom: AppTheme.space1,
        ),
        iconColor: AppTheme.textTertiary,
        collapsedIconColor: AppTheme.textTertiary,
        title: Row(
          children: [
            Icon(Icons.block_outlined, size: 13, color: AppTheme.textTertiary),
            const SizedBox(width: 6),
            Text(
              '已否定（负知识）',
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w600,
                color: AppTheme.textTertiary,
              ),
            ),
          ],
        ),
        onExpansionChanged: (open) {
          if (open && _future == null) {
            setState(() {
              _future = ref
                  .read(knowledgeRepositoryProvider)
                  .rejectedSources();
            });
          }
        },
        children: [
          FutureBuilder<List<api.RejectedSourceRow>>(
            future: _future,
            builder: (context, snap) {
              if (snap.connectionState != ConnectionState.done) {
                return const Padding(
                  padding: EdgeInsets.all(AppTheme.space2),
                  child: SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                );
              }
              final rows = snap.data ?? const [];
              if (rows.isEmpty) {
                return Padding(
                  padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
                  child: Text(
                    '还没有否定的来源。',
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                );
              }
              return Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final row in rows)
                    Padding(
                      padding: const EdgeInsets.only(bottom: AppTheme.space1),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            row.title ?? row.identity,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                          if (row.rejectReason.isNotEmpty)
                            Text(
                              row.rejectReason,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 11,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                        ],
                      ),
                    ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

/// 目标页左栏：活跃目标导航摘要（只读）。
///
/// 展示最多 3 条活跃目标的愿景名与当前抓手切片，作为进入目标域后的
/// 「正在推进什么」速览；管理动作（滚动抓手/达成/暂停/唤醒）不在这里，
/// 与主区只读总览同一纪律：写操作以对话流/弹窗面板为单一入口。
/// 点击一条目标暂无二级详情页，不做无动作的假入口。
class _GoalNavSummary extends ConsumerWidget {
  const _GoalNavSummary();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final activeAsync = ref.watch(activeGoalsProvider);
    return activeAsync.when(
      loading: () => const SizedBox.expand(),
      error: (e, _) => Center(
        child: Text(
          '目标加载失败',
          style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
        ),
      ),
      data: (goals) {
        if (goals.isEmpty) {
          return const _EmptyState(
            icon: Icons.flag_outlined,
            message: '还没有设定目标\n在对话面板中设立 1~3 个长程愿景',
          );
        }
        return ListView(
          padding: const EdgeInsets.symmetric(vertical: AppTheme.space2),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
              child: Text(
                '活跃目标 ${goals.length} / 3',
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  letterSpacing: 0.4,
                  color: AppTheme.textTertiary,
                ),
              ),
            ),
            for (final Goal goal in goals) _GoalNavCard(goal: goal),
          ],
        );
      },
    );
  }
}

/// 单条活跃目标的左栏摘要卡片。
class _GoalNavCard extends StatelessWidget {
  const _GoalNavCard({required this.goal});

  final Goal goal;

  @override
  Widget build(BuildContext context) {
    final focus = goal.currentFocus?.trim() ?? '';
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8),
      child: Container(
        margin: const EdgeInsets.only(bottom: 6),
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          border: Border.all(color: AppTheme.surface3.withValues(alpha: 0.6)),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(
                  goal.phase.label,
                  style: TextStyle(
                    fontSize: 10.5,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.accentPrimary,
                  ),
                ),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    goal.content,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
              ],
            ),
            if (focus.isNotEmpty) ...[
              const SizedBox(height: 3),
              Text(
                '📍 $focus',
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
