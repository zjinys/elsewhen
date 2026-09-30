import 'package:flutter/services.dart';

import '../providers/wiki_provider.dart';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import 'markdown_view.dart';

class ArtifactVersions extends ConsumerStatefulWidget {
  final String slug;
  const ArtifactVersions({super.key, required this.slug});
  @override
  ConsumerState<ArtifactVersions> createState() => _VersionsState();
}

class _VersionsState extends ConsumerState<ArtifactVersions> {
  List<api.ArtifactVersion> _versions = [];
  final Map<String, api.KnowledgeRevisionDto> _pages = {};
  final Map<String, String> _revisions = {};
  final Set<String> _compare = {};
  final Map<String, String> _adoptedBodies = {};
  bool _busy = false, _more = false;
  String? _error;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load({bool more = false}) async {
    setState(() => _busy = true);
    try {
      final rows = await ref
          .read(knowledgeRepositoryProvider)
          .versions(widget.slug, more ? _versions.length : 0);
      if (mounted) {
        setState(() {
          _versions = more ? [..._versions, ...rows] : rows;
          _more = rows.length == 50;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '读取产物版本失败：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _open(String slug) async {
    if (_pages.containsKey(slug)) return;
    try {
      final repo = ref.read(knowledgeRepositoryProvider);
      final page = await repo.artifact(slug, null);
      if (mounted) {
        setState(() {
          _pages[slug] = page;
          _revisions[slug] = page.id;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '读取版本正文失败：$e');
    }
  }

  Future<void> _adopt(api.ArtifactVersion version) async {
    final revision = _revisions[version.slug];
    if (revision == null) return;
    setState(() => _busy = true);
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .adopt(version.slug, revision, !version.adopted);
      if (mounted) {
        notifyKnowledgeChanged(ref);
        await _load();
      }
    } catch (e) {
      if (mounted) setState(() => _error = '采用版本失败：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _body(String slug) => _pages[slug] == null
      ? const LinearProgressIndicator()
      : SelectionArea(child: MarkdownView(markdown: _pages[slug]!.contentMd));
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Row(
        children: [
          const Expanded(child: Text('产物版本')),
          IconButton(
            tooltip: '刷新产物版本',
            onPressed: _busy
                ? null
                : () {
                    _pages.clear();
                    _revisions.clear();
                    _compare.clear();
                    _load();
                  },
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_error != null) Text(_error!),
      if (_versions.isEmpty && !_busy)
        const Text('暂无产物。可以在本页对话中生成摘要、观点、灵感或脚本并保存。'),
      for (final v in _versions)
        ExpansionTile(
          key: ValueKey(v.slug),
          onExpansionChanged: (open) {
            if (open) _open(v.slug);
          },
          title: Text('${v.title} · v${v.version}'),
          subtitle: Text(
            '${v.contentType} · ${v.adopted ? '已采用' : '未采用'} · ${v.createdAt}',
          ),
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: Text(
                '模型：${v.model ?? '未知（未记录）'} · 流程版本：${v.strategy ?? '未知（旧产物）'}',
              ),
            ),
            ExpansionTile(
              title: const Text('生成指令'),
              children: [SelectableText(v.instruction ?? '此版本未记录生成指令')],
            ),
            _body(v.slug),
            if (v.adoptedRevision != null &&
                v.adoptedRevision != _revisions[v.slug])
              ExpansionTile(
                title: const Text('查看实际采用的修订'),
                onExpansionChanged: (open) async {
                  if (!open || _adoptedBodies.containsKey(v.slug)) return;
                  try {
                    final r = await ref
                        .read(knowledgeRepositoryProvider)
                        .artifact(v.slug, v.adoptedRevision);
                    if (mounted)
                      setState(() => _adoptedBodies[v.slug] = r.contentMd);
                  } catch (e) {
                    if (mounted) setState(() => _error = '读取已采用修订失败：$e');
                  }
                },
                children: [
                  if (_adoptedBodies[v.slug] != null)
                    SelectionArea(
                      child: MarkdownView(markdown: _adoptedBodies[v.slug]!),
                    )
                  else
                    const LinearProgressIndicator(),
                ],
              ),
            if (v.adoptedRevision != null &&
                _revisions[v.slug] != null &&
                v.adoptedRevision != _revisions[v.slug])
              const Text('当前正文已有新修订；采用记录仍指向之前确认的修订。'),
            Wrap(
              spacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                TextButton.icon(
                  icon: const Icon(Icons.open_in_new, size: 16),
                  label: const Text('打开知识页'),
                  onPressed: () => openWikiTab(
                    ref,
                    PageTabEntry(slug: v.slug, title: v.title),
                  ),
                ),
                TextButton.icon(
                  icon: const Icon(Icons.copy_outlined, size: 16),
                  label: const Text('复制全文'),
                  onPressed: _pages[v.slug] == null
                      ? null
                      : () async {
                          await Clipboard.setData(
                            ClipboardData(text: _pages[v.slug]!.contentMd),
                          );
                          if (context.mounted)
                            ScaffoldMessenger.of(context).showSnackBar(
                              const SnackBar(content: Text('已复制完整正文')),
                            );
                        },
                ),
                FilterChip(
                  label: const Text('加入比较'),
                  selected: _compare.contains(v.slug),
                  onSelected: (selected) {
                    setState(() {
                      if (!selected) {
                        _compare.remove(v.slug);
                      } else if (_compare.length < 2) {
                        _compare.add(v.slug);
                      }
                    });
                    _open(v.slug);
                  },
                ),
                TextButton(
                  onPressed: _busy || !_revisions.containsKey(v.slug)
                      ? null
                      : () => _adopt(v),
                  child: Text(v.adopted ? '取消采用' : '采用这个版本'),
                ),
              ],
            ),
          ],
        ),
      if (_more)
        TextButton(
          onPressed: _busy ? null : () => _load(more: true),
          child: const Text('加载更多版本'),
        ),
      if (_compare.length == 2)
        LayoutBuilder(
          builder: (context, size) {
            final columns = _compare
                .map(
                  (slug) => Padding(
                    padding: const EdgeInsets.all(8),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          _versions.firstWhere((v) => v.slug == slug).title,
                          style: const TextStyle(fontWeight: FontWeight.bold),
                        ),
                        _body(slug),
                      ],
                    ),
                  ),
                )
                .toList();
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('版本比较'),
                if (size.maxWidth >= 650)
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: columns.map((c) => Expanded(child: c)).toList(),
                  )
                else
                  ...columns,
              ],
            );
          },
        ),
    ],
  );
}
