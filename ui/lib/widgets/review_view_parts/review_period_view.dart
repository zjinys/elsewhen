part of '../review_view.dart';

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
