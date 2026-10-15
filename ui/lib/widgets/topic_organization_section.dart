import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../models/wiki_page.dart';
import '../providers/knowledge_provider.dart';
import '../theme/app_theme.dart';
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
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Material(
      color: AppTheme.surface1,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(10),
        side: BorderSide(color: AppTheme.surface3),
      ),
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
          tilePadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          leading: Container(
            padding: const EdgeInsets.all(7),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(
              Icons.account_tree_outlined,
              size: 16,
              color: AppTheme.textSecondary,
            ),
          ),
          title: const Text(
            '整理主题结构',
            style: TextStyle(fontSize: 13.5, fontWeight: FontWeight.w600),
          ),
          subtitle: Padding(
            padding: const EdgeInsets.only(top: 2),
            child: Text(
              '合并相近主题或拆分过大的主题，预览确认后生效',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 0, 14, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(10),
                    decoration: BoxDecoration(
                      color: AppTheme.surface2,
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: Row(
                      children: [
                        Icon(
                          Icons.info_outline,
                          size: 15,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            '确认后旧主题归档保留，来源和历史仍可追溯。新主题作为参考知识，不会升级为个人规则。',
                            style: TextStyle(
                              fontSize: 11.5,
                              color: AppTheme.textSecondary,
                              height: 1.4,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 10),
                  if (widget.page.status != 'archived') ...[
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            decoration: InputDecoration(
                              hintText: '搜索待合并的主题…',
                              hintStyle: TextStyle(
                                fontSize: 12.5,
                                color: AppTheme.textTertiary,
                              ),
                              isDense: true,
                              contentPadding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 8,
                              ),
                              filled: true,
                              fillColor: AppTheme.surface2,
                              border: OutlineInputBorder(
                                borderRadius: BorderRadius.circular(6),
                                borderSide: BorderSide(
                                  color: AppTheme.surface3,
                                ),
                              ),
                              enabledBorder: OutlineInputBorder(
                                borderRadius: BorderRadius.circular(6),
                                borderSide: BorderSide(
                                  color: AppTheme.surface3,
                                ),
                              ),
                            ),
                            style: const TextStyle(fontSize: 12.5),
                            onChanged: (v) => _query = v,
                            onSubmitted: (_) => _search(),
                          ),
                        ),
                        const SizedBox(width: 8),
                        OutlinedButton.icon(
                          onPressed: _busy ? null : _search,
                          icon: const Icon(Icons.search, size: 15),
                          label: const Text(
                            '查找主题',
                            style: TextStyle(fontSize: 12),
                          ),
                          style: OutlinedButton.styleFrom(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 12,
                              vertical: 10,
                            ),
                          ),
                        ),
                      ],
                    ),
                    if (_candidates.isNotEmpty) const SizedBox(height: 8),
                    for (final page in _candidates.where(
                      (p) => p.slug != widget.page.slug,
                    ))
                      CheckboxListTile(
                        contentPadding: EdgeInsets.zero,
                        title: Text(
                          page.title,
                          style: const TextStyle(fontSize: 13),
                        ),
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
                    const SizedBox(height: 8),
                  ],
                  Wrap(
                    spacing: 8,
                    children: [
                      OutlinedButton.icon(
                        icon: const Icon(Icons.merge_outlined, size: 15),
                        label: const Text(
                          '预览合并',
                          style: TextStyle(fontSize: 12),
                        ),
                        onPressed:
                            _busy ||
                                widget.page.status == 'archived' ||
                                _selected.isEmpty
                            ? null
                            : () => _run(() async {
                                await ref
                                    .read(knowledgeRepositoryProvider)
                                    .organize([
                                      widget.page.slug,
                                      ..._selected,
                                    ], 'merge');
                              }),
                      ),
                      OutlinedButton.icon(
                        icon: const Icon(Icons.call_split, size: 15),
                        label: const Text(
                          '预览拆分本页',
                          style: TextStyle(fontSize: 12),
                        ),
                        onPressed: _busy || widget.page.status == 'archived'
                            ? null
                            : () => _run(() async {
                                await ref
                                    .read(knowledgeRepositoryProvider)
                                    .organize([widget.page.slug], 'split');
                              }),
                      ),
                    ],
                  ),
                  if (_busy) const LinearProgressIndicator(),
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: Text(
                        _error!,
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.error,
                          fontSize: 12,
                        ),
                      ),
                    ),
                  for (final plan in _plans)
                    Container(
                      margin: const EdgeInsets.only(top: 10),
                      child: Material(
                        color: AppTheme.surface2,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(8),
                          side: BorderSide(color: AppTheme.surface3),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.all(12),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Row(
                                children: [
                                  Expanded(
                                    child: Text(
                                      '${plan.mode == 'merge' ? '合并' : '拆分'}方案 · ${plan.inputSlugs.length} 页 → ${plan.topics.length} 页',
                                      style: const TextStyle(
                                        fontSize: 13,
                                        fontWeight: FontWeight.w600,
                                      ),
                                    ),
                                  ),
                                  Container(
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: 6,
                                      vertical: 2,
                                    ),
                                    decoration: BoxDecoration(
                                      color: plan.status == 'accepted'
                                          ? AppTheme.accentPrimary.withValues(
                                              alpha: 0.12,
                                            )
                                          : AppTheme.surface3,
                                      borderRadius: BorderRadius.circular(4),
                                    ),
                                    child: Text(
                                      {
                                            'pending': '待确认',
                                            'accepted': '已采纳',
                                            'rejected': '已拒绝',
                                            'undone': '已撤销',
                                          }[plan.status] ??
                                          plan.status,
                                      style: TextStyle(
                                        fontSize: 11,
                                        fontWeight: FontWeight.w600,
                                        color: plan.status == 'accepted'
                                            ? AppTheme.accentPrimary
                                            : AppTheme.textSecondary,
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                              if (plan.status == 'accepted')
                                Padding(
                                  padding: const EdgeInsets.only(top: 6),
                                  child: TextButton.icon(
                                    icon: const Icon(Icons.undo, size: 15),
                                    label: const Text(
                                      '撤销整批整理',
                                      style: TextStyle(fontSize: 12),
                                    ),
                                    onPressed: _busy
                                        ? null
                                        : () => _run(
                                            () => ref
                                                .read(
                                                  knowledgeRepositoryProvider,
                                                )
                                                .undoOrganization(plan.id),
                                          ),
                                  ),
                                ),
                              const SizedBox(height: 8),
                              for (final topic in plan.topics)
                                Padding(
                                  padding: const EdgeInsets.only(bottom: 6),
                                  child: Material(
                                    color: AppTheme.surface1,
                                    shape: RoundedRectangleBorder(
                                      borderRadius: BorderRadius.circular(6),
                                      side: BorderSide(
                                        color: AppTheme.surface3,
                                      ),
                                    ),
                                    clipBehavior: Clip.antiAlias,
                                    child: Theme(
                                      data: Theme.of(context).copyWith(
                                        dividerColor: Colors.transparent,
                                      ),
                                      child: ExpansionTile(
                                        tilePadding: const EdgeInsets.symmetric(
                                          horizontal: 10,
                                          vertical: 2,
                                        ),
                                        title: Text(
                                          topic.title,
                                          style: const TextStyle(
                                            fontSize: 12.5,
                                            fontWeight: FontWeight.w600,
                                          ),
                                        ),
                                        subtitle: Text(
                                          '${topic.snapshotIds.length} 份原料 · ${topic.eventIds.length} 个事件',
                                          style: TextStyle(
                                            fontSize: 11,
                                            color: AppTheme.textTertiary,
                                          ),
                                        ),
                                        children: [
                                          Padding(
                                            padding: const EdgeInsets.fromLTRB(
                                              10,
                                              0,
                                              10,
                                              10,
                                            ),
                                            child: Column(
                                              crossAxisAlignment:
                                                  CrossAxisAlignment.start,
                                              children: [
                                                if (topic
                                                    .applicableWhen
                                                    .isNotEmpty) ...[
                                                  Text(
                                                    '适用条件：${topic.applicableWhen}',
                                                    style: TextStyle(
                                                      fontSize: 11.5,
                                                      color: AppTheme
                                                          .textSecondary,
                                                    ),
                                                  ),
                                                  const SizedBox(height: 6),
                                                ],
                                                SelectionArea(
                                                  child: MarkdownView(
                                                    markdown: topic.contentMd,
                                                  ),
                                                ),
                                              ],
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                  ),
                                ),
                              if (plan.status == 'pending')
                                Padding(
                                  padding: const EdgeInsets.only(top: 8),
                                  child: Wrap(
                                    spacing: 8,
                                    children: [
                                      FilledButton(
                                        onPressed: _busy
                                            ? null
                                            : () => _run(() async {
                                                await ref
                                                    .read(
                                                      knowledgeRepositoryProvider,
                                                    )
                                                    .resolveOrganization(
                                                      plan.id,
                                                      true,
                                                    );
                                              }),
                                        child: const Text('确认整理并归档旧主题'),
                                      ),
                                      TextButton(
                                        onPressed: _busy
                                            ? null
                                            : () => _run(() async {
                                                await ref
                                                    .read(
                                                      knowledgeRepositoryProvider,
                                                    )
                                                    .resolveOrganization(
                                                      plan.id,
                                                      false,
                                                    );
                                              }),
                                        child: const Text('拒绝方案'),
                                      ),
                                    ],
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  if (_more)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: TextButton(
                        onPressed: _busy ? null : () => _load(more: true),
                        child: const Text(
                          '加载更早整理方案',
                          style: TextStyle(fontSize: 12),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
