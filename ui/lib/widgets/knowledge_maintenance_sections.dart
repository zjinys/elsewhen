import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../utils/error_report.dart';
import 'markdown_view.dart';

String _failure(Object error, String action) => reportUiError(action, error);

String _repairFailure(Object error) {
  final message = _failure(error, '准备来源修复');
  final raw = error.toString();
  if (raw.contains('依据不足')) {
    return '剩余依据不足，请先补充有效原料。当前知识保留并等待补证据。';
  }
  if (raw.contains('后台阅读全文')) {
    return '所选原料正在后台阅读全文，请在阅读完成后准备修复建议。';
  }
  if (raw.contains('上游知识')) {
    return '上游知识仍待复核，请先打开上游页面检查更新。';
  }
  return '$message 可刷新来源后重试；当前正文未修改。';
}

class KnowledgeSourceRepairSection extends ConsumerStatefulWidget {
  final String slug;
  final bool hasEvents;
  final VoidCallback onChanged;
  final Widget Function(api.KnowledgeProposal) previewBuilder;
  const KnowledgeSourceRepairSection({
    super.key,
    required this.slug,
    required this.hasEvents,
    required this.onChanged,
    required this.previewBuilder,
  });
  @override
  ConsumerState<KnowledgeSourceRepairSection> createState() =>
      _SourceRepairState();
}

class _SourceRepairState extends ConsumerState<KnowledgeSourceRepairSection> {
  List<api.KnowledgeRepairSource>? _sources;
  final Set<String> _selected = {};
  bool _busy = false;
  String? _error;
  String _search = '';
  api.KnowledgeProposal? _preview;

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await ref
          .read(knowledgeRepositoryProvider)
          .repairSources(widget.slug);
      if (!mounted) return;
      setState(() {
        _sources = items;
        _selected
          ..clear()
          ..addAll(
            items
                .where((s) => s.selected && s.eligible)
                .map((s) => s.snapshotId),
          );
      });
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取可用来源'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _prepare() async {
    setState(() {
      _busy = true;
      _error = null;
      _preview = null;
    });
    try {
      final repo = ref.read(knowledgeRepositoryProvider);
      final id = await repo.repair(widget.slug, _selected.toList());
      if (!mounted) return;
      widget.onChanged();
      final data = await repo.details(widget.slug);
      if (!mounted) return;
      setState(
        () => _preview = data.proposals.where((p) => p.id == id).firstOrNull,
      );
    } catch (e) {
      if (mounted) {
        setState(() => _error = _repairFailure(e));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('修复来源依据'),
    subtitle: const Text('保留有效来源，或选择替代原料；审阅后更新知识'),
    onExpansionChanged: (open) {
      if (open && _sources == null && !_busy) _load();
    },
    children: [
      const Text('不认可或已归档的来源不能继续支持新结论。下方默认保留有效来源，原知识在确认修订前保留。'),
      if (_sources != null) ...[
        TextField(
          decoration: const InputDecoration(
            labelText: '查找替代原料',
            prefixIcon: Icon(Icons.search),
          ),
          onChanged: (value) => setState(() => _search = value),
        ),
        if (_sources!.length > 50)
          const Text('列表显示前 50 项，可输入标题定位其他原料。已选来源仍保留。'),
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 300),
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final source
                  in _sources!
                      .where(
                        (s) => s.title.toLowerCase().contains(
                          _search.toLowerCase(),
                        ),
                      )
                      .take(50))
                CheckboxListTile(
                  title: Text('${source.title} · v${source.version}'),
                  subtitle: Text(
                    !source.eligible
                        ? '不可用，将从新修订依据中排除'
                        : source.selected
                        ? '当前知识的来源'
                        : '可作为替代来源',
                  ),
                  value: _selected.contains(source.snapshotId),
                  onChanged: _busy || !source.eligible
                      ? null
                      : (value) => setState(() {
                          value == true
                              ? _selected.add(source.snapshotId)
                              : _selected.remove(source.snapshotId);
                        }),
                ),
            ],
          ),
        ),
        if (_selected.isEmpty && !widget.hasEvents)
          const Text('剩余依据不足。请选择或先导入有效原料；当前知识仍待补证据。'),
        if (_selected.length > 8) const Text('最多选择 8 份原料，请缩小范围。'),
        Wrap(
          spacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            FilledButton.icon(
              onPressed:
                  _busy ||
                      (_selected.isEmpty && !widget.hasEvents) ||
                      _selected.length > 8
                  ? null
                  : _prepare,
              icon: const Icon(Icons.fact_check_outlined),
              label: const Text('准备修复建议'),
            ),
            TextButton(
              onPressed: _busy ? null : _load,
              child: const Text('刷新来源'),
            ),
          ],
        ),
      ],
      if (_busy) const LinearProgressIndicator(),
      if (_error != null) Text(_error!),
      if (_error != null && _sources == null)
        TextButton(
          onPressed: _busy ? null : _load,
          child: const Text('重新读取来源'),
        ),
      if (_preview != null) widget.previewBuilder(_preview!),
    ],
  );
}

