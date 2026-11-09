import 'dart:async';

import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';

/// 知识消化队列与运行日志（只读）。消化由后台 worker 自动进行，
/// 这里只负责查看：没有触发或重试入口，失败项冷却期满后会自动回队。
Future<void> showKnowledgeDigestDialog(BuildContext context) {
  return showDialog<void>(
    context: context,
    builder: (_) => const KnowledgeDigestDialog(),
  );
}

class KnowledgeDigestDialog extends ConsumerStatefulWidget {
  const KnowledgeDigestDialog({super.key});

  @override
  ConsumerState<KnowledgeDigestDialog> createState() =>
      _KnowledgeDigestDialogState();
}

/// 队列筛选：「待处理」合并 pending / retry / running。
enum _JobFilter {
  all(null),
  waiting({'pending', 'retry', 'running'}),
  failed({'failed'}),
  skipped({'skipped'}),
  succeeded({'succeeded'});

  const _JobFilter(this.statuses);
  final Set<String>? statuses;
  String label(BuildContext context) => switch (this) {
    all => context.l10n.l6All778fc,
    waiting => context.l10n.pending,
    failed => context.l10n.failed,
    skipped => context.l10n.skipped,
    succeeded => context.l10n.completed,
  };
}

class _KnowledgeDigestDialogState extends ConsumerState<KnowledgeDigestDialog> {
  List<api.KnowledgeDigestJobDto>? _jobs;
  List<api.KnowledgeDigestRunDto>? _runs;
  Object? _error;
  _JobFilter _filter = _JobFilter.all;
  Timer? _refresh;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _load();
    _refresh = Timer.periodic(const Duration(seconds: 5), (_) => _load());
  }

  @override
  void dispose() {
    _refresh?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (_loading) return;
    _loading = true;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final results = await Future.wait([
        repo.listKnowledgeDigestJobs(limit: 500),
        repo.listKnowledgeDigestRuns(limit: 100),
      ]);
      if (!mounted) return;
      setState(() {
        _jobs = results[0] as List<api.KnowledgeDigestJobDto>;
        _runs = results[1] as List<api.KnowledgeDigestRunDto>;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e);
    } finally {
      _loading = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final size = MediaQuery.sizeOf(context);
    return Dialog(
      child: SizedBox(
        width: size.width.clamp(360.0, 760.0),
        height: (size.height * 0.8).clamp(360.0, 720.0),
        child: DefaultTabController(
          length: 2,
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  AppTheme.space6,
                  AppTheme.space4,
                  AppTheme.space2,
                  0,
                ),
                child: Row(
                  children: [
                    Text(
                      context.l10n.knowledgeProcessing,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                    const Spacer(),
                    IconButton(
                      tooltip: context.l10n.refresh,
                      icon: const Icon(Icons.refresh, size: 18),
                      onPressed: _load,
                    ),
                    IconButton(
                      tooltip: context.l10n.close,
                      icon: const Icon(Icons.close, size: 18),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                  ],
                ),
              ),
              TabBar(
                tabs: [
                  Tab(text: context.l10n.l6Queuecd354),
                  Tab(text: context.l10n.l6RunLoga8ce4),
                ],
              ),
              Expanded(
                child: _error != null
                    ? Center(
                        child: AppErrorMessage(
                          action: context.l10n.load,
                          error: _error!,
                          style: TextStyle(color: AppTheme.error),
                        ),
                      )
                    : TabBarView(children: [_buildQueue(), _buildRuns()]),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildQueue() {
    final jobs = _jobs;
    if (jobs == null) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    final statuses = _filter.statuses;
    final shown = statuses == null
        ? jobs
        : jobs.where((job) => statuses.contains(job.status)).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space6,
            AppTheme.space3,
            AppTheme.space6,
            AppTheme.space2,
          ),
          child: Wrap(
            spacing: AppTheme.space2,
            runSpacing: AppTheme.space2,
            children: [
              for (final filter in _JobFilter.values)
                ChoiceChip(
                  label: Text(
                    '${filter.label(context)} ${_countOf(jobs, filter.statuses)}',
                  ),
                  selected: _filter == filter,
                  onSelected: (_) => setState(() => _filter = filter),
                ),
            ],
          ),
        ),
        Expanded(
          child: shown.isEmpty
              ? Center(
                  child: Text(
                    context.l10n.l6NoRecords09f60,
                    style: TextStyle(color: AppTheme.textTertiary),
                  ),
                )
              : ListView.separated(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space6,
                    vertical: AppTheme.space2,
                  ),
                  itemCount: shown.length,
                  separatorBuilder: (_, _) =>
                      Divider(height: 1, color: AppTheme.surface3),
                  itemBuilder: (_, i) => _JobTile(job: shown[i]),
                ),
        ),
      ],
    );
  }

  int _countOf(List<api.KnowledgeDigestJobDto> jobs, Set<String>? statuses) =>
      statuses == null
      ? jobs.length
      : jobs.where((job) => statuses.contains(job.status)).length;

  Widget _buildRuns() {
    final runs = _runs;
    if (runs == null) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    if (runs.isEmpty) {
      return Center(
        child: Text(
          context.l10n.l6NoRunsYet022d0,
          style: TextStyle(color: AppTheme.textTertiary),
        ),
      );
    }
    return ListView.separated(
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space6,
        vertical: AppTheme.space3,
      ),
      itemCount: runs.length,
      separatorBuilder: (_, _) => Divider(height: 1, color: AppTheme.surface3),
      itemBuilder: (_, i) => _RunTile(run: runs[i]),
    );
  }
}

