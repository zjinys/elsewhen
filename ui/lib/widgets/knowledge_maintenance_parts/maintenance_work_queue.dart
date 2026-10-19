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
      if (mounted) setState(() => _error = _failure(e, '读取整理队列'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _label(String? status) => switch (status) {
    null => '全部',
    'pending' => '待处理',
    'running' => '处理中',
    'waiting' => '等待',
    'retry' => '稍后重试',
    'skipped' => '已跳过',
    _ => '已完成',
  };
  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('整理队列'),
    subtitle: const Text('查看积压、阅读进度和等待原因'),
    onExpansionChanged: (open) {
      _open = open;
      if (open && !_busy) _load(reset: true);
    },
    children: [
      if (_page != null)
        Text(
          '共 ${_page!.total} 项任务 · 待处理 ${_page!.pending} · 处理中 ${_page!.running} · 等待 ${_page!.waiting} · 重试 ${_page!.retry} · 跳过 ${_page!.skipped} · 完成 ${_page!.completed}',
        ),
      TextButton.icon(
        onPressed: _busy ? null : () => _load(reset: true),
        icon: const Icon(Icons.refresh),
        label: const Text('刷新列表'),
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
      if (_error != null) Text(_error!),
      if (_page != null && _items.isEmpty) const Text('此状态下暂无任务。'),
      for (final item in _items)
        ListTile(
          title: Text(item.title),
          subtitle: Text(
            '${switch (item.task) {
              'source-reading' => '全文阅读',
              'source-compilation' => '原料整理',
              'wiki-integration' => '跨资料整理',
              'knowledge-review' => '知识复核',
              _ => '来源更新',
            }} · ${_label(item.status)}\n${item.detail}${item.availableAt == null ? '' : '\n计划时间：${item.availableAt}'}',
          ),
          onTap: item.pageSlug.isEmpty
              ? null
              : () => widget.onOpenPage(item.pageSlug),
        ),
      if ((_page?.hasMore ?? false) || _error != null)
        TextButton(
          onPressed: _busy ? null : () => _load(),
          child: Text(_error != null ? '重新读取队列' : '加载更多任务'),
        ),
    ],
  );
}
