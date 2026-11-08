part of '../wiki_page_detail_view.dart';

//! 实体事实清单

class _EntityFacts extends ConsumerWidget {
  final String slug;
  const _EntityFacts({required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final facts = ref.watch(entityFactsProvider(slug)).value ?? const [];
    if (facts.isEmpty) return const SizedBox.shrink();
    final conflicts = _conflictingFactIds(facts);
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            context.l10n.entityFacts,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textTertiary,
            ),
          ),
          if (conflicts.isNotEmpty) ...[
            const SizedBox(height: 8),
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: AppTheme.warning.withValues(alpha: .10),
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(
                  color: AppTheme.warning.withValues(alpha: .35),
                ),
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    Icons.warning_amber_rounded,
                    size: 16,
                    color: AppTheme.warning,
                  ),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      context.l10n.entityFactConflictHelp,
                      style: TextStyle(
                        fontSize: 11.5,
                        height: 1.35,
                        color: AppTheme.textSecondary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
          const SizedBox(height: 8),
          for (final fact in facts.take(8))
            Container(
              margin: const EdgeInsets.only(bottom: 8),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.surface3),
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    conflicts.contains(fact.id)
                        ? Icons.warning_amber_rounded
                        : Icons.fact_check_outlined,
                    size: 14,
                    color: conflicts.contains(fact.id)
                        ? AppTheme.warning
                        : AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          fact.factText,
                          style: TextStyle(
                            fontSize: 12,
                            height: 1.4,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                        const SizedBox(height: 4),
                        InkWell(
                          onTap: () =>
                              _showSource(context, ref, fact.sourceEventId),
                          child: Text(
                            context.l10n.entityFactSource(
                              _factDate(context, fact.occurredAt),
                              '${fact.confidence}',
                              _shortId(fact.sourceEventId),
                            ),
                            style: TextStyle(
                              fontSize: 10.5,
                              color: AppTheme.accentPrimary,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: context.l10n.entityRemoveFact,
                    icon: Icon(
                      Icons.close,
                      size: 15,
                      color: AppTheme.textTertiary,
                    ),
                    onPressed: () => _deleteFact(context, ref, fact.id),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }

  Set<String> _conflictingFactIds(List<EntityFactDto> facts) {
    final groups = <String, List<EntityFactDto>>{};
    for (final fact in facts) {
      final separator = fact.factText.indexOf('：');
      final key = separator > 0
          ? fact.factText.substring(0, separator).trim()
          : fact.factText.trim();
      groups.putIfAbsent(key, () => []).add(fact);
    }
    return {
      for (final group in groups.values)
        if (group.map((fact) => fact.factText).toSet().length > 1)
          ...group.map((fact) => fact.id),
    };
  }

  String _factDate(BuildContext context, String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    return MaterialLocalizations.of(context).formatCompactDate(parsed);
  }

  String _shortId(String value) =>
      value.length > 8 ? value.substring(0, 8) : value;

  Future<void> _deleteFact(
    BuildContext context,
    WidgetRef ref,
    String id,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(dialogContext.l10n.entityRemoveFactQuestion),
        content: Text(dialogContext.l10n.entityRemoveFactHelp),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(dialogContext.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(dialogContext.l10n.entityRemove),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteEntityFact(id);
      ref.invalidate(entityFactsProvider(slug));
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(context.l10n.entityFactRemoved)));
      }
    } catch (error) {
      if (context.mounted) {
        showAppError(context, context.l10n.entityRemove, error);
      }
    }
  }

  Future<void> _showSource(
    BuildContext context,
    WidgetRef ref,
    String eventId,
  ) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final EventAnalysisDetailDto? detail;
    try {
      detail = await repo.getEventAnalysisDetail(eventId);
    } catch (error) {
      if (context.mounted) showAppError(context, context.l10n.load, error);
      return;
    }
    if (!context.mounted) return;
    if (detail == null) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(context.l10n.entitySourceMissing)));
      return;
    }
    final EventAnalysisDetailDto source = detail;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(dialogContext.l10n.entitySourceEvent),
        content: SizedBox(
          width: 520,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                ContentFontScope(child: SelectableText(source.rawText)),
                if ((source.summary ?? '').isNotEmpty) ...[
                  const SizedBox(height: 16),
                  Text(
                    dialogContext.l10n.entityAiSummary,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  ContentFontScope(child: SelectableText(source.summary!)),
                ],
                if (source.emotion?.isNotEmpty ?? false) ...[
                  const SizedBox(height: 16),
                  Text(
                    dialogContext.l10n.entityEmotionAnnotations,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  for (final tag in source.emotion!)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(
                        child: SelectableText(
                          tag.target == null || tag.target!.isEmpty
                              ? dialogContext.l10n.entityEmotionAboutEvent(
                                  tag.feeling,
                                )
                              : dialogContext.l10n.entityEmotionAboutTarget(
                                  tag.target!,
                                  tag.feeling,
                                ),
                        ),
                      ),
                    ),
                ],
                if (source.guidance != null) ...[
                  const SizedBox(height: 16),
                  Text(
                    dialogContext.l10n.entityAiGuidance,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  for (final ref in source.guidance!.related)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(
                        child: SelectableText(
                          dialogContext.l10n.entityRelatedReference(
                            ref.slug,
                            ref.note,
                          ),
                        ),
                      ),
                    ),
                  for (final lesson in source.guidance!.lessons)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(child: SelectableText(lesson)),
                    ),
                  for (final followUp in source.guidance!.followUpsRefined)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(child: SelectableText(followUp)),
                    ),
                ],
                const SizedBox(height: 16),
                Text(
                  dialogContext.l10n.entityAnalysisMetadata(
                    _factDate(dialogContext, source.recordedAt),
                    dialogContext.l10n.entityAnalysisStatus(source.jobStatus),
                  ),
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: Text(dialogContext.l10n.close),
          ),
        ],
      ),
    );
  }
}