String _formatTime(String? rfc3339) {
  if (rfc3339 == null || rfc3339.isEmpty) return '—';
  final parsed = DateTime.tryParse(rfc3339);
  if (parsed == null) return rfc3339;
  return DateFormat('MM-dd HH:mm').format(parsed.toLocal());
}

({String label, Color color}) _statusStyle(
  BuildContext context,
  String status,
) => switch (status) {
  'pending' => (label: context.l10n.pending, color: AppTheme.textSecondary),
  'running' => (label: context.l10n.processing, color: AppTheme.accentPrimary),
  'retry' => (label: context.l10n.waitingRetry, color: AppTheme.warning),
  'failed' => (
    label: context.l10n.l6FailedCoolingDown4a20e,
    color: AppTheme.error,
  ),
  'skipped' => (label: context.l10n.skipped, color: AppTheme.textTertiary),
  'succeeded' => (label: context.l10n.completed, color: AppTheme.success),
  _ => (label: status, color: AppTheme.textSecondary),
};

class _StatusBadge extends StatelessWidget {
  const _StatusBadge(this.status);
  final String status;

  @override
  Widget build(BuildContext context) {
    final style = _statusStyle(context, status);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: style.color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      ),
      child: Text(
        style.label,
        style: TextStyle(fontSize: 11, color: style.color),
      ),
    );
  }
}

class _JobTile extends StatelessWidget {
  const _JobTile({required this.job});
  final api.KnowledgeDigestJobDto job;

  @override
  Widget build(BuildContext context) {
    final waiting =
        job.status == 'pending' ||
        job.status == 'retry' ||
        job.status == 'failed';
    final detail = <String>[
      context.l10n.l6RecordedArg085217(
        (_formatTime(job.recordedAt)).toString(),
      ),
      if (job.attempts > 0)
        context.l10n.l6AttemptsArg0fcc38((job.attempts).toString()),
      if (job.failedRounds > 0)
        context.l10n.l6AutomaticallyRequeuedArg0Times3363a(
          (job.failedRounds).toString(),
        ),
      if (waiting)
        context.l10n.l6NextAttemptArg0272da(
          (_formatTime(job.availableAt)).toString(),
        ),
    ];
    final reason = job.status == 'skipped' ? job.skipReason : job.lastError;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  job.eventExcerpt,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
                ),
              ),
              const SizedBox(width: AppTheme.space2),
              _StatusBadge(job.status),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            detail.join(' · '),
            style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
          ),
          if (job.reading case final reading?) ...[
            const SizedBox(height: 4),
            Text(switch (reading.phase) {
              'reading' => context.l10n.digestReadingPhase,
              'merging' => context.l10n.digestMergingPhase,
              'ready' => context.l10n.digestReadyPhase,
              'cancelled' => context.l10n.digestCancelledPhase,
              _ => _statusStyle(context, job.status).label,
            }, style: TextStyle(fontSize: 12, color: AppTheme.textSecondary)),
            Text(
              context.l10n.digestReadingProgress(
                reading.readChars.toString(),
                reading.totalChars.toString(),
                reading.completedLeaves.toString(),
                reading.completedMerges.toString(),
              ),
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
            if (reading.error case final error?) ...[
              const SizedBox(height: 2),
              Text(
                error.contains('budget') || error.contains('instructions')
                    ? context.l10n.digestReadingBudgetHelp
                    : error.contains('_limit')
                    ? context.l10n.digestReadingLimitHelp
                    : context.l10n.digestReadingRetryHelp,
                style: TextStyle(fontSize: 12, color: AppTheme.warning),
              ),
              Text(
                context.l10n.l6NextAttemptArg0272da(
                  _formatTime(reading.retryAt),
                ),
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
            ],
          ],
          if (reason != null && reason.isNotEmpty && job.reading == null) ...[
            const SizedBox(height: 2),
            if (job.status == 'skipped')
              ProcessingNotes(
                note: reason,
                status: context.l10n.queueDetailSkipped,
              )
            else
              AppErrorMessage(
                action: context.l10n.knowledgeProcessing,
                error: reason,
              ),
          ],
        ],
      ),
    );
  }
}

class _RunTile extends StatelessWidget {
  const _RunTile({required this.run});
  final api.KnowledgeDigestRunDto run;

  @override
  Widget build(BuildContext context) {
    final duration = run.durationMs == null
        ? null
        : '${(run.durationMs! / 1000).toStringAsFixed(1)}s';
    final summary = <String>[
      _formatTime(run.startedAt),
      context.l10n.l6EventsArg045727((run.eventCount).toString()),
      ?duration,
      if (run.model != null && run.model!.isNotEmpty) run.model!,
    ];
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  summary.join(' · '),
                  style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
                ),
              ),
              _StatusBadge(run.status),
            ],
          ),
          if (run.status == 'succeeded') ...[
            const SizedBox(height: 4),
            _slugLine(context.l10n.newItem, run.createdSlugs),
            _slugLine(context.l10n.l6Updated9db0, run.updatedSlugs),
            _slugLine(
              context.l10n.l6ProtectedOnlyEvidenceCanBeAddedba2bf,
              run.protectedSlugs,
            ),
            if (run.createdSlugs.isEmpty &&
                run.updatedSlugs.isEmpty &&
                run.protectedSlugs.isEmpty)
              Text(
                context.l10n.l6NoNewKnowledgeExtractedc38ab,
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
          ],
          if (run.error != null && run.error!.isNotEmpty) ...[
            const SizedBox(height: 4),
            AppErrorMessage(
              error: run.error!,
              action: context.l10n.knowledgeProcessing,
            ),
          ],
        ],
      ),
    );
  }

  Widget _slugLine(String label, List<String> slugs) {
    if (slugs.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: 2),
      child: SelectableText(
        '$label：${slugs.join('、')}',
        style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
      ),
    );
  }
}
