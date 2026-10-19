part of '../review_view.dart';

class _AnchorDateStepper extends ConsumerWidget {
  const _AnchorDateStepper({required this.wide});

  final bool wide;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final anchor = ref.watch(reviewAnchorDateProvider);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        IconButton(
          visualDensity: VisualDensity.compact,
          tooltip: '前一天',
          onPressed: () => _shift(ref, anchor, -1),
          icon: const Icon(Icons.chevron_left, size: 20),
        ),
        Text(
          anchor,
          style: TextStyle(
            fontSize: 12.5,
            color: AppTheme.textSecondary,
            fontFeatures: const [FontFeature.tabularFigures()],
          ),
        ),
        IconButton(
          visualDensity: VisualDensity.compact,
          tooltip: '后一天',
          onPressed: () => _shift(ref, anchor, 1),
          icon: const Icon(Icons.chevron_right, size: 20),
        ),
      ],
    );
  }

  void _shift(WidgetRef ref, String anchor, int days) {
    final date = DateTime.tryParse(anchor);
    if (date == null) {
      return;
    }
    final next = date.add(Duration(days: days));
    ref
        .read(reviewAnchorDateProvider.notifier)
        .set(
          '${next.year}-'
          '${next.month.toString().padLeft(2, '0')}-'
          '${next.day.toString().padLeft(2, '0')}',
        );
  }
}

// ---------------------------------------------------------------------------
// 日视图
// ---------------------------------------------------------------------------

class _ReviewDayView extends ConsumerWidget {
  const _ReviewDayView({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final date = ref.watch(reviewAnchorDateProvider);
    final overview = ref.watch(dailyOverviewProvider(date));
    return overview.when(
      loading: () =>
          const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('回顾加载失败', style: TextStyle(color: AppTheme.textTertiary)),
            const SizedBox(height: AppTheme.space2),
            TextButton(
              onPressed: () => ref.invalidate(dailyOverviewProvider(date)),
              child: const Text('重试'),
            ),
          ],
        ),
      ),
      data: (data) => ListView(
        padding: EdgeInsets.fromLTRB(
          horizontalPadding,
          12,
          horizontalPadding,
          24,
        ),
        children: [
          _AiReviewCard(review: data.review),
          const SizedBox(height: AppTheme.space3),
          _DayEventsCard(entries: data.entries),
          const SizedBox(height: AppTheme.space3),
          if (data.todos.isNotEmpty) ...[
            _TodoCard(todos: data.todos),
            const SizedBox(height: AppTheme.space3),
          ],
          if (data.reminderHits.isNotEmpty) ...[
            _ReminderCard(hits: data.reminderHits),
            const SizedBox(height: AppTheme.space3),
          ],
          if (data.memoryEcho.isNotEmpty)
            _MemoryEchoCard(echoes: data.memoryEcho),
        ],
      ),
    );
  }
}

/// 卡片基形：标题 + 可选空态文案。零信息块不渲染（历史/只读减负纪律）。
class _ReviewCard extends StatelessWidget {
  const _ReviewCard({required this.title, required this.children});

  final String title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3, width: 1),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            title,
            style: TextStyle(
              fontSize: 11.5,
              fontWeight: FontWeight.w600,
              letterSpacing: 0.4,
              color: AppTheme.textTertiary,
            ),
          ),
          const SizedBox(height: AppTheme.space2),
          ...children,
        ],
      ),
    );
  }
}

/// AI 回顾四组：只会包含有内容的组；全部为空时给当天空态。
class _AiReviewCard extends StatelessWidget {
  const _AiReviewCard({required this.review});

  final api.DailyReviewDto? review;

  // static final：闭包不能进 const 容器（AppTheme getter 也非 const）。
  static final _groups =
      <String, List<api.DailyReviewItemDto> Function(api.DailyReviewDto)>{
        '完成': (r) => r.accomplishments,
        '想法与决定': (r) => r.ideasDecisions,
        '人与事': (r) => r.peopleProjects,
        '后续': (r) => r.followUps,
      };

