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
  all('全部', null),
  waiting('待处理', {'pending', 'retry', 'running'}),
  failed('失败', {'failed'}),
  skipped('跳过', {'skipped'}),
  succeeded('已完成', {'succeeded'});

  const _JobFilter(this.label, this.statuses);
  final String label;
  final Set<String>? statuses;
}

class _KnowledgeDigestDialogState extends ConsumerState<KnowledgeDigestDialog> {
  List<api.KnowledgeDigestJobDto>? _jobs;
  List<api.KnowledgeDigestRunDto>? _runs;
  Object? _error;
  _JobFilter _filter = _JobFilter.all;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
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
                      '知识消化',
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                    const Spacer(),
                    IconButton(
                      tooltip: '刷新',
                      icon: const Icon(Icons.refresh, size: 18),
                      onPressed: _load,
                    ),
                    IconButton(
                      tooltip: '关闭',
                      icon: const Icon(Icons.close, size: 18),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                  ],
                ),
              ),
              const TabBar(
                tabs: [
                  Tab(text: '队列'),
                  Tab(text: '运行日志'),
                ],
              ),
              Expanded(
                child: _error != null
                    ? Center(
                        child: Text(
                          '读取失败：$_error',
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
                    '${filter.label} ${_countOf(jobs, filter.statuses)}',
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
                    '没有记录',
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
        child: Text('还没有运行记录', style: TextStyle(color: AppTheme.textTertiary)),
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

({String label, Color color}) _statusStyle(String status) => switch (status) {
  'pending' => (label: '待处理', color: AppTheme.textSecondary),
  'running' => (label: '处理中', color: AppTheme.accentPrimary),
  'retry' => (label: '等待重试', color: AppTheme.warning),
  'failed' => (label: '失败（冷却中）', color: AppTheme.error),
  'skipped' => (label: '跳过', color: AppTheme.textTertiary),
  'succeeded' => (label: '已完成', color: AppTheme.success),
  _ => (label: status, color: AppTheme.textSecondary),
};

class _StatusBadge extends StatelessWidget {
  const _StatusBadge(this.status);
  final String status;

  @override
  Widget build(BuildContext context) {
    final style = _statusStyle(status);
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
      '记录于 ${_formatTime(job.recordedAt)}',
      if (job.attempts > 0) '尝试 ${job.attempts} 次',
      if (job.failedRounds > 0) '已自动回队 ${job.failedRounds} 轮',
      if (waiting) '下次处理 ${_formatTime(job.availableAt)}',
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
          if (reason != null && reason.isNotEmpty) ...[
            const SizedBox(height: 2),
            Text(
              reason,
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 11,
                color: job.status == 'skipped'
                    ? AppTheme.textTertiary
                    : AppTheme.error,
              ),
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
      '${run.eventCount} 条事件',
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
            _slugLine('新建', run.createdSlugs),
            _slugLine('更新', run.updatedSlugs),
            _slugLine('受保护（仅累加证据）', run.protectedSlugs),
            if (run.createdSlugs.isEmpty &&
                run.updatedSlugs.isEmpty &&
                run.protectedSlugs.isEmpty)
              Text(
                '没有提炼出新的知识',
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
          ],
          if (run.error != null && run.error!.isNotEmpty) ...[
            const SizedBox(height: 4),
            SelectableText(
              run.error!,
              style: TextStyle(fontSize: 11, color: AppTheme.error),
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
