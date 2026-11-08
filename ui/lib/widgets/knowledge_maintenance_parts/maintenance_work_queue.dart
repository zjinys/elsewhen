part of '../knowledge_maintenance_sections.dart';

//! 工作队列 section

class KnowledgeWorkQueueSection extends ConsumerStatefulWidget {
  final Future<void> Function(String) onOpenPage;
  const KnowledgeWorkQueueSection({super.key, required this.onOpenPage});
  @override
  ConsumerState<KnowledgeWorkQueueSection> createState() => _WorkQueueState();
}

class _WorkQueueState extends ConsumerState<KnowledgeWorkQueueSection> {
  api.KnowledgeQueuePage? _page;
  final List<api.KnowledgeQueueItem> _items = [];
  String? _status, _error;
  bool _busy = false, _open = false;
  Timer? _poll;
  @override
  void initState() {
    super.initState();
    _poll = Timer.periodic(const Duration(seconds: 15), (_) {
      if (_open && !_busy && _items.length <= 50) _load(reset: true);
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _load({bool reset = false}) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final page = await ref
          .read(knowledgeRepositoryProvider)
          .queue(reset ? 0 : _items.length, _status);
      if (!mounted) return;
      setState(() {
        if (reset) _items.clear();
        _items.addAll(page.items);
        _page = page;
      });
    } catch (e) {
      if (mounted) {
        setState(
          () => _error = _failure(
            context,
            e,
            context.l10n.l6LoadProcessingQueue079ca,
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _label(String? status) => switch (status) {
    null => context.l10n.l6All778fc,
    'pending' => context.l10n.pending,
    'running' => context.l10n.processing,
    'waiting' => context.l10n.l6Waiting25d23,
    'retry' => context.l10n.l6TryAgainLaterb8e0d,
    'skipped' => context.l10n.l6Skipped9f38a,
    _ => context.l10n.completed,
  };
  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: Text(context.l10n.l6ProcessingQueueb0388),
    subtitle: Text(context.l10n.l6ViewPendingWorkReadingProgressAnd0ed68),
    onExpansionChanged: (open) {
      _open = open;
      if (open && !_busy) _load(reset: true);
    },
    children: [
      if (_page != null)
        Text(
          context.l10n.l6TotalArg0PendingArg1RunningArg2a5909(
            (_page!.total).toString(),
            (_page!.pending).toString(),
            (_page!.running).toString(),
            (_page!.waiting).toString(),
            (_page!.retry).toString(),
            (_page!.skipped).toString(),
            (_page!.completed).toString(),
          ),
        ),
      TextButton.icon(
        onPressed: _busy ? null : () => _load(reset: true),
        icon: const Icon(Icons.refresh),
        label: Text(context.l10n.l6RefreshListbd82e),
      ),
      Wrap(
        spacing: 6,
        children: [
          for (final status in [
            null,
            'pending',
            'running',
            'waiting',
            'retry',
            'skipped',
            'succeeded',
          ])
            ChoiceChip(
              label: Text(_label(status)),
              selected: _status == status,
              onSelected: _busy
                  ? null
                  : (_) {
                      setState(() => _status = status);
                      _load(reset: true);
                    },
            ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_error != null)
        AppErrorMessage(action: context.l10n.load, error: _error!),
      if (_page != null && _items.isEmpty)
        Text(context.l10n.l6NoTasksInThisState7922e),
      for (final item in _items)
        ListTile(
          title: Text(item.title),
          subtitle: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '${switch (item.task) {
                  'source-reading' => context.l10n.l6FullDocumentReadingc1da8,
                  'source-compilation' => context.l10n.l6SourceProcessingfbebb,
                  'wiki-integration' => context.l10n.l6CrossSourceProcessing5240c,
                  'knowledge-review' => context.l10n.l6KnowledgeReview4f5bb,
                  _ => context.l10n.l6SourceRefresh65f83,
                }} · ${_label(item.status)}',
              ),
              if (item.availableAt != null)
                Text(context.l10n.l6ScheduledArg020b73(item.availableAt!)),
              if (item.detail.isNotEmpty)
                ProcessingNotes(
                  note: item.detail,
                  status: switch (item.status) {
                    'running' => context.l10n.queueDetailRunning,
                    'waiting' => context.l10n.queueDetailWaiting,
                    'retry' => context.l10n.queueDetailRetry,
                    'skipped' => context.l10n.queueDetailSkipped,
                    'succeeded' => context.l10n.queueDetailCompleted,
                    _ => context.l10n.queueDetailPending,
                  },
                ),
            ],
          ),
          onTap: item.pageSlug.isEmpty
              ? null
              : () => widget.onOpenPage(item.pageSlug),
        ),
      if ((_page?.hasMore ?? false) || _error != null)
        TextButton(
          onPressed: _busy ? null : () => _load(),
          child: Text(
            _error != null
                ? context.l10n.l6ReloadQueueb9f40
                : context.l10n.l6LoadMoreTasks1db39,
          ),
        ),
    ],
  );
}