  @override
  Widget build(BuildContext context) {
    final review = this.review;
    if (review == null) {
      return _ReviewCard(
        title: 'AI 回顾',
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
            child: Text(
              '当天还没有 AI 回顾',
              style: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
            ),
          ),
        ],
      );
    }
    final sections = _groups.entries
        .map((entry) => (label: entry.key, items: entry.value(review)))
        .where((section) => section.items.isNotEmpty)
        .toList();
    return _ReviewCard(
      title: 'AI 回顾',
      children: [
        for (final section in sections) ...[
          Padding(
            padding: const EdgeInsets.only(top: AppTheme.space1, bottom: 2),
            child: Text(
              section.label,
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: AppTheme.textSecondary,
              ),
            ),
          ),
          for (final item in section.items)
            Padding(
              padding: const EdgeInsets.only(left: AppTheme.space2, bottom: 2),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '·',
                    style: TextStyle(color: AppTheme.textTertiary, height: 1.5),
                  ),
                  const SizedBox(width: AppTheme.space2),
                  Expanded(
                    child: Text(
                      item.text,
                      style: TextStyle(
                        fontSize: 13,
                        color: AppTheme.textPrimary,
                        height: 1.5,
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ],
    );
  }
}

/// 当日事件明细区：摘要行可展开原文。这是原事件卡流的降级形态——
/// 单日的读面，不再承接完整的浏览/详情工作区。
class _DayEventsCard extends StatelessWidget {
  const _DayEventsCard({required this.entries});

  final List<api.DailyEntryDto> entries;

  @override
  Widget build(BuildContext context) {
    if (entries.isEmpty) {
      // AppTheme 色是 getter 非 const——卡片树不可 const。
      return _ReviewCard(
        title: '当日事件',
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
            child: Text(
              '当天没有记录事件',
              style: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
            ),
          ),
        ],
      );
    }
    return _ReviewCard(
      title: '当日事件',
      children: [
        for (final entry in entries)
          // 卡片 Container 有背景色，会隔断 ListTile 的墨水涟漪——
          // ExpansionTile 需要紧邻的 Material 祖先，就地给一层透明的。
          Material(
            type: MaterialType.transparency,
            child: Theme(
              data: Theme.of(context)
                  .copyWith(dividerColor: Colors.transparent),
              child: ExpansionTile(
                tilePadding: EdgeInsets.zero,
                childrenPadding: const EdgeInsets.only(
                  left: AppTheme.space3,
                  bottom: AppTheme.space2,
                ),
                iconColor: AppTheme.textTertiary,
                collapsedIconColor: AppTheme.textTertiary,
                title: Text(
                  entry.rawText,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
                ),
                subtitle: Text(
                  '${entry.recordedAt.length >= 16 ? entry.recordedAt.substring(11, 16) : entry.recordedAt}'
                  ' · ${entry.source}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
                children: [
                  Align(
                    alignment: AlignmentDirectional.centerStart,
                    child: Text(
                      entry.rawText,
                      style: TextStyle(
                        fontSize: 12.5,
                        color: AppTheme.textSecondary,
                        height: 1.5,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

class _TodoCard extends StatelessWidget {
  const _TodoCard({required this.todos});

  final List<api.TodoDto> todos;

  @override
  Widget build(BuildContext context) {
    return _ReviewCard(
      title: '待办',
      children: [
        for (final todo in todos)
          Padding(
            padding: const EdgeInsets.only(bottom: AppTheme.space1),
            child: Row(
              children: [
                Icon(
                  todo.status == 'done'
                      ? Icons.check_circle_outline
                      : Icons.radio_button_unchecked,
                  size: 14,
                  color: todo.status == 'done'
                      ? AppTheme.success
                      : AppTheme.textTertiary,
                ),
                const SizedBox(width: AppTheme.space2),
                Expanded(
                  child: Text(
                    todo.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: todo.status == 'done'
                          ? AppTheme.textTertiary
                          : AppTheme.textPrimary,
                      decoration: todo.status == 'done'
                          ? TextDecoration.lineThrough
                          : null,
                    ),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// 规则提醒只呈现（含已有反馈标记），反馈操作仍归事件详情域，不重复。
class _ReminderCard extends StatelessWidget {
  const _ReminderCard({required this.hits});

  final List<api.RuleHitDto> hits;

  static const _feedbackLabels = {'helpful': '有用', 'not_relevant': '不相关'};

  @override
  Widget build(BuildContext context) {
    return _ReviewCard(
      title: '规则提醒',
      children: [
        for (final hit in hits)
          Padding(
            padding: const EdgeInsets.only(bottom: AppTheme.space2),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  Icons.notifications_none,
                  size: 14,
                  color: AppTheme.warning,
                ),
                const SizedBox(width: AppTheme.space2),
                Expanded(
                  child: Text(
                    hit.ruleContent,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: AppTheme.textPrimary,
                      height: 1.4,
                    ),
                  ),
                ),
                if (hit.feedback != null) ...[
                  const SizedBox(width: AppTheme.space2),
                  Text(
                    _feedbackLabels[hit.feedback] ?? hit.feedback!,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ],
            ),
          ),
      ],
    );
  }
}

/// 共同记忆回引：约一周前的条目原文快照，呈现不评价。
class _MemoryEchoCard extends StatelessWidget {
  const _MemoryEchoCard({required this.echoes});

  final List<api.MemoryEchoDto> echoes;

  @override
  Widget build(BuildContext context) {
    return _ReviewCard(
      title: '上周的今天',
      children: [
        for (final echo in echoes)
          Padding(
            padding: const EdgeInsets.only(bottom: AppTheme.space2),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  echo.recordedAt.length >= 10
                      ? echo.recordedAt.substring(0, 10)
                      : echo.recordedAt,
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
                const SizedBox(height: 2),
                Text(
                  echo.rawText,
                  style: TextStyle(
                    fontSize: 12.5,
                    color: AppTheme.textSecondary,
                    height: 1.5,
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// 周/月视图
// ---------------------------------------------------------------------------

/// 该粒度是否有未读报告（报告面加载失败或未就绪 = 无提醒，静默降级）。
bool _hasUnread(WidgetRef ref, String periodType) {
  final reports = ref.watch(deviationReportsProvider(periodType));
  return reports.value?.any((r) => r.readAt == null) ?? false;
}

/// 未读小圆点（R4 唯一提醒形态）：不弹窗、不锁屏，切换处一个点。
class _UnreadBadge extends StatelessWidget {
  const _UnreadBadge({required this.unread, required this.child});

  final bool unread;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (!unread) return child;
    return Badge(
      backgroundColor: AppTheme.accentPrimary,
      smallSize: 7,
      child: child,
    );
  }
}

/// 周期盘点报告（R4）：最新一份完整铺开，更早的收进「更早的盘点」
/// ExpansionTile 按周期回看。加载失败静默（统计页不受报告面影响）。
/// 首次铺开时自动标记已读（看到即读，幂等）。
