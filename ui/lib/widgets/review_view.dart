import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../providers/knowledge_provider.dart';
import '../providers/review_provider.dart';
import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';

/// 回顾页（R1 第一刀）：主工作区的回顾读面。
///
/// 三粒度（日/周/月）共用一个锚点日期：
/// - 日视图 = `get_daily_overview` 五块（AI 回顾、当日事件、待办、
///   规则提醒、记忆回引）+ 当日事件明细区（原文内联展开）；
/// - 周/月视图 = 本地 SQL 实时拼装统计（Rust `get_period_stats`），
///   无 AI、不落表；空数据有明确空态。
///
/// 旧「事件」域不再作为独立工作区：入口在本页左栏 footer「回顾」。
///
/// 禁 pumpAndSettle（门禁约束）；宽窄屏同一纵向流，仅边距随宽收放。
class ReviewView extends ConsumerWidget {
  const ReviewView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final granularity = ref.watch(reviewGranularityProvider);
    // 断点走令牌：平板/小窗（≥600）加宽边距；桌面（>1024）再给足呼吸感。
    final width = MediaQuery.sizeOf(context).width;
    final wide = width >= Breakpoints.phone;
    final horizontalPadding = width >= Breakpoints.desktop
        ? AppTheme.space12
        : wide
            ? AppTheme.space8
            : AppTheme.space3;
    // 粒度切换处的未读小圆点（R4 唯一提醒形态）：任一该粒度报告
    // readAt 为空 = 未读；报告面尚未加载时不显示（静默降级）。
    final weekUnread = _hasUnread(ref, 'week');
    final monthUnread = _hasUnread(ref, 'month');
    return Column(
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(
            horizontalPadding,
            12,
            horizontalPadding,
            0,
          ),
          child: Row(
            children: [
              SegmentedButton<ReviewGranularity>(
                segments: [
                  ButtonSegment(
                    value: ReviewGranularity.day,
                    label: const Text('日'),
                  ),
                  ButtonSegment(
                    value: ReviewGranularity.week,
                    label: _UnreadBadge(unread: weekUnread, child: const Text('周')),
                  ),
                  ButtonSegment(
                    value: ReviewGranularity.month,
                    label: _UnreadBadge(
                      unread: monthUnread,
                      child: const Text('月'),
                    ),
                  ),
                ],
                selected: {granularity},
                onSelectionChanged: (selection) {
                  ref
                      .read(reviewGranularityProvider.notifier)
                      .set(selection.first);
                },
              ),
              const Spacer(),
              _AnchorDateStepper(wide: wide),
            ],
          ),
        ),
        KnowledgeHealthBar(horizontalPadding: horizontalPadding),
        Expanded(
          child: switch (granularity) {
            ReviewGranularity.day => _ReviewDayView(
              horizontalPadding: horizontalPadding,
            ),
            ReviewGranularity.week => _ReviewPeriodView(
              periodType: 'week',
              horizontalPadding: horizontalPadding,
            ),
            ReviewGranularity.month => _ReviewPeriodView(
              periodType: 'month',
              horizontalPadding: horizontalPadding,
            ),
          },
        ),
      ],
    );
  }
}

/// 锚点日期翻页（日粒度 ±1 天，周/月粒度 ±7/±30 天粗步进）。
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
class _PeriodReportSection extends ConsumerStatefulWidget {
  const _PeriodReportSection({required this.periodType});

  final String periodType;

  @override
  ConsumerState<_PeriodReportSection> createState() =>
      _PeriodReportSectionState();
}

class _PeriodReportSectionState extends ConsumerState<_PeriodReportSection> {
  bool _marked = false;

  @override
  void initState() {
    super.initState();
    Future.microtask(() => _markUnreadSeen());
  }