class KnowledgeReviewHistorySection extends ConsumerStatefulWidget {
  final String slug;
  const KnowledgeReviewHistorySection({super.key, required this.slug});
  @override
  ConsumerState<KnowledgeReviewHistorySection> createState() =>
      _ReviewHistoryState();
}

class _ReviewHistoryState extends ConsumerState<KnowledgeReviewHistorySection> {
  final List<api.KnowledgeReviewRecord> _items = [];
  bool _busy = false, _loaded = false, _more = false;
  String? _error;
  Future<void> _load({bool reset = false}) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await ref
          .read(knowledgeRepositoryProvider)
          .history(widget.slug, reset ? 0 : _items.length);
      if (!mounted) return;
      setState(() {
        if (reset) _items.clear();
        _items.addAll(items);
        _loaded = true;
        _more = items.length == 50;
      });
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取处理历史'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _text(String title, String? content) => content == null
      ? const SizedBox.shrink()
      : ExpansionTile(
          title: Text(title),
          children: [SelectionArea(child: MarkdownView(markdown: content))],
        );

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('处理记录'),
    subtitle: const Text('原建议、采纳内容、拒绝与问题解决记录'),
    onExpansionChanged: (open) {
      if (open && !_busy) _load(reset: true);
    },
    children: [
      if (_busy) const LinearProgressIndicator(),
      if (_error != null) Text(_error!),
      if (_loaded && _items.isEmpty) const Text('暂无处理记录。'),
      for (final item in _items)
        ExpansionTile(
          key: ValueKey('review-${item.id}'),
          title: Text(item.title),
          subtitle: Text(
            '${switch (item.action) {
              'accepted' => '已保存',
              'rejected' => '已拒绝',
              'resolved' => '已关联解决',
              'dismissed' => '已忽略',
              _ => item.action,
            }} · ${item.createdAt}',
          ),
          children: [
            Text(item.description),
            if (item.note != null) Text('处理说明：${item.note}'),
            if (item.selectedParts.isNotEmpty)
              Text(
                '采纳的差异段：${item.selectedParts.map((n) => n.toInt() + 1).join('、')}',
              ),
            if (item.originalApplicable != null)
              Text('建议适用条件：${item.originalApplicable}'),
            if (item.resultApplicable != null)
              Text('保存的适用条件：${item.resultApplicable}'),
            _text('修改前正文', item.beforeContent),
            _text('原始建议', item.originalContent),
            _text('实际保存的正文', item.resultContent),
            if (item.revisionId != null)
              SelectableText('关联修订：${item.revisionId}'),
            if (item.targetSlug != null)
              TextButton(
                onPressed: () async {
                  final page = await ref.read(
                    wikiPageProvider(item.targetSlug!).future,
                  );
                  if (mounted && page != null) openWikiPageTab(ref, page);
                },
                child: const Text('打开对应知识页'),
              ),
          ],
        ),
      if (_more || _error != null)
        TextButton(
          onPressed: _busy ? null : () => _load(),
          child: Text(_error != null ? '重新读取' : '加载更多记录'),
        ),
    ],
  );
}

