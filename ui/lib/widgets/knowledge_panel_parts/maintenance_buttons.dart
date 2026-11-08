part of '../knowledge_panel.dart';

//! 知识维护对话框与引用入口按钮。

class KnowledgeMaintenanceButton extends StatelessWidget {
  const KnowledgeMaintenanceButton({super.key});
  @override
  Widget build(BuildContext context) => TextButton.icon(
    icon: const Icon(Icons.fact_check_outlined, size: 18),
    label: Text(context.l10n.knowledgeReview),
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
              title: Text(context.l10n.knowledgeReview),
              subtitle: Text(
                context
                    .l10n
                    .l6ReferenceKnowledgeIsOrganizedAutomaticallyYoucefad,
              ),
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
                    error: (e, _) => AppErrorMessage(
                      action: context.l10n.load,
                      error: _error(
                        context,
                        e,
                        context: context.l10n.l6LoadProposal021f2,
                      ),
                    ),
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
                          Text(
                            context
                                .l10n
                                .l6NoChangesNeedYourConfirmationNew743fb,
                          ),
                        for (final p in items.where(
                          (p) => p.status == 'pending',
                        ))
                          KnowledgeProposalCard(proposal: p),
                      ],
                    ),
                  ),
                  const Divider(),
                  Text(context.l10n.l6SourceAndContentChecks5517e),
                  issues.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => AppErrorMessage(
                      action: context.l10n.load,
                      error: _error(
                        context,
                        e,
                        context: context.l10n.l6LoadCheckse2e22,
                      ),
                    ),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty)
                          Text(context.l10n.l6NoChecksNeedReview4053b),
                        for (final issue in items) _IssueTile(issue: issue),
                      ],
                    ),
                  ),
                  const Divider(),
                  KnowledgeWorkQueueSection(
                    onOpenPage: (slug) => _openPage(context, ref, slug),
                  ),
                  Text(context.l10n.l6RecentProcessingAndInsights23cc8),
                  runs.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => AppErrorMessage(
                      action: context.l10n.load,
                      error: _error(
                        context,
                        e,
                        context: context.l10n.l6LoadInsights25b9e,
                      ),
                    ),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty)
                          Text(
                            context
                                .l10n
                                .l6CheckedAutomaticallyInTheBackgroundAs34c09,
                          ),
                        for (final run in items)
                          ListTile(
                            onTap: run.sourceSlug == null
                                ? null
                                : () =>
                                      _openPage(context, ref, run.sourceSlug!),
                            title: Text(
                              run.status == 'succeeded'
                                  ? context.l10n.l6Arg0CompletedArg1Items30653(
                                      (switch (run.task) {
                                        'source-compilation' =>
                                          context.l10n.l6SourceProcessingfbebb,
                                        'source-reading' =>
                                          context
                                              .l10n
                                              .l6FullDocumentReadingc1da8,
                                        'knowledge-refresh' =>
                                          context.l10n.l6SourceUpdateCheckc6668,
                                        'wiki-integration' =>
                                          context
                                              .l10n
                                              .l6TopicProcessingAndChecks872c6,
                                        _ => context.l10n.pageKindInsight,
                                      }).toString(),
                                      (run.resultCount).toString(),
                                    )
                                  : run.status == 'running'
                                  ? context.l10n.processing
                                  : context
                                        .l10n
                                        .l6FailedAutomaticRetryScheduledc9e00,
                            ),
                            subtitle: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  [
                                    if (run.sourceTitle != null)
                                      '${run.sourceTitle}${run.sourceVersion == null ? '' : ' · v${run.sourceVersion}'}',
                                    run.startedAt,
                                    if (run.retryAt != null)
                                      context.l10n.l6AutomaticRetryArg06d99e(
                                        run.retryAt!,
                                      ),
                                  ].join('\n'),
                                ),
                                if (run.detail != null)
                                  ProcessingNotes(note: run.detail!),
                                if (run.error != null)
                                  AppErrorMessage(
                                    error: run.error!,
                                    action: context.l10n.knowledgeProcessing,
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
    label: Text(context.l10n.l6SupportingEvidenceabba3),
    onPressed: () async {
      try {
        final citations = await ref
            .read(knowledgeRepositoryProvider)
            .citations(messageId);
        if (!context.mounted) return;
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(context.l10n.l6KnowledgeCitedInThisAnswer4ad6e),
            content: SizedBox(
              width: 680,
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (citations.isEmpty)
                      Text(context.l10n.l6ThisAnswerHasNoSystemVerified0ba58),
                    for (final citation in citations) ...[
                      TextButton(
                        onPressed: () =>
                            _openPage(context, ref, citation.pageSlug),
                        child: Text(citation.title),
                      ),
                      Text('${citation.category} · ${citation.reason}'),
                      if (citation.applicableWhen.isNotEmpty)
                        Text(
                          context.l10n.l6ConditionsArg006edc(
                            (citation.applicableWhen).toString(),
                          ),
                        ),
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
                                  snapshot?.contentMd ??
                                      context
                                          .l10n
                                          .l6SourceVersionDoesNotExist8a671,
                                );
                              }
                            } catch (error) {
                              if (context.mounted) {
                                await showKnowledgeText(
                                  context,
                                  context.l10n.l6CouldNotLoadd9f60,
                                  _error(context, error),
                                );
                              }
                            }
                          },
                        ),
                      for (final id in citation.eventIds)
                        TextButton(
                          onPressed: () => _showEvent(context, ref, id),
                          child: Text(context.l10n.l6ViewSourceEvent3c3f9),
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
            context.l10n.l6CouldNotLoadd9f60,
            _error(
              context,
              error,
              context: context.l10n.l6LoadSourceEventsaaf09,
            ),
          );
        }
      }
    },
  );
}
