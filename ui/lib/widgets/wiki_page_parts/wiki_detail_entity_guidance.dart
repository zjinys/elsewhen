part of '../wiki_page_detail_view.dart';

//! 实体引导（派生层只读）

/// 派生层只读——原始事件与页面真源都不动。
class _EntityGuidance extends ConsumerStatefulWidget {
  final String slug;
  const _EntityGuidance({required this.slug});

  @override
  ConsumerState<_EntityGuidance> createState() => _EntityGuidanceState();
}

class _EntityGuidanceState extends ConsumerState<_EntityGuidance> {
  List<PageGuidanceDto>? _items;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final items = await repo.listPageGuidance(widget.slug);
      if (!mounted) return;
      setState(() => _items = items);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e);
    }
  }

  String _guidanceDate(BuildContext context, String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    return MaterialLocalizations.of(context).formatCompactDate(parsed);
  }

  @override
  Widget build(BuildContext context) {
    final items = _items;
    if (_error != null) {
      return Padding(
        padding: const EdgeInsets.only(top: AppTheme.space4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            AppErrorMessage(action: context.l10n.load, error: _error!),
            TextButton(
              onPressed: () {
                setState(() {
                  _error = null;
                  _items = null;
                });
                _load();
              },
              child: Text(context.l10n.retry),
            ),
          ],
        ),
      );
    }
    if (items == null) {
      return const Padding(
        padding: EdgeInsets.only(top: AppTheme.space4),
        child: SizedBox(
          width: 16,
          height: 16,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      );
    }
    if (items.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            context.l10n.entityGuidance,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textTertiary,
            ),
          ),
          const SizedBox(height: 6),
          for (final item in items)
            Card(
              margin: const EdgeInsets.only(bottom: 6),
              child: Material(
                type: MaterialType.transparency,
                child: ListTile(
                  dense: true,
                  title: ContentFontScope(
                    child: SelectableText(
                      item.note ??
                          item.lessons.firstOrNull ??
                          context.l10n.entityViewGuidance,
                    ),
                  ),
                  subtitle: Text(
                    context.l10n.entityGuidanceDetails(
                      _guidanceDate(context, item.recordedAt),
                    ),
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  trailing: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      SizedBox(
                        width: 30,
                        height: 26,
                        child: IconButton(
                          padding: EdgeInsets.zero,
                          iconSize: 14,
                          tooltip: context.l10n.entityGuidanceHelpful,
                          icon: Icon(
                            item.feedback == 'helpful'
                                ? Icons.thumb_up
                                : Icons.thumb_up_outlined,
                          ),
                          color: item.feedback == 'helpful'
                              ? AppTheme.accentPrimary
                              : AppTheme.textTertiary,
                          onPressed: () => _setFeedback(item, 'helpful'),
                        ),
                      ),
                      const SizedBox(width: 2),
                      SizedBox(
                        width: 30,
                        height: 26,
                        child: IconButton(
                          padding: EdgeInsets.zero,
                          iconSize: 14,
                          tooltip: context.l10n.entityGuidanceNotRelevant,
                          icon: Icon(
                            item.feedback == 'not_relevant'
                                ? Icons.thumb_down
                                : Icons.thumb_down_outlined,
                          ),
                          color: item.feedback == 'not_relevant'
                              ? AppTheme.accentPrimary
                              : AppTheme.textTertiary,
                          onPressed: () => _setFeedback(item, 'not_relevant'),
                        ),
                      ),
                    ],
                  ),
                  onTap: () => _showGuidance(context, item),
                ),
              ),
            ),
        ],
      ),
    );
  }

  /// guidance 采纳反馈（遗留收口切片 B）：只记录不外推，无任何自动消费；
  /// 再点一次已选项即清除（null 反馈）。乐观本地更新 + 失败回滚。
  Future<void> _setFeedback(PageGuidanceDto item, String feedback) async {
    final previous = item.feedback;
    final next = previous == feedback ? null : feedback;
    final index = _items!.indexWhere((i) => i.analysisId == item.analysisId);
    setState(() {
      _items = List<PageGuidanceDto>.from(_items!);
      _items![index] = _copyWithFeedback(item, next);
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setGuidanceFeedback(
        analysisId: item.analysisId,
        slug: widget.slug,
        note: item.note,
        feedback: next,
      );
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _items = List<PageGuidanceDto>.from(_items!);
        _items![index] = _copyWithFeedback(item, previous);
      });
      if (mounted) {
        showAppError(context, context.l10n.save, e);
      }
    }
  }

  PageGuidanceDto _copyWithFeedback(PageGuidanceDto item, String? feedback) =>
      PageGuidanceDto(
        analysisId: item.analysisId,
        eventId: item.eventId,
        recordedAt: item.recordedAt,
        note: item.note,
        lessons: item.lessons,
        followUpsRefined: item.followUpsRefined,
        feedback: feedback,
      );

  /// 单条指导详情（派生层只读，不写任何真源）。
  void _showGuidance(BuildContext context, PageGuidanceDto item) {
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(dialogContext.l10n.entityGuidance),
        content: SizedBox(
          width: 480,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  dialogContext.l10n.entityRecordedOn(
                    _guidanceDate(dialogContext, item.recordedAt),
                  ),
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
                if (item.note?.isNotEmpty ?? false) ...[
                  const SizedBox(height: 10),
                  ContentFontScope(child: SelectableText(item.note!)),
                ],
                if (item.lessons.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  Text(
                    dialogContext.l10n.entityReusableLessons,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 4),
                  for (final lesson in item.lessons)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: ContentFontScope(
                        child: SelectableText('- $lesson'),
                      ),
                    ),
                ],
                if (item.followUpsRefined.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  Text(
                    dialogContext.l10n.entityRefinedTodos,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 4),
                  for (final followUp in item.followUpsRefined)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: ContentFontScope(
                        child: SelectableText('- $followUp'),
                      ),
                    ),
                ],
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