class KnowledgeIssueResolutionSection extends ConsumerStatefulWidget {
  final api.KnowledgeIssue issue;
  final VoidCallback onChanged;
  const KnowledgeIssueResolutionSection({
    super.key,
    required this.issue,
    required this.onChanged,
  });
  @override
  ConsumerState<KnowledgeIssueResolutionSection> createState() =>
      _IssueResolutionState();
}

class _IssueResolutionState
    extends ConsumerState<KnowledgeIssueResolutionSection> {
  List<api.WikiPageDto>? _targets;
  String? _slug, _error;
  api.KnowledgeRevisionDto? _revision;
  final _note = TextEditingController();
  bool _busy = false, _done = false;
  @override
  void dispose() {
    _note.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
      _slug = null;
      _revision = null;
    });
    try {
      final targets = await ref
          .read(knowledgeRepositoryProvider)
          .resolutionTargets(widget.issue.fingerprint);
      if (mounted) setState(() => _targets = targets);
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取可关联知识'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _select(String? slug) async {
    setState(() {
      _slug = slug;
      _revision = null;
      _busy = true;
      _error = null;
    });
    try {
      final revisions = await ref
          .read(knowledgeRepositoryProvider)
          .revisions(slug!);
      if (mounted) setState(() => _revision = revisions.firstOrNull);
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取解决修订'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _resolve() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .resolveIssue(
            widget.issue.fingerprint,
            _slug!,
            _revision!.id,
            _note.text,
          );
      if (!mounted) return;
      setState(() => _done = true);
      widget.onChanged();
    } catch (e) {
      if (mounted) {
        setState(() => _error = '${_failure(e, '关联解决记录')} 来源或修订可能已变化，请重新选择。');
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => _done
      ? const Text('已保存解决记录')
      : ExpansionTile(
          title: const Text('关联解决记录'),
          subtitle: const Text('将问题关联到覆盖全部依据的知识修订'),
          onExpansionChanged: (open) {
            if (open && _targets == null && !_busy) _load();
          },
          children: [
            if (_targets?.isEmpty ?? false)
              const Text('暂无覆盖这些来源的知识页。请先在相关主题中整理分歧及适用条件，再回到此处关联。'),
            if (_targets?.isNotEmpty ?? false)
              DropdownButtonFormField<String>(
                initialValue: _slug,
                isExpanded: true,
                decoration: const InputDecoration(labelText: '解决问题的知识页'),
                items: [
                  for (final p in _targets!)
                    DropdownMenuItem(
                      value: p.slug,
                      child: Text(p.title, overflow: TextOverflow.ellipsis),
                    ),
                ],
                onChanged: _busy ? null : _select,
              ),
            if (_revision != null) ...[
              Text('当前修订：${_revision!.createdAt}'),
              ExpansionTile(
                title: const Text('核对修订正文'),
                children: [
                  SelectionArea(
                    child: MarkdownView(markdown: _revision!.contentMd),
                  ),
                ],
              ),
              TextField(
                controller: _note,
                maxLength: 1000,
                minLines: 2,
                maxLines: 4,
                decoration: const InputDecoration(labelText: '这次修订如何处理分歧'),
                onChanged: (_) => setState(() {}),
              ),
              FilledButton(
                onPressed: _busy || _note.text.trim().isEmpty ? null : _resolve,
                child: const Text('确认关联解决'),
              ),
            ],
            if (_busy) const LinearProgressIndicator(),
            if (_error != null) Text(_error!),
            if (_error != null)
              TextButton(
                onPressed: _busy ? null : _load,
                child: const Text('刷新可关联知识'),
              ),
          ],
        );
}

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
