import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'markdown_view.dart';
import '../l10n/localization.dart';
import 'app_error_notice.dart';

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
  Object? _error;
  bool _adoptError = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load({bool more = false}) async {
    setState(() {
      _busy = true;
      _error = null;
      _adoptError = false;
    });
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
      if (mounted) setState(() => _error = e);
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
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _adopt(api.ArtifactVersion version) async {
    final revision = _revisions[version.slug];
    if (revision == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _adoptError = false;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .adopt(version.slug, revision, !version.adopted);
      if (mounted) {
        notifyKnowledgeChanged(ref);
        await _load();
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = e;
          _adoptError = true;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _date(String raw) {
    final date = DateTime.tryParse(raw)?.toLocal();
    return date == null
        ? raw
        : MaterialLocalizations.of(context).formatCompactDate(date);
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
              context.l10n.artifactWorks,
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: AppTheme.textSecondary,
                letterSpacing: 0.3,
              ),
            ),
          ),
          IconButton(
            tooltip: context.l10n.artifactRefresh,
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
          child: AppErrorMessage(
            action: _adoptError ? context.l10n.save : context.l10n.load,
            error: _error!,
          ),
        ),
      if (_versions.isEmpty && !_busy && _error == null)
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
                  context.l10n.artifactEmpty,
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
                    '${context.artifactTypeLabel(v.contentType)} · ${v.adopted ? context.l10n.readingAdopted : context.l10n.artifactNotAdopted} · ${_date(v.createdAt)}',
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
                            context.l10n.artifactMetadata(
                              v.model ?? context.l10n.artifactUnknownModel,
                              v.strategy ??
                                  context.l10n.artifactUnknownStrategy,
                            ),
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
                                title: Text(
                                  context.l10n.artifactInstruction,
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
                                      v.instruction ??
                                          context.l10n.artifactNoInstruction,
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
                                  title: Text(
                                    context.l10n.artifactAdoptedRevision,
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
                                        setState(() => _error = e);
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
                                context.l10n.artifactNewerRevisionHelp,
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
                                label: Text(
                                  context.l10n.artifactOpenPage,
                                  style: TextStyle(fontSize: 12),
                                ),
                                onPressed: () => openWikiTab(
                                  ref,
                                  PageTabEntry(slug: v.slug, title: v.title),
                                ),
                              ),
                              TextButton.icon(
                                icon: const Icon(Icons.copy_outlined, size: 15),
                                label: Text(
                                  context.l10n.artifactCopyBody,
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
                                          ScaffoldMessenger.of(
                                            context,
                                          ).showSnackBar(
                                            SnackBar(
                                              content: Text(
                                                context.l10n.artifactBodyCopied,
                                              ),
                                            ),
                                          );
                                        }
                                      },
                              ),
                              FilterChip(
                                label: Text(
                                  context.l10n.artifactCompareSelect,
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
                                  v.adopted
                                      ? context.l10n.artifactUnadopt
                                      : context.l10n.artifactAdopt,
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
          child: Text(context.l10n.artifactMoreVersions),
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
                        Flexible(
                          child: Text(
                            context.l10n.artifactCompare,
                            style: TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.w600,
                            ),
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