  /// 看到报告即已读：逐份标记未读报告（幂等），标记后刷读面消点。
  Future<void> _markUnreadSeen() async {
    if (_marked || !mounted) return;
    _marked = true; // 每个 mount 只发一轮，防 invalidate 重拉后重复标记。
    final reports = ref.read(deviationReportsProvider(widget.periodType)).value;
    final unread = reports?.where((r) => r.readAt == null).toList() ?? const [];
    if (unread.isEmpty || !mounted) return;
    try {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      for (final report in unread) {
        await bridge.markDeviationReportRead(report.id);
      }
      if (!mounted) return;
      ref.invalidate(deviationReportsProvider(widget.periodType));
    } catch (_) {
      // 已读标记失败不响：下个 tick 重开页时未读仍在，再标一次即可。
    }
  }

  @override
  Widget build(BuildContext context) {
    final reports = ref.watch(deviationReportsProvider(widget.periodType));
    return reports.when(
      loading: () => const SizedBox.shrink(),
      error: (_, _) => const SizedBox.shrink(),
      data: (list) {
        if (list.isEmpty) return const SizedBox.shrink();
        final latest = list.first;
        final older = list.sublist(1);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _PeriodReportCard(report: latest, latest: true),
            if (older.isNotEmpty)
              Theme(
                data: Theme.of(
                  context,
                ).copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  childrenPadding: EdgeInsets.zero,
                  iconColor: AppTheme.textTertiary,
                  collapsedIconColor: AppTheme.textTertiary,
                  title: Text(
                    '更早的盘点（${older.length}）',
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  children: [
                    for (final report in older)
                      Padding(
                        padding: const EdgeInsets.only(top: AppTheme.space2),
                        child: _PeriodReportCard(report: report),
                      ),
                  ],
                ),
              ),
          ],
        );
      },
    );
  }
}

/// 单份盘点报告：统计摘要 + 偏差条目（历史报告复用同一张卡）。
class _PeriodReportCard extends ConsumerWidget {
  const _PeriodReportCard({required this.report, this.latest = false});

  final api.DeviationReportDto report;
  final bool latest;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final unDismissed = report.deviations
        .where((d) => !d.dismissed)
        .length;
    return _ReviewCard(
      title:
          '周期盘点 · ${report.startDate} ~ ${report.endDate}'
          '${latest ? '（最近一份）' : ''}',
      children: [
        Row(
          children: [
            _CountPill(label: '事件', count: report.total),
            const SizedBox(width: AppTheme.space3),
            _CountPill(label: '待办完成', count: report.todoDoneCount),
            const Spacer(),
            if (report.detectedAt == null)
              Text(
                '偏差检测待进行',
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
          ],
        ),
        if (report.detectedAt != null && report.deviations.isEmpty) ...[
          const SizedBox(height: AppTheme.space2),
          Text(
            '未发现待说出的偏差',
            style: TextStyle(
              fontSize: 12,
              color: AppTheme.textTertiary,
              height: 1.5,
            ),
          ),
        ],
        if (report.deviations.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space2),
          if (unDismissed > 0)
            Text(
              '偏离目标的时刻——只陈述，不评判：',
              style: TextStyle(
                fontSize: 11,
                color: AppTheme.textTertiary,
                height: 1.5,
              ),
            ),
          for (final (index, deviation) in report.deviations.indexed)
            _DeviationRow(
              reportId: report.id,
              periodType: report.periodType,
              index: index,
              deviation: deviation,
            ),
        ],
      ],
    );
  }
}

/// 偏差条目：类型中文 + 严重度色点 + 证据 + 目标；忽略置位不删除。
class _DeviationRow extends ConsumerWidget {
  const _DeviationRow({
    required this.reportId,
    required this.periodType,
    required this.index,
    required this.deviation,
  });

  final String reportId;
  final String periodType;
  final int index;
  final api.DeviationItemDto deviation;

