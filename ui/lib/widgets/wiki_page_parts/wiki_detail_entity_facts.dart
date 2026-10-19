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
            '结构化事实',
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
                      '发现可能冲突的事实。历史来源均已保留，请查看来源后移除错误的派生事实。',
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
                            '${_factDate(fact.occurredAt)} · 置信度 ${fact.confidence}/5 · 查看来源 ${_shortId(fact.sourceEventId)}',
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
                    tooltip: '纠正：移除此事实',
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

  String _factDate(String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    return '${parsed.year}-${parsed.month.toString().padLeft(2, '0')}-${parsed.day.toString().padLeft(2, '0')}';
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
        title: const Text('移除这条事实？'),
        content: const Text('只会移除知识页中的派生事实，不会删除原始事件。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('移除'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.deleteEntityFact(id);
    ref.invalidate(entityFactsProvider(slug));
    if (context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('事实已移除，原始事件仍保留')));
    }
  }

  Future<void> _showSource(
    BuildContext context,
    WidgetRef ref,
    String eventId,
  ) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final detail = await repo.getEventAnalysisDetail(eventId);
    if (!context.mounted) return;
    if (detail == null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('来源事件不存在')));
      return;
    }
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('来源事件'),
        content: SizedBox(
          width: 520,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                ContentFontScope(child: SelectableText(detail.rawText)),
                if ((detail.summary ?? '').isNotEmpty) ...[
                  const SizedBox(height: 16),
                  Text(
                    'AI 摘要',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  ContentFontScope(child: SelectableText(detail.summary!)),
                ],
                if (detail.emotion?.isNotEmpty ?? false) ...[
                  const SizedBox(height: 16),
                  Text(
                    '情绪标注（文本，无数值）',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  for (final tag in detail.emotion!)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(
                        child: SelectableText(
                          tag.target == null || tag.target!.isEmpty
                              ? '对这件事：${tag.feeling}'
                              : '对 ${tag.target}：${tag.feeling}',
                        ),
                      ),
                    ),
                ],
                if (detail.guidance != null) ...[
                  const SizedBox(height: 16),
                  Text(
                    'AI 指导（派生层）',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  for (final ref in detail.guidance!.related)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(
                        child: SelectableText('相关：${ref.slug} — ${ref.note}'),
                      ),
                    ),
                  for (final lesson in detail.guidance!.lessons)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(child: SelectableText(lesson)),
                    ),
                  for (final followUp in detail.guidance!.followUpsRefined)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: ContentFontScope(child: SelectableText(followUp)),
                    ),
                ],
                const SizedBox(height: 16),
                Text(
                  '记录于 ${_factDate(detail.recordedAt)} · 分析状态 ${detail.jobStatus}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('关闭'),
          ),
        ],
      ),
    );
  }
}
