part of '../review_view.dart';

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