  Color _severityColor() => switch (deviation.severity) {
        'high' => AppTheme.error,
        'medium' => AppTheme.warning,
        _ => AppTheme.accentMuted,
      };

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final baseStyle = TextStyle(
      fontSize: 12,
      height: 1.55,
      color: deviation.dismissed
          ? AppTheme.textTertiary
          : AppTheme.textSecondary,
    );
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space2),
      child: Container(
        padding: const EdgeInsets.all(AppTheme.space2),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 5),
              child: Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(
                  color: deviation.dismissed
                      ? AppTheme.surface4
                      : _severityColor(),
                  shape: BoxShape.circle,
                ),
              ),
            ),
            const SizedBox(width: AppTheme.space2),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    deviation.dismissed
                        ? '${deviationTypeLabel(deviation.deviationType)} · 已忽略'
                        : deviationTypeLabel(deviation.deviationType),
                    style: baseStyle.copyWith(
                      fontWeight: FontWeight.w600,
                      color: deviation.dismissed
                          ? AppTheme.textTertiary
                          : AppTheme.textPrimary,
                    ),
                  ),
                  Text(deviation.evidence, style: baseStyle),
                  Text(
                    '目标：${deviation.goalContent}',
                    style: baseStyle.copyWith(fontSize: 11),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
            if (!deviation.dismissed)
              TextButton(
                onPressed: () async {
                  try {
                    final bridge = ref.read(storageRepositoryProvider)
                        as RustBridgeRepository;
                    await bridge.dismissDeviation(reportId, index);
                    ref.invalidate(
                      deviationReportsProvider(periodType),
                    );
                  } catch (_) {
                    // 忽略失败静默，条目保持可再次操作。
                  }
                },
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                ),
                child: Text(
                  '忽略',
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// 周/月拼装统计。Rust 侧已做确定性排序与 top 12 截断；这里只拼版。
class _ReviewPeriodView extends ConsumerWidget {
  const _ReviewPeriodView({
    required this.periodType,
    required this.horizontalPadding,
  });

  final String periodType;
  final double horizontalPadding;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final anchor = ref.watch(reviewAnchorDateProvider);
    final stats = ref.watch(periodStatsProvider((periodType, anchor)));
    return stats.when(
      loading: () =>
          const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Text('统计加载失败', style: TextStyle(color: AppTheme.textTertiary)),
          const SizedBox(height: AppTheme.space2),
          TextButton(
            onPressed: () =>
                ref.invalidate(periodStatsProvider((periodType, anchor))),
            child: const Text('重试'),
          ),
        ],
      ),
      data: (data) {
        final empty = data.eventCount == 0 && data.todoDoneCount == 0;
        return ListView(
          padding: EdgeInsets.fromLTRB(
            horizontalPadding,
            12,
            horizontalPadding,
            24,
          ),
          children: [
            _PeriodReportSection(periodType: periodType),
            Text(
              '${data.startDate} ~ ${data.endDate}',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: AppTheme.space2),
            if (empty)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 48),
                child: Text(
                  periodType == 'week' ? '本周没有记录' : '本月没有记录',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 12.5,
                    height: 1.6,
                    color: AppTheme.textTertiary,
                  ),
                ),
              )
            else ...[
              Row(
                children: [
                  _CountPill(label: '事件', count: data.eventCount),
                  const SizedBox(width: AppTheme.space3),
                  _CountPill(label: '待办完成', count: data.todoDoneCount),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              _DailyBars(counts: data.dailyCounts),
              for (final (title, rows) in <(String, List<api.NamedCountDto>)>[
                ('事类', data.byEventType),
                ('人物', data.byPerson),
                ('项目', data.byProject),
                ('活动', data.byActivity),
              ])
                if (rows.isNotEmpty) ...[
                  const SizedBox(height: AppTheme.space3),
                  _ReviewCard(
                    title: title,
                    children: [
                      for (final row in rows)
                        Padding(
                          padding: const EdgeInsets.only(
                            bottom: AppTheme.space1,
                          ),
                          child: Row(
                            children: [
                              Expanded(
                                child: Text(
                                  row.label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 12.5,
                                    color: AppTheme.textPrimary,
                                  ),
                                ),
                              ),
                              Text(
                                '${row.count}',
                                style: TextStyle(
                                  fontSize: 12.5,
                                  color: AppTheme.textTertiary,
                                  fontFeatures: const [
                                    FontFeature.tabularFigures(),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ],
            ],
          ],
        );
      },
    );
  }
}

class _CountPill extends StatelessWidget {
  const _CountPill({required this.label, required this.count});

  final String label;
  final int count;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space3,
        vertical: AppTheme.space2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        border: Border.all(color: AppTheme.surface3, width: 1),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            '$count',
            style: TextStyle(
              fontSize: 16,
              fontWeight: FontWeight.w600,
              color: AppTheme.textPrimary,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
          const SizedBox(width: AppTheme.space1),
          Text(
            label,
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
        ],
      ),
    );
  }
}

/// 日分布微条形：纯 Container 宽度比例，不引图表库。
class _DailyBars extends StatelessWidget {
  const _DailyBars({required this.counts});

  final List<api.DayCountDto> counts;

  @override
  Widget build(BuildContext context) {
    final maxCount = counts.fold<int>(0, (m, d) => d.count > m ? d.count : m);
    if (maxCount == 0) {
      return const SizedBox.shrink();
    }
    return Container(
      padding: const EdgeInsets.all(AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3, width: 1),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (final day in counts)
            Expanded(
              child: Tooltip(
                message: '${day.date} · ${day.count}',
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 1.5),
                  child: SizedBox(
                    height: 36,
                    child: Align(
                      alignment: Alignment.bottomCenter,
                      child: FractionallySizedBox(
                        widthFactor: 0.6,
                        heightFactor: day.count == 0 ? 0 : day.count / maxCount,
                        child: Container(
                          decoration: BoxDecoration(
                            color: day.count == 0
                                ? AppTheme.surface3
                                : AppTheme.accentPrimary,
                            borderRadius: const BorderRadius.vertical(
                              top: Radius.circular(2),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// 知识管道健康（P0-a「可见性」的用户面，2026-10-20）
// ---------------------------------------------------------------------------

/// 回顾页底部常驻健康条：一行摘要 + 点击展开详细（含摘要预热账本）。
/// 数据失败静默降级为不渲染——健康巡检本身不应制造噪声。
class KnowledgeHealthBar extends ConsumerStatefulWidget {
  final double horizontalPadding;
  const KnowledgeHealthBar({super.key, required this.horizontalPadding});

  @override
  ConsumerState<KnowledgeHealthBar> createState() => _KnowledgeHealthBarState();
}

class _KnowledgeHealthBarState extends ConsumerState<KnowledgeHealthBar> {
  Future<api.KnowledgeHealthReport>? _future;

  @override
  void initState() {
    super.initState();
    try {
      _future = ref
          .read(knowledgeRepositoryProvider)
          .health(daysLookback: 30);
    } catch (_) {
      // 桥未就绪（如 widget 测试环境）或调用失败时静默降级，避免阻塞主视图
      _future = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_future == null) return const SizedBox.shrink();
    return FutureBuilder<api.KnowledgeHealthReport>(
      future: _future,
      builder: (context, snap) {
        if (snap.hasError) return const SizedBox.shrink();
        final h = snap.data;
        if (h == null) return const SizedBox.shrink();
        final summaryBroken = h.summaryUnavailableRecent > 0;
        return Padding(
          padding: EdgeInsets.fromLTRB(widget.horizontalPadding, 4, widget.horizontalPadding, 4),
          child: InkWell(
            borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
            onTap: () => _showDetails(h),
            child: Container(
              padding: const EdgeInsets.symmetric(
                horizontal: AppTheme.space3,
                vertical: 6,
              ),
              decoration: BoxDecoration(
                color: AppTheme.surface1,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(
                  color: summaryBroken ? const Color(0xFFF7768E) : AppTheme.surface3,
                  width: 1,
                ),
              ),
              child: Row(
                children: [
                  Icon(
                    summaryBroken ? Icons.warning_amber_outlined : Icons.monitor_heart_outlined,
                    size: 14,
                    color: summaryBroken ? const Color(0xFFF7768E) : const Color(0xFF9ECE6A),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      '知识管道 · ${h.pagesActive} 页活跃'
                      '${h.pagesIncubating > 0 ? ' · ${h.pagesIncubating} 孵化中' : ''}'
                      '${h.negativeSources > 0 ? ' · ${h.negativeSources} 已否定' : ''}'
                      ' · 摘要 30d 内 ${h.summaryReadyRecent} 成'
                      '${summaryBroken ? ' ${h.summaryUnavailableRecent} 不可用' : ''}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11,
                        color: summaryBroken
                            ? const Color(0xFFF7768E)
                            : AppTheme.textTertiary,
                      ),
                    ),
                  ),
                  Icon(Icons.expand_less, size: 16, color: AppTheme.textTertiary),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  void _showDetails(api.KnowledgeHealthReport h) {
    final runsFuture = ref.read(knowledgeRepositoryProvider).runs();
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: AppTheme.surface1,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(14)),
      ),
      builder: (context) => SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(AppTheme.space4),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                '知识管道健康（近 30 天）',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(height: AppTheme.space3),
              _healthRow('编译', '待审建议 ${h.proposalsPending}（30d 采纳 ${h.proposalsAccepted} / 拒绝 ${h.proposalsRejected}）· 后台任务失败 ${h.backgroundRunsFailedRecent}/${h.backgroundRunsRecent}'),
              _healthRow('库存', '活跃 ${h.pagesActive} · 归档 ${h.pagesArchived} · 无摘要 ${h.pagesWithoutSummary} · 长期未更新 ${h.pagesStale} · 从未投递 ${h.pagesNeverSurfaced}'),
              _healthRow('投递', '挂载 ${h.mountEventsRecent} 次（${h.mountPagesRecent} 页）· 使用记录 ${h.usageRecordsRecent} · 被引用 ${h.citedPagesRecent} 页'),
              _healthRow('检索', 'AI 检索 ${h.searchCallsRecent} 次 · 取页 ${h.pageFetchCallsRecent} 次'),
              _healthRow(
                '摘要管线',
                '完成 ${h.summaryReadyRecent} · 不可用 ${h.summaryUnavailableRecent}'
                '${h.lastSummaryError == null ? '' : '\n最近错误：${h.lastSummaryError}'}',
              ),
              _healthRow('孵化闸门', '待定论 ${h.pagesIncubating} 页 · 负知识 ${h.negativeSources} 条'),
              const SizedBox(height: AppTheme.space3),
              Text(
                '摘要预热账本（背景 tick 多批推进，事故 2026-10-19 P1(c)）',
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textTertiary,
                ),
              ),
              const SizedBox(height: AppTheme.space2),
              FutureBuilder<List<api.KnowledgeBackgroundRunDto>>(
                future: runsFuture,
                builder: (context, runSnap) {
                  final runs = (runSnap.data ?? const [])
                      .where((r) => r.task == 'summary-preheat')
                      .toList();
                  if (runs.isEmpty) {
                    return Text(
                      '暂无预热记录（大树冷启动时由背景分批追平）。',
                      style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                    );
                  }
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      for (final run in runs.take(5))
                        Padding(
                          padding: const EdgeInsets.only(bottom: 4),
                          child: Text(
                            '${run.startedAt.substring(0, 16).replaceFirst("T", " ")} '
                            '${run.status == "succeeded" ? "推进 ${run.resultCount} 节点" : "失败：${run.error ?? "未知"}"}',
                            style: TextStyle(
                              fontSize: 11,
                              color: run.status == 'succeeded'
                                  ? AppTheme.textTertiary
                                  : const Color(0xFFF7768E),
                            ),
                          ),
                        ),
                    ],
                  );
                },
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _healthRow(String label, String value) => Padding(
    padding: const EdgeInsets.only(bottom: AppTheme.space2),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            color: AppTheme.textTertiary,
          ),
        ),
        const SizedBox(height: 2),
        Text(
          value,
          style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
        ),
      ],
    ),
  );
}
