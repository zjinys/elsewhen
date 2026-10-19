part of '../knowledge_panel.dart';

//! 知识产出区与单条来源 tile。

class KnowledgeOutputsSection extends ConsumerStatefulWidget {
  final WikiPage page;
  const KnowledgeOutputsSection({super.key, required this.page});
  @override
  ConsumerState<KnowledgeOutputsSection> createState() =>
      _KnowledgeOutputsSectionState();
}

class _KnowledgeOutputsSectionState
    extends ConsumerState<KnowledgeOutputsSection> {
  Timer? _poll;
  @override
  void initState() {
    super.initState();
    _poll = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted) ref.invalidate(knowledgeDetailsProvider(widget.page.slug));
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final page = widget.page;
    return ListView(
      padding: const EdgeInsets.only(bottom: 24),
      children: [
        if (page.kind == 'topic')
          TopicOrganizationSection(
            page: page,
            onChanged: () => _refresh(ref, page.slug),
          ),
        if (['source', 'note'].contains(page.kind))
          Padding(
            padding: const EdgeInsets.only(top: 4, bottom: 14),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  Icons.info_outline,
                  size: 14,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '后台会整理参考知识，并结合相关资料维护共同主题。按需提炼的新增产出直接保存；涉及人工修改或已确认内容时才需审阅。原文保留，个人规则由你确认。',
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textTertiary,
                      height: 1.45,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ref
            .watch(knowledgeDetailsProvider(page.slug))
            .when(
              // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view）。
              skipLoadingOnReload: true,
              loading: () => const LinearProgressIndicator(),
              error: (error, _) => Text(_error(error, context: '读取知识产出')),
              data: (data) {
                final pending = data.proposals
                    .where((p) => p.status == 'pending')
                    .toList();
                final outputs = data.outputPages
                    .where((p) => p.area != 'derivative')
                    .toList();
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (pending.isNotEmpty)
                      _outputsSectionTitle(
                        context,
                        '待审建议',
                        count: pending.length,
                      ),
                    for (final proposal in pending)
                      KnowledgeProposalCard(
                        key: ValueKey(proposal.id),
                        proposal: proposal,
                        sourceSlug: page.slug,
                      ),
                    if (outputs.isNotEmpty)
                      _outputsSectionTitle(
                        context,
                        '已整理的参考知识',
                        count: outputs.length,
                      ),
                    for (final output in outputs)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 8),
                        child: Material(
                          color: AppTheme.surface1,
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(10),
                            side: BorderSide(color: AppTheme.surface3),
                          ),
                          child: Theme(
                            data: Theme.of(context)
                                .copyWith(dividerColor: Colors.transparent),
                            child: ExpansionTile(
                              key: PageStorageKey(output.slug),
                              tilePadding: const EdgeInsets.symmetric(
                                horizontal: 14,
                                vertical: 4,
                              ),
                              leading: Container(
                                padding: const EdgeInsets.all(7),
                                decoration: BoxDecoration(
                                  color: AppTheme.surface2,
                                  borderRadius: BorderRadius.circular(8),
                                ),
                                child: Icon(
                                  output.slug.startsWith('knowledge-topic/')
                                      ? Icons.hub_outlined
                                      : Icons.menu_book_outlined,
                                  size: 16,
                                  color: AppTheme.accentPrimary,
                                ),
                              ),
                              title: Text(
                                output.title,
                                style: const TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              subtitle: Padding(
                                padding: const EdgeInsets.only(top: 2),
                                child: Text(
                                  output.slug.startsWith('knowledge-topic/')
                                      ? '跨资料主题 · 包含本页及相关原料'
                                      : '${WikiPage.fromDto(output).kindLabel} · 来自本页原料',
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    color: AppTheme.textTertiary,
                                  ),
                                ),
                              ),
                              children: [
                                Container(
                                  width: double.infinity,
                                  margin: const EdgeInsets.fromLTRB(
                                    10,
                                    0,
                                    10,
                                    10,
                                  ),
                                  decoration: BoxDecoration(
                                    color: AppTheme.surface2,
                                    borderRadius: BorderRadius.circular(8),
                                    border: Border.all(
                                      color: AppTheme.surface3,
                                    ),
                                  ),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Padding(
                                        padding: const EdgeInsets.fromLTRB(
                                          14,
                                          12,
                                          14,
                                          8,
                                        ),
                                        child: SelectionArea(
                                          child: MarkdownView(
                                            markdown: output.contentMd,
                                          ),
                                        ),
                                      ),
                                      Divider(
                                        height: 1,
                                        thickness: 1,
                                        color: AppTheme.surface3,
                                      ),
                                      Padding(
                                        padding: const EdgeInsets.symmetric(
                                          horizontal: 8,
                                          vertical: 4,
                                        ),
                                        child: _KnowledgePageLink(
                                          page: WikiPage.fromDto(output),
                                          prefix: '打开知识页',
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
                    if (pending.isEmpty && outputs.isEmpty)
                      Container(
                        margin: const EdgeInsets.symmetric(vertical: 8),
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: AppTheme.surface1,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: AppTheme.surface3),
                        ),
                        child: Row(
                          children: [
                            Icon(
                              Icons.auto_stories_outlined,
                              size: 20,
                              color: AppTheme.textTertiary,
                            ),
                            const SizedBox(width: 12),
                            Expanded(
                              child: Text(
                                '暂无知识产出。原文仍可直接使用；后台整理会按内容决定是否形成知识。',
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
                    KnowledgeReviewHistorySection(slug: page.slug),
                  ],
                );
              },
            ),
        const SizedBox(height: 20),
        WikiDerivatives(slug: page.slug),
      ],
    );
  }
}

class _SourceTile extends StatelessWidget {
  final api.SourceSnapshot source;
  final WikiPage? page;
  const _SourceTile({required this.source, this.page});

  Future<void> _openLocator(String locator) async {
    final uri = Uri.tryParse(locator);
    if (uri != null && (uri.scheme == 'http' || uri.scheme == 'https')) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  @override
  Widget build(BuildContext context) {
    final isUrl =
        source.locator != null &&
        (source.locator!.startsWith('http://') ||
            source.locator!.startsWith('https://'));
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Material(
        color: AppTheme.surface1,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(color: AppTheme.surface3),
        ),
        clipBehavior: Clip.antiAlias,
        child: Theme(
          data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
          child: ExpansionTile(
            tilePadding: const EdgeInsets.symmetric(
              horizontal: 14,
              vertical: 4,
            ),
            leading: Container(
              padding: const EdgeInsets.all(7),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Icon(
                Icons.article_outlined,
                size: 18,
                color: AppTheme.accentPrimary,
              ),
            ),
            title: Row(
              children: [
                Expanded(
                  child: Text(
                    source.title,
                    style: const TextStyle(
                      fontSize: 13.5,
                      fontWeight: FontWeight.w600,
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 8),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 1,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface2,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    'v${source.version}',
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ),
              ],
            ),
            subtitle: Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Row(
                children: [
                  if (source.locator != null) ...[
                    Icon(
                      isUrl ? Icons.link : Icons.notes,
                      size: 13,
                      color: isUrl
                          ? AppTheme.accentPrimary
                          : AppTheme.textTertiary,
                    ),
                    const SizedBox(width: 4),
                    Flexible(
                      child: isUrl
                          ? InkWell(
                              onTap: () => _openLocator(source.locator!),
                              child: Text(
                                source.locator!,
                                style: TextStyle(
                                  fontSize: 11.5,
                                  color: AppTheme.accentPrimary,
                                  decoration: TextDecoration.underline,
                                ),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            )
                          : Text(
                              source.locator!,
                              style: TextStyle(
                                fontSize: 11.5,
                                color: AppTheme.textTertiary,
                              ),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      '·',
                      style: TextStyle(
                        fontSize: 12,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                    const SizedBox(width: 8),
                  ],
                  Text(
                    _shortTime(source.capturedAt),
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ),
            ),
            children: [
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12),
                margin: const EdgeInsets.fromLTRB(12, 0, 12, 10),
                decoration: BoxDecoration(
                  color: AppTheme.surface2,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: SelectionArea(
                  child: MarkdownView(markdown: source.contentMd),
                ),
              ),
              if (page != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
                  child: _OpinionControl(page: page!, opinion: source.opinion),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
