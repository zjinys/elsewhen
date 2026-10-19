part of '../knowledge_panel.dart';

//! 知识维护对话框与引用入口按钮。

class KnowledgeMaintenanceButton extends StatelessWidget {
  const KnowledgeMaintenanceButton({super.key});
  @override
  Widget build(BuildContext context) => TextButton.icon(
    icon: const Icon(Icons.fact_check_outlined, size: 18),
    label: const Text('知识审阅'),
    onPressed: () => showKnowledgeMaintenance(context),
  );
}

class _MaintenanceDialog extends ConsumerWidget {
  const _MaintenanceDialog();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final proposals = ref.watch(knowledgeProposalsProvider);
    final issues = ref.watch(knowledgeIssuesProvider);
    final runs = ref.watch(knowledgeRunsProvider);
    return Dialog(
      child: SizedBox(
        width: 860,
        height: MediaQuery.sizeOf(context).height * .85,
        child: Column(
          children: [
            ListTile(
              title: const Text('知识审阅'),
              subtitle: const Text('后台自动整理参考知识；人工修改与规则确认由你决定'),
              trailing: IconButton(
                onPressed: () => Navigator.pop(context),
                icon: const Icon(Icons.close),
              ),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.all(16),
                children: [
                  proposals.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取提案')),
                    data: (items) => Column(
                      children: [
                        if (items.any(
                          (p) =>
                              p.pageId != null &&
                              p.baseHash != null &&
                              p.status == 'pending',
                        ))
                          KnowledgeBatchReview(
                            proposals: items,
                            preview: (p) => KnowledgeProposalCard(proposal: p),
                          ),
                        if (!items.any((p) => p.status == 'pending'))
                          const Text('暂无需要你确认的修改。新增参考知识已直接保存在产出中。'),
                        for (final p in items.where(
                          (p) => p.status == 'pending',
                        ))
                          KnowledgeProposalCard(proposal: p),
                      ],
                    ),
                  ),
                  const Divider(),
                  const Text('来源与内容检查'),
                  issues.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取检查项')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('暂无待核对提示。'),
                        for (final issue in items) _IssueTile(issue: issue),
                      ],
                    ),
                  ),
                  const Divider(),
                  KnowledgeWorkQueueSection(
                    onOpenPage: (slug) => _openPage(context, ref, slug),
                  ),
                  const Text('最近自动整理与洞察记录'),
                  runs.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取洞察记录')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('积累事件后由后台自动检查，无需手动触发。'),
                        for (final run in items)
                          ListTile(
                            onTap: run.sourceSlug == null
                                ? null
                                : () =>
                                      _openPage(context, ref, run.sourceSlug!),
                            title: Text(
                              run.status == 'succeeded'
                                  ? '${switch (run.task) {
                                      'source-compilation' => '原料整理',
                                      'source-reading' => '全文阅读',
                                      'knowledge-refresh' => '来源更新检查',
                                      'wiki-integration' => '主题整理与检查',
                                      _ => '洞察',
                                    }}完成 · ${run.resultCount} 项'
                                  : run.status == 'running'
                                  ? '处理中'
                                  : '失败，稍后自动重试',
                            ),
                            subtitle: Text(
                              [
                                if (run.sourceTitle != null)
                                  '${run.sourceTitle}${run.sourceVersion == null ? '' : ' · v${run.sourceVersion}'}',
                                run.startedAt,
                                if (run.detail != null) run.detail!,
                                if (run.error != null) run.error!,
                                if (run.retryAt != null) '自动重试：${run.retryAt}',
                              ].join('\n'),
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class KnowledgeCitationsButton extends ConsumerWidget {
  final String messageId;
  const KnowledgeCitationsButton({super.key, required this.messageId});
  @override
  Widget build(BuildContext context, WidgetRef ref) => TextButton.icon(
    icon: const Icon(Icons.source_outlined, size: 14),
    label: const Text('引用依据'),
    onPressed: () async {
      try {
        final citations = await ref
            .read(knowledgeRepositoryProvider)
            .citations(messageId);
        if (!context.mounted) return;
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('本条回答实际引用的知识'),
            content: SizedBox(
              width: 680,
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (citations.isEmpty) const Text('这条回答没有经过系统核验的知识引用。'),
                    for (final citation in citations) ...[
                      TextButton(
                        onPressed: () =>
                            _openPage(context, ref, citation.pageSlug),
                        child: Text(citation.title),
                      ),
                      Text('${citation.category} · ${citation.reason}'),
                      if (citation.applicableWhen.isNotEmpty)
                        Text('适用条件：${citation.applicableWhen}'),
                      SelectionArea(child: Text(citation.excerpt)),
                      for (final source in citation.sources)
                        TextButton(
                          child: Text('${source.title} · v${source.version}'),
                          onPressed: () async {
                            try {
                              final snapshot = await ref
                                  .read(knowledgeRepositoryProvider)
                                  .snapshot(source.snapshotId);
                              if (context.mounted) {
                                await showKnowledgeText(
                                  context,
                                  source.title,
                                  snapshot?.contentMd ?? '来源版本不存在',
                                );
                              }
                            } catch (error) {
                              if (context.mounted) {
                                await showKnowledgeText(
                                  context,
                                  '读取失败',
                                  _error(error),
                                );
                              }
                            }
                          },
                        ),
                      for (final id in citation.eventIds)
                        TextButton(
                          onPressed: () => _showEvent(context, ref, id),
                          child: const Text('查看来源事件'),
                        ),
                      const Divider(),
                    ],
                  ],
                ),
              ),
            ),
          ),
        );
      } catch (error) {
        if (context.mounted) {
          await showKnowledgeText(
            context,
            '读取失败',
            _error(error, context: '读取来源事件'),
          );
        }
      }
    },
  );
}
