import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
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
          // 小节标题：从本页对话生成的独立作品（摘要/观点/脚本…）的各版本。
          // 不名「产物版本」（与 tab 名重复），改名「衍生作品」更直白。
          Expanded(
            child: Text(
              '衍生作品',
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: AppTheme.textSecondary,
                letterSpacing: 0.3,
              ),
            ),
          ),
          IconButton(
            tooltip: '刷新衍生作品',
            onPressed: _busy
                ? null
                : () {
                    _pages.clear();
                    _revisions.clear();
                    _compare.clear();
                    _load();
                  },
            icon: const Icon(Icons.refresh, size: 16),
          ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_error != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Text(
            _error!,
            style: TextStyle(color: Theme.of(context).colorScheme.error),
          ),
        ),
      if (_versions.isEmpty && !_busy)
        Container(
          margin: const EdgeInsets.symmetric(vertical: 6),
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            color: AppTheme.surface1,
            borderRadius: BorderRadius.circular(10),
            border: Border.all(color: AppTheme.surface3),
          ),
          child: Row(
            children: [
              Icon(
                Icons.auto_stories_outlined,
                size: 18,
                color: AppTheme.textTertiary,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  '还没有衍生作品。在本页对话里让 AI 生成摘要、观点、灵感或脚本，保存后会在这里列为版本。',
                  style: TextStyle(
                    fontSize: 12,
                    color: AppTheme.textSecondary,
                    height: 1.45,
                  ),
                ),
              ),
            ],
          ),
        ),
      for (final v in _versions)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Material(
            color: AppTheme.surface1,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(10),
              side: BorderSide(
                color: v.adopted
                    ? AppTheme.accentPrimary.withValues(alpha: 0.35)
                    : AppTheme.surface3,
              ),
            ),
            child: Theme(
              data: Theme.of(context)
                  .copyWith(dividerColor: Colors.transparent),
              child: ExpansionTile(
                key: ValueKey(v.slug),
                onExpansionChanged: (open) {
                  if (open) _open(v.slug);
                },
                tilePadding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 4,
                ),
                leading: Container(
                  padding: const EdgeInsets.all(7),
                  decoration: BoxDecoration(
                    color: v.adopted
                        ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                        : AppTheme.surface2,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(
                    v.adopted
                        ? Icons.check_circle_outline
                        : Icons.auto_awesome_outlined,
                    size: 16,
                    color: v.adopted
                        ? AppTheme.accentPrimary
                        : AppTheme.textSecondary,
                  ),
                ),
                title: Text(
                  '${v.title} · v${v.version}',
                  style: const TextStyle(
                    fontSize: 13.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                subtitle: Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Text(
                    '${v.contentType} · ${v.adopted ? '已采用' : '未采用'} · ${v.createdAt}',
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
                children: [
                  Container(
                    width: double.infinity,
                    margin: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                    decoration: BoxDecoration(
                      color: AppTheme.surface2,
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(color: AppTheme.surface3),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            '模型：${v.model ?? '未知（未记录）'} · 流程版本：${v.strategy ?? '未知（旧产物）'}',
                            style: TextStyle(
                              fontSize: 11.5,
                              color: AppTheme.textTertiary,
                            ),
                          ),
                          const SizedBox(height: 6),
                          Material(
                            color: AppTheme.surface1,
                            shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(6),
                              side: BorderSide(color: AppTheme.surface3),
                            ),
                            child: Theme(
                              data: Theme.of(context)
                                  .copyWith(dividerColor: Colors.transparent),
                              child: ExpansionTile(
                                tilePadding: const EdgeInsets.symmetric(
                                  horizontal: 10,
                                  vertical: 0,
                                ),
                                title: const Text(
                                  '生成指令',
                                  style: TextStyle(fontSize: 12.5),
                                ),
                                children: [
                                  Padding(
                                    padding: const EdgeInsets.fromLTRB(
                                      10,
                                      0,
                                      10,
                                      8,
                                    ),
                                    child: SelectableText(
                                      v.instruction ?? '此版本未记录生成指令',
                                      style: TextStyle(
                                        fontSize: 12,
                                        color: AppTheme.textSecondary,
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                          const SizedBox(height: 10),
                          _body(v.slug),
                          if (v.adoptedRevision != null &&
                              v.adoptedRevision != _revisions[v.slug]) ...[
                            const SizedBox(height: 8),
                            Material(
                              color: AppTheme.surface1,
                              shape: RoundedRectangleBorder(
                                borderRadius: BorderRadius.circular(6),
                                side: BorderSide(color: AppTheme.surface3),
                              ),
                              child: Theme(
                                data: Theme.of(context)
                                    .copyWith(dividerColor: Colors.transparent),
                                child: ExpansionTile(
                                  tilePadding: const EdgeInsets.symmetric(
                                    horizontal: 10,
                                    vertical: 0,
                                  ),
                                  title: const Text(
                                    '查看实际采用的修订',
                                    style: TextStyle(fontSize: 12.5),
                                  ),
                                  onExpansionChanged: (open) async {
                                    if (!open ||
                                        _adoptedBodies.containsKey(v.slug)) {
                                      return;
                                    }
                                    try {
                                      final r = await ref
                                          .read(knowledgeRepositoryProvider)
                                          .artifact(v.slug, v.adoptedRevision);
                                      if (mounted) {
                                        setState(
                                          () => _adoptedBodies[v.slug] =
                                              r.contentMd,
                                        );
                                      }
                                    } catch (e) {
                                      if (mounted) {
                                        setState(() => _error = '读取已采用修订失败：$e');
                                      }
                                    }
                                  },
                                  children: [
                                    Padding(
                                      padding: const EdgeInsets.fromLTRB(
                                        10,
                                        0,
                                        10,
                                        8,
                                      ),
                                      child: _adoptedBodies[v.slug] != null
                                          ? SelectionArea(
                                              child: MarkdownView(
                                                markdown:
                                                    _adoptedBodies[v.slug]!,
                                              ),
                                            )
                                          : const LinearProgressIndicator(),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ],
                          if (v.adoptedRevision != null &&
                              _revisions[v.slug] != null &&
                              v.adoptedRevision != _revisions[v.slug])
                            Padding(
                              padding: const EdgeInsets.only(top: 6),
                              child: Text(
                                '当前正文已有新修订；采用记录仍指向之前确认的修订。',
                                style: TextStyle(
                                  fontSize: 11.5,
                                  color: AppTheme.textTertiary,
                                ),
                              ),
                            ),
                          const SizedBox(height: 10),
                          Wrap(
                            spacing: 8,
                            runSpacing: 4,
                            crossAxisAlignment: WrapCrossAlignment.center,
                            children: [
                              TextButton.icon(
                                icon: const Icon(Icons.open_in_new, size: 15),
                                label: const Text(
                                  '打开知识页',
                                  style: TextStyle(fontSize: 12),
                                ),
                                onPressed: () => openWikiTab(
                                  ref,
                                  PageTabEntry(slug: v.slug, title: v.title),
                                ),
                              ),
                              TextButton.icon(
                                icon: const Icon(Icons.copy_outlined, size: 15),
                                label: const Text(
                                  '复制全文',
                                  style: TextStyle(fontSize: 12),
                                ),
                                onPressed: _pages[v.slug] == null
                                    ? null
                                    : () async {
                                        await Clipboard.setData(
                                          ClipboardData(
                                            text: _pages[v.slug]!.contentMd,
                                          ),
                                        );
                                        if (context.mounted) {
                                          ScaffoldMessenger.of(context)
                                              .showSnackBar(
                                                const SnackBar(
                                                  content: Text('已复制完整正文'),
                                                ),
                                              );
                                        }
                                      },
                              ),
                              FilterChip(
                                label: const Text(
                                  '加入比较',
                                  style: TextStyle(fontSize: 12),
                                ),
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
                                onPressed:
                                    _busy || !_revisions.containsKey(v.slug)
                                    ? null
                                    : () => _adopt(v),
                                child: Text(
                                  v.adopted ? '取消采用' : '采用这个版本',
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: v.adopted
                                        ? AppTheme.accentPrimary
                                        : null,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
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
                        const SizedBox(height: 6),
                        _body(slug),
                      ],
                    ),
                  ),
                )
                .toList();
            return Container(
              margin: const EdgeInsets.only(top: 14),
              decoration: BoxDecoration(
                color: AppTheme.surface1,
                borderRadius: BorderRadius.circular(10),
                border: Border.all(
                  color: AppTheme.accentPrimary.withValues(alpha: 0.35),
                ),
              ),
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(
                          Icons.compare_arrows_rounded,
                          size: 16,
                          color: AppTheme.accentPrimary,
                        ),
                        const SizedBox(width: 6),
                        const Text(
                          '版本比较',
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    if (size.maxWidth >= 650)
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: columns
                            .map((c) => Expanded(child: c))
                            .toList(),
                      )
                    else
                      ...columns,
                  ],
                ),
              ),
            );
          },
        ),
    ],
  );
}
