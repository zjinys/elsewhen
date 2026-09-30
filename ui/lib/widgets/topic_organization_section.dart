import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../models/wiki_page.dart';
import '../providers/knowledge_provider.dart';
import 'markdown_view.dart';

class TopicOrganizationSection extends ConsumerStatefulWidget {
  final WikiPage page;
  final VoidCallback onChanged;
  const TopicOrganizationSection({
    super.key,
    required this.page,
    required this.onChanged,
  });
  @override
  ConsumerState<TopicOrganizationSection> createState() =>
      _TopicOrganizationSectionState();
}

class _TopicOrganizationSectionState
    extends ConsumerState<TopicOrganizationSection> {
  final Set<String> _selected = {};
  List<api.TopicOrganizationPreview> _plans = [];
  bool _busy = false;
  bool _more = false;
  String _query = '';
  List<api.LibraryEntry> _candidates = [];
  String? _error;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load({bool more = false}) async {
    try {
      final plans = await ref
          .read(knowledgeRepositoryProvider)
          .organizationHistory(widget.page.slug, more ? _plans.length : 0);
      if (mounted) {
        setState(() {
          _plans = more ? [..._plans, ...plans] : plans;
          _more = plans.length == 50;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '读取整理方案失败：$e');
    }
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
      if (!mounted) return;
      widget.onChanged();
      await _load();
    } catch (e) {
      if (mounted) setState(() => _error = '整理未完成，原主题保留：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _search() async {
    try {
      final rows = await ref
          .read(knowledgeRepositoryProvider)
          .browse(_query, null, 'topic', null, null, 0);
      if (mounted) setState(() => _candidates = rows.items);
    } catch (e) {
      if (mounted) setState(() => _error = '查找失败：$e');
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('整理主题结构'),
    subtitle: const Text('合并相近主题或拆分过大的主题，预览确认后生效'),
    children: [
      const Text('确认后旧主题归档保留，来源和历史仍可追溯。新主题作为参考知识，不会升级为个人规则。'),
      if (widget.page.status != 'archived') ...[
        TextField(
          decoration: const InputDecoration(labelText: '搜索待合并的主题'),
          onChanged: (v) => _query = v,
          onSubmitted: (_) => _search(),
        ),
        TextButton(
          onPressed: _busy ? null : _search,
          child: const Text('查找主题'),
        ),
        for (final page in _candidates.where((p) => p.slug != widget.page.slug))
          CheckboxListTile(
            title: Text(page.title),
            value: _selected.contains(page.slug),
            onChanged: _busy
                ? null
                : (v) => setState(() {
                    if (v == true && _selected.length < 7) {
                      _selected.add(page.slug);
                    } else {
                      _selected.remove(page.slug);
                    }
                  }),
          ),
      ],
      Wrap(
        spacing: 8,
        children: [
          OutlinedButton.icon(
            icon: const Icon(Icons.merge_outlined),
            label: const Text('预览合并'),
            onPressed:
                _busy || widget.page.status == 'archived' || _selected.isEmpty
                ? null
                : () => _run(() async {
                    await ref.read(knowledgeRepositoryProvider).organize([
                      widget.page.slug,
                      ..._selected,
                    ], 'merge');
                  }),
          ),
          OutlinedButton.icon(
            icon: const Icon(Icons.call_split),
            label: const Text('预览拆分本页'),
            onPressed: _busy || widget.page.status == 'archived'
                ? null
                : () => _run(() async {
                    await ref.read(knowledgeRepositoryProvider).organize([
                      widget.page.slug,
                    ], 'split');
                  }),
          ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_error != null)
        Text(
          _error!,
          style: TextStyle(color: Theme.of(context).colorScheme.error),
        ),
      for (final plan in _plans)
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${plan.mode == 'merge' ? '合并' : '拆分'}方案 · ${plan.inputSlugs.length} 页 → ${plan.topics.length} 页',
                ),
                Text(
                  {
                        'pending': '待确认',
                        'accepted': '已采纳',
                        'rejected': '已拒绝',
                        'undone': '已撤销',
                      }[plan.status] ??
                      plan.status,
                ),
                if (plan.status == 'accepted')
                  TextButton.icon(
                    icon: const Icon(Icons.undo),
                    label: const Text('撤销整批整理'),
                    onPressed: _busy
                        ? null
                        : () => _run(
                            () => ref
                                .read(knowledgeRepositoryProvider)
                                .undoOrganization(plan.id),
                          ),
                  ),
                for (final topic in plan.topics)
                  ExpansionTile(
                    title: Text(topic.title),
                    subtitle: Text(
                      '${topic.snapshotIds.length} 份原料 · ${topic.eventIds.length} 个事件',
                    ),
                    children: [
                      Text('适用条件：${topic.applicableWhen}'),
                      SelectionArea(
                        child: MarkdownView(markdown: topic.contentMd),
                      ),
                    ],
                  ),
                if (plan.status == 'pending')
                  Wrap(
                    spacing: 8,
                    children: [
                      FilledButton(
                        onPressed: _busy
                            ? null
                            : () => _run(() async {
                                await ref
                                    .read(knowledgeRepositoryProvider)
                                    .resolveOrganization(plan.id, true);
                              }),
                        child: const Text('确认整理并归档旧主题'),
                      ),
                      TextButton(
                        onPressed: _busy
                            ? null
                            : () => _run(() async {
                                await ref
                                    .read(knowledgeRepositoryProvider)
                                    .resolveOrganization(plan.id, false);
                              }),
                        child: const Text('拒绝方案'),
                      ),
                    ],
                  ),
              ],
            ),
          ),
        ),
      if (_more)
        TextButton(
          onPressed: _busy ? null : () => _load(more: true),
          child: const Text('加载更多整理历史'),
        ),
    ],
  );
}
