part of '../knowledge_panel.dart';

//! 知识来源区与来源跳转链接。

/// Provenance is secondary information in the page, not another dialog stack.
class KnowledgeSourcesSection extends ConsumerWidget {
  final WikiPage page;
  const KnowledgeSourcesSection({super.key, required this.page});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return ref
        .watch(knowledgeDetailsProvider(page.slug))
        .when(
          // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view，
          // 防止分析/消化/同步产出的例行重查把面板内容闪成进度条）。
          skipLoadingOnReload: true,
          loading: () => const LinearProgressIndicator(),
          error: (error, _) => Text(_error(error, context: '读取来源与修订')),
          data: (data) => ListView(
            padding: const EdgeInsets.only(bottom: 24),
            children: [
              KnowledgeReadingControl(slug: page.slug),
              Padding(
                padding: const EdgeInsets.only(top: 8, bottom: 14),
                child: Row(
                  children: [
                    Icon(
                      Icons.info_outline,
                      size: 14,
                      color: AppTheme.textTertiary,
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        '来源用于核对依据，修订记录用于查看知识演进。日常阅读和 AI 检索不需要逐项操作。',
                        style: TextStyle(
                          fontSize: 12,
                          color: AppTheme.textTertiary,
                          height: 1.4,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              if (data.sourcePages.isNotEmpty) ...[
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Text(
                    '上游依据',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
                for (final origin in data.sourcePages)
                  _KnowledgePageLink(
                    page: WikiPage.fromDto(origin),
                    prefix: ['source', 'note'].contains(origin.kind)
                        ? '原料'
                        : '上游知识',
                  ),
                const SizedBox(height: 6),
              ],
              for (var i = 0; i < data.sources.length; i++)
                _SourceTile(
                  source: data.sources[i],
                  page: (i == 0 && ['source', 'note'].contains(page.kind))
                      ? page
                      : null,
                ),
              if (data.sources.isEmpty && page.sourceEventIds.isEmpty)
                Container(
                  margin: const EdgeInsets.only(bottom: 12),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 12,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface1,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: AppTheme.surface3),
                  ),
                  child: Row(
                    children: [
                      Icon(
                        Icons.inbox_outlined,
                        size: 16,
                        color: AppTheme.textTertiary,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        '尚无可核验来源。',
                        style: TextStyle(
                          fontSize: 12.5,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                    ],
                  ),
                ),
              for (final id in page.sourceEventIds)
                Material(
                  color: AppTheme.surface1,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: ListTile(
                    dense: true,
                    leading: Container(
                      padding: const EdgeInsets.all(6),
                      decoration: BoxDecoration(
                        color: AppTheme.surface2,
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Icon(
                        Icons.history_edu_outlined,
                        size: 16,
                        color: AppTheme.accentPrimary,
                      ),
                    ),
                    title: const Text(
                      '来源事件记录',
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    subtitle: Text(
                      id,
                      style: TextStyle(
                        fontSize: 11.5,
                        fontFamily: 'monospace',
                        color: AppTheme.textTertiary,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    trailing: const Icon(Icons.chevron_right, size: 18),
                    onTap: () => _showEvent(context, ref, id),
                  ),
                ),
              if (['source', 'note'].contains(page.kind) &&
                  data.sources.isEmpty)
                Container(
                  margin: const EdgeInsets.only(bottom: 12),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 10,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface1,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: AppTheme.surface3),
                  ),
                  child: _OpinionControl(page: page, opinion: null),
                ),
              if (['method', 'case', 'principle'].contains(page.kind))
                _MetadataEditor(slug: page.slug, metadata: data.metadata),
              for (final issue in data.issues) _IssueTile(issue: issue),
              if (!['source', 'note'].contains(page.kind))
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Material(
                    color: AppTheme.surface1,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(color: AppTheme.surface3),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Theme(
                      data: Theme.of(context)
                          .copyWith(dividerColor: Colors.transparent),
                      child: KnowledgeSourceRepairSection(
                        slug: page.slug,
                        hasEvents: page.sourceEventIds.isNotEmpty,
                        onChanged: () => _refresh(ref, page.slug),
                        previewBuilder: (proposal) => KnowledgeProposalCard(
                          key: ValueKey('repair-${proposal.id}'),
                          proposal: proposal,
                        ),
                      ),
                    ),
                  ),
                ),
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Material(
                  color: AppTheme.surface1,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(10),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: Theme(
                    data: Theme.of(context)
                        .copyWith(dividerColor: Colors.transparent),
                    child: KnowledgeReviewHistorySection(slug: page.slug),
                  ),
                ),
              ),
              if (data.history.length > 1)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Material(
                    color: AppTheme.surface1,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(color: AppTheme.surface3),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Theme(
                      data: Theme.of(context)
                          .copyWith(dividerColor: Colors.transparent),
                      child: ExpansionTile(
                        tilePadding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 4,
                        ),
                        title: Row(
                          children: [
                            const Text(
                              '原文历史版本',
                              style: TextStyle(
                                fontSize: 13.5,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(width: 8),
                            _CountBadge(count: data.history.length),
                          ],
                        ),
                        subtitle: Text(
                          '共有 ${data.history.length} 个历史快照',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                        children: [
                          Padding(
                            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                            child: Column(
                              children: [
                                for (final source in data.history)
                                  _SourceTile(source: source),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              Consumer(
                builder: (context, ref, _) {
                  final items = ref
                      .watch(knowledgeRevisionsProvider(page.slug))
                      .whenOrNull(data: (items) => items);
                  final count = items?.length;
                  final latest = items?.firstOrNull;
                  return Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Material(
                      color: AppTheme.surface1,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(10),
                        side: BorderSide(color: AppTheme.surface3),
                      ),
                      clipBehavior: Clip.antiAlias,
                      child: Theme(
                        data: Theme.of(context)
                            .copyWith(dividerColor: Colors.transparent),
                        child: ExpansionTile(
                          tilePadding: const EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 4,
                          ),
                          title: Row(
                            children: [
                              const Text(
                                '知识修订记录',
                                style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              if (count != null && count > 0) ...[
                                const SizedBox(width: 8),
                                _CountBadge(count: count),
                              ],
                            ],
                          ),
                          subtitle: Text(
                            latest != null
                                ? '最新：${latest.reason} · ${_shortTime(latest.createdAt)}'
                                : '查看本页正文的历次修订与恢复',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textTertiary,
                            ),
                          ),
                          children: [
                            Padding(
                              padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                              child: _RevisionList(
                                slug: page.slug,
                                canRestore: ![
                                  'source',
                                  'note',
                                ].contains(page.kind),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  );
                },
              ),
            ],
          ),
        );
  }
}

class KnowledgeOriginLinks extends ConsumerWidget {
  final String slug;
  const KnowledgeOriginLinks({super.key, required this.slug});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(knowledgeDetailsProvider(slug))
      .when(
        // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view）。
        skipLoadingOnReload: true,
        loading: () => const SizedBox.shrink(),
        error: (_, _) => const Text('来源暂时无法读取'),
        data: (data) => Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final page in data.sourcePages)
              _KnowledgePageLink(
                page: WikiPage.fromDto(page),
                prefix: ['source', 'note'].contains(page.kind) ? '原料' : '上游知识',
              ),
          ],
        ),
      );
}

class _KnowledgePageLink extends ConsumerWidget {
  final WikiPage page;
  final String prefix;
  const _KnowledgePageLink({required this.page, required this.prefix});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Container(
    margin: const EdgeInsets.only(bottom: 8),
    decoration: BoxDecoration(
      color: AppTheme.surface1,
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: AppTheme.surface3),
    ),
    child: InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: () => openWikiPageTab(ref, page),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Row(
          children: [
            Icon(Icons.link_outlined, size: 16, color: AppTheme.accentPrimary),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                '$prefix · ${page.title}',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.textPrimary,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            Icon(Icons.arrow_outward, size: 14, color: AppTheme.textSecondary),
          ],
        ),
      ),
    ),
  );
}

/// Original stays open while proposals and compiled references are inspected.
