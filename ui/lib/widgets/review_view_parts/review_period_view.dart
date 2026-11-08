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
          Text(
            context.l10n.l6CouldNotLoadStatistics88ae7,
            style: TextStyle(color: AppTheme.textTertiary),
          ),
          const SizedBox(height: AppTheme.space2),
          TextButton(
            onPressed: () =>
                ref.invalidate(periodStatsProvider((periodType, anchor))),
            child: Text(context.l10n.retry),
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
                  periodType == 'week'
                      ? context.l10n.l6NoRecordsThisWeeka75a5
                      : context.l10n.l6NoRecordsThisMonthd57d9,
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
                  _CountPill(
                    label: context.l10n.l6Events550e3,
                    count: data.eventCount,
                  ),
                  const SizedBox(width: AppTheme.space3),
                  _CountPill(
                    label: context.l10n.l6ToDosCompletede1bfa,
                    count: data.todoDoneCount,
                  ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              _DailyBars(counts: data.dailyCounts),
              for (final (title, rows) in <(String, List<api.NamedCountDto>)>[
                (context.l10n.l6EventTypes40b68, data.byEventType),
                (context.l10n.l6Peopleb755b, data.byPerson),
                (context.l10n.pageKindProject, data.byProject),
                (context.l10n.l6Activitiesb2548, data.byActivity),
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
      _future = ref.read(knowledgeRepositoryProvider).health(daysLookback: 30);
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
          padding: EdgeInsets.fromLTRB(
            widget.horizontalPadding,
            4,
            widget.horizontalPadding,
            4,
          ),
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
                  color: summaryBroken
                      ? const Color(0xFFF7768E)
                      : AppTheme.surface3,
                  width: 1,
                ),
              ),
              child: Row(
                children: [
                  Icon(
                    summaryBroken
                        ? Icons.warning_amber_outlined
                        : Icons.monitor_heart_outlined,
                    size: 14,
                    color: summaryBroken
                        ? const Color(0xFFF7768E)
                        : const Color(0xFF9ECE6A),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      context.l10n
                          .l6KnowledgeProcessingArg0ActivePagesArg179c73(
                            (h.pagesActive).toString(),
                            (h.pagesIncubating > 0
                                    ? context.l10n.l6IncubatingArg07051b(
                                        (h.pagesIncubating).toString(),
                                      )
                                    : '')
                                .toString(),
                            (h.negativeSources > 0
                                    ? context.l10n.l6RejectedArg02115d(
                                        (h.negativeSources).toString(),
                                      )
                                    : '')
                                .toString(),
                            (h.summaryReadyRecent).toString(),
                            (summaryBroken
                                    ? context.l10n.l6Arg0Unavailable85bdd(
                                        (h.summaryUnavailableRecent).toString(),
                                      )
                                    : '')
                                .toString(),
                          ),
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
                  Icon(
                    Icons.expand_less,
                    size: 16,
                    color: AppTheme.textTertiary,
                  ),
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
                context.l10n.l6KnowledgeProcessingHealthLast30Days3bf5c,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(height: AppTheme.space3),
              _healthRow(
                context.l10n.l6Compilation8669e,
                context.l10n.l6PendingProposalsArg030DaysAcceptedf2e76(
                  (h.proposalsPending).toString(),
                  (h.proposalsAccepted).toString(),
                  (h.proposalsRejected).toString(),
                  (h.backgroundRunsFailedRecent).toString(),
                  (h.backgroundRunsRecent).toString(),
                ),
              ),
              _healthRow(
                context.l10n.l6Librarydfd6b,
                context.l10n.l6ActiveArg0ArchivedArg1NoSummaryb4539(
                  (h.pagesActive).toString(),
                  (h.pagesArchived).toString(),
                  (h.pagesWithoutSummary).toString(),
                  (h.pagesStale).toString(),
                  (h.pagesNeverSurfaced).toString(),
                ),
              ),
              _healthRow(
                context.l10n.l6Delivery412a9,
                context.l10n.l6AttachedArg0TimesArg1PagesUsageabc07(
                  (h.mountEventsRecent).toString(),
                  (h.mountPagesRecent).toString(),
                  (h.usageRecordsRecent).toString(),
                  (h.citedPagesRecent).toString(),
                ),
              ),
              _healthRow(
                context.l10n.l6Retrieval1c037,
                context.l10n.l6AiSearchesArg0PageReadsArg15216f(
                  (h.searchCallsRecent).toString(),
                  (h.pageFetchCallsRecent).toString(),
                ),
              ),
              _healthRow(
                context.l10n.l6SummaryPipeline00b38,
                context.l10n.l6CompletedArg0UnavailableArg1Arg2ee56d(
                  (h.summaryReadyRecent).toString(),
                  (h.summaryUnavailableRecent).toString(),
                  (h.lastSummaryError == null
                          ? ''
                          : context.errorMessage(
                              context.l10n.load,
                              h.lastSummaryError!,
                            ))
                      .toString(),
                ),
              ),
              _healthRow(
                context.l10n.l6SourceReviewGate877bd,
                context.l10n.l6SourcesAwaitingADecisionArg0Rejected9c019(
                  (h.pagesIncubating).toString(),
                  (h.negativeSources).toString(),
                ),
              ),
              const SizedBox(height: AppTheme.space3),
              Text(
                context
                    .l10n
                    .l6SummaryPreparationHistoryProcessedInBackground2648e,
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
                      context
                          .l10n
                          .l6NoPreparationHistoryYetBackgroundBatchescc092,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
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
                            '${run.status == "succeeded" ? context.l10n.nodesAdvanced(run.resultCount) : context.errorMessage(context.l10n.load, run.error ?? "unknown")}',
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
