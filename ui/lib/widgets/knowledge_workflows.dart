import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../bridge/rust_bridge_repository.dart';

const readingLabels = {
  'unread': '待读',
  'read': '已读',
  'valuable': '值得使用',
  'adopted': '已采用',
  'archived': '归档',
};

class KnowledgeReadingControl extends ConsumerStatefulWidget {
  final String slug;
  const KnowledgeReadingControl({super.key, required this.slug});
  @override
  ConsumerState<KnowledgeReadingControl> createState() => _ReadingState();
}

class _ReadingState extends ConsumerState<KnowledgeReadingControl> {
  String? _value, _error;
  bool _busy = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final value = await ref
          .read(knowledgeRepositoryProvider)
          .reading(widget.slug);
      if (mounted) setState(() => _value = value);
    } catch (e) {
      if (mounted) setState(() => _error = '读取状态失败：$e');
    }
  }

  Future<void> _save(String value) async {
    setState(() => _busy = true);
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .setReading(widget.slug, value);
      if (mounted) {
        setState(() {
          _value = value;
          _error = null;
        });
        notifyKnowledgeChanged(ref);
      }
    } catch (e) {
      if (mounted) setState(() => _error = '保存状态失败：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Wrap(
        spacing: 8,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          const Text('阅读与使用'),
          if (_value != null)
            DropdownButton<String>(
              value: _value,
              items: readingLabels.entries
                  .map(
                    (e) => DropdownMenuItem(value: e.key, child: Text(e.value)),
                  )
                  .toList(),
              onChanged: _busy
                  ? null
                  : (v) {
                      if (v != null) _save(v);
                    },
            ),
          if (_busy)
            const SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
        ],
      ),
      const Text('记录自己的阅读与使用进度；不会改变原文或观点认可状态。'),
      if (_error != null) Text(_error!),
    ],
  );
}

/// Database pagination; no original bodies cross the bridge for a list.
class KnowledgeLibrary extends ConsumerStatefulWidget {
  const KnowledgeLibrary({super.key});
  @override
  ConsumerState<KnowledgeLibrary> createState() => _LibraryState();
}

