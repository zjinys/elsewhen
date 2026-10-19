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
  String? _error;

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
      setState(() => _error = e.toString());
    }
  }

  String _guidanceDate(String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    final m = parsed.month.toString().padLeft(2, '0');
    final d = parsed.day.toString().padLeft(2, '0');
    return '${parsed.year}-$m-$d';
  }

  @override
  Widget build(BuildContext context) {
    final items = _items;
    if (_error != null) {
      return Padding(
        padding: const EdgeInsets.only(top: AppTheme.space4),
        child: Row(
          children: [
            Text(
              '相关指导加载失败',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(width: 8),
            GestureDetector(
              onTap: () {
                setState(() {
                  _error = null;
                  _items = null;
                });
                _load();
              },
              child: Text(
                '重试',
                style: TextStyle(fontSize: 12, color: AppTheme.accentPrimary),
              ),
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
            '相关指导',
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
              child: ListTile(
                dense: true,
                title: ContentFontScope(
                  child: SelectableText(
                    item.note ?? item.lessons.firstOrNull ?? '查看指导',
                  ),
                ),
                subtitle: Text(
                  '${_guidanceDate(item.recordedAt)} - 点击查看要点',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
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
                        tooltip: '有用（再点一次清除）',
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
                        tooltip: '不相关（再点一次清除）',
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
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('反馈失败: $e')));
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
        title: const Text('相关指导'),
        content: SizedBox(
          width: 480,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '记录于 ${_guidanceDate(item.recordedAt)}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
                if (item.note?.isNotEmpty ?? false) ...[
                  const SizedBox(height: 10),
                  ContentFontScope(child: SelectableText(item.note!)),
                ],
                if (item.lessons.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  Text(
                    '可复用经验',
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
                    '待办精炼',
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
            child: const Text('关闭'),
          ),
        ],
      ),
    );
  }
}