class _LibraryState extends ConsumerState<KnowledgeLibrary> {
  final _query = TextEditingController(), _tag = TextEditingController();
  String? _area, _kind, _state, _error;
  List<api.LibraryEntry> _items = [];
  bool _busy = false, _more = false;
  int _request = 0;
  Timer? _debounce;
  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _query.dispose();
    _tag.dispose();
    super.dispose();
  }

  Future<void> _load({bool more = false}) async {
    final request = ++_request;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final page = await ref
          .read(knowledgeRepositoryProvider)
          .browse(
            _query.text.trim(),
            _area,
            _kind,
            _tag.text.trim().isEmpty ? null : _tag.text.trim(),
            _state,
            more ? _items.length : 0,
          );
      if (mounted && request == _request) {
        setState(() {
          _items = more ? [..._items, ...page.items] : page.items;
          _more = page.hasMore;
        });
      }
    } catch (e) {
      if (mounted && request == _request) {
        setState(() => _error = '读取知识列表失败：$e');
      }
    } finally {
      if (mounted && request == _request) setState(() => _busy = false);
    }
  }

  void _search() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), () => _load());
  }

  Widget _filter(
    String label,
    String? value,
    Map<String, String> labels,
    void Function(String?) update,
  ) => DropdownButton<String>(
    value: value,
    hint: Text(label),
    items: [
      DropdownMenuItem<String>(value: null, child: Text('全部$label')),
      for (final e in labels.entries)
        DropdownMenuItem(value: e.key, child: Text(e.value)),
    ],
    onChanged: (v) {
      setState(() => update(v));
      _load();
    },
  );
  @override
  Widget build(BuildContext context) {
    ref.listen(knowledgeRevisionProvider, (_, _) => _load());
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            children: [
              TextField(
                controller: _query,
                onChanged: (_) => _search(),
                decoration: InputDecoration(
                  labelText: '搜索标题或全文',
                  prefixIcon: const Icon(Icons.search),
                  suffixIcon: IconButton(
                    tooltip: '清除搜索',
                    icon: const Icon(Icons.clear),
                    onPressed: () {
                      _query.clear();
                      _debounce?.cancel();
                      _load();
                    },
                  ),
                ),
              ),
              Wrap(
                spacing: 12,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  _filter('分区', _area, {
                    'imported': '素材库',
                    'insight': '知识沉淀',
                    'network': '联系人与项目',
                  }, (v) => _area = v),
                  _filter('类型', _kind, {
                    'source': '原料',
                    'note': '笔记',
                    'method': '方法',
                    'case': '案例',
                    'principle': '规律',
                    'topic': '主题',
                    'person': '联系人',
                    'project': '项目',
                  }, (v) => _kind = v),
                  _filter('阅读状态', _state, readingLabels, (v) => _state = v),
                  SizedBox(
                    width: 140,
                    child: TextField(
                      controller: _tag,
                      onChanged: (_) => _search(),
                      decoration: const InputDecoration(labelText: '标签'),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        if (_busy) const LinearProgressIndicator(),
        if (_error != null) Text(_error!),
        Expanded(
          child: ListView.builder(
            itemCount: _items.length + 1,
            itemBuilder: (context, index) {
              if (index == _items.length) {
                return _more
                    ? TextButton(
                        onPressed: _busy ? null : () => _load(more: true),
                        child: const Text('加载更多'),
                      )
                    : Padding(
                        padding: const EdgeInsets.all(16),
                        child: Text(_items.isEmpty ? '没有匹配的知识页' : '已显示全部结果'),
                      );
              }
              final p = _items[index];
              return ListTile(
                title: Text(
                  p.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                subtitle: Text(
                  p.summary,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                trailing: Text(readingLabels[p.readingState] ?? p.readingState),
                onTap: () => openWikiTab(
                  ref,
                  PageTabEntry(slug: p.slug, title: p.title),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class KnowledgeBatchReview extends ConsumerStatefulWidget {
  final List<api.KnowledgeProposal> proposals;
  final Widget Function(api.KnowledgeProposal) preview;
  const KnowledgeBatchReview({
    super.key,
    required this.proposals,
    required this.preview,
  });
  @override
  ConsumerState<KnowledgeBatchReview> createState() => _BatchState();
}

class _BatchState extends ConsumerState<KnowledgeBatchReview> {
  final Set<String> _selected = {};
  bool _busy = false;
  String? _result;
  Future<void> _resolve(bool accept) async {
    setState(() => _busy = true);
    try {
      final results = await ref
          .read(knowledgeRepositoryProvider)
          .resolveBatch(_selected.toList(), accept);
      if (mounted) {
        setState(() {
          _selected.removeAll(results.where((r) => r.success).map((r) => r.id));
          _result = results.map((r) => r.detail).join('\n');
        });
        notifyKnowledgeChanged(ref);
      }
    } catch (e) {
      if (mounted) setState(() => _result = '批量处理失败：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('批量审阅修订'),
    subtitle: const Text('先查看正文与适用条件，每批最多确认 20 项'),
    children: [
      for (final p in widget.proposals.where(
        (p) => p.pageId != null && p.baseHash != null && p.status == 'pending',
      ))
        Column(
          children: [
            CheckboxListTile(
              title: Text(p.title),
              value: _selected.contains(p.id),
              onChanged: _busy
                  ? null
                  : (v) => setState(() {
                      if (v == true && _selected.length < 20) {
                        _selected.add(p.id);
                      } else {
                        _selected.remove(p.id);
                      }
                    }),
            ),
            ExpansionTile(
              title: const Text('查看本项修订'),
              children: [widget.preview(p)],
            ),
          ],
        ),
      Wrap(
        spacing: 8,
        children: [
          FilledButton(
            onPressed: _busy || _selected.isEmpty ? null : () => _resolve(true),
            child: Text('确认采纳 ${_selected.length} 项'),
          ),
          TextButton(
            onPressed: _busy || _selected.isEmpty
                ? null
                : () => _resolve(false),
            child: const Text('拒绝所选'),
          ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_result != null) Text(_result!),
    ],
  );
}
