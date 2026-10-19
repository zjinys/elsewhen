part of '../knowledge_maintenance_sections.dart';

//! 审查历史 section

class KnowledgeReviewHistorySection extends ConsumerStatefulWidget {
  final String slug;
  const KnowledgeReviewHistorySection({super.key, required this.slug});
  @override
  ConsumerState<KnowledgeReviewHistorySection> createState() =>
      _ReviewHistoryState();
}

class _ReviewHistoryState extends ConsumerState<KnowledgeReviewHistorySection> {
  final List<api.KnowledgeReviewRecord> _items = [];
  bool _busy = false, _loaded = false, _more = false;
  String? _error;
  Future<void> _load({bool reset = false}) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await ref
          .read(knowledgeRepositoryProvider)
          .history(widget.slug, reset ? 0 : _items.length);
      if (!mounted) return;
      setState(() {
        if (reset) _items.clear();
        _items.addAll(items);
        _loaded = true;
        _more = items.length == 50;
      });
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取处理历史'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _text(String title, String? content) => content == null
      ? const SizedBox.shrink()
      : ExpansionTile(
          dense: true,
          tilePadding: EdgeInsets.zero,
          title: Text(
            title,
            style: TextStyle(
              fontSize: 12.5,
              fontWeight: FontWeight.w600,
              color: AppTheme.textSecondary,
            ),
          ),
          children: [
            Padding(
              padding: const EdgeInsets.only(bottom: 10),
              child: SelectionArea(child: MarkdownView(markdown: content)),
            ),
          ],
        );

  /// 「标签 + 值」字段块：小灰标签在上、正文在下，用于处理说明 / 适用条件等。
  /// 替代原「标签：值」同行混排，层级更清。mono 用于修订 ID 这类等宽标识。
  Widget _field(String label, String value, {bool mono = false}) => Padding(
    padding: const EdgeInsets.only(top: 10),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            letterSpacing: 0.3,
            color: AppTheme.textTertiary,
          ),
        ),
        const SizedBox(height: 3),
        Text(
          value,
          style: TextStyle(
            fontSize: 12.5,
            height: 1.5,
            color: AppTheme.textSecondary,
            fontFamily: mono ? 'monospace' : null,
          ),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) => Container(
    margin: const EdgeInsets.only(top: 8, bottom: 10),
    child: Material(
      color: AppTheme.surface1,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(10),
        side: BorderSide(color: AppTheme.surface3),
      ),
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
          tilePadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          leading: Container(
            padding: const EdgeInsets.all(7),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(
              Icons.history_toggle_off_outlined,
              size: 16,
              color: AppTheme.accentPrimary,
            ),
          ),
          title: const Text(
            '知识演进',
            style: TextStyle(fontSize: 13.5, fontWeight: FontWeight.w600),
          ),
          subtitle: Padding(
            padding: const EdgeInsets.only(top: 2),
            child: Text(
              '这页内容被建议与修订的历史（采纳 / 拒绝 / 关联解决）',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ),
          onExpansionChanged: (open) {
            if (open && !_busy) _load(reset: true);
          },
          children: [
            if (_busy) const LinearProgressIndicator(),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(14, 0, 14, 8),
                child: Text(_error!),
              ),
            if (_loaded && _items.isEmpty)
              const Padding(
                padding: EdgeInsets.fromLTRB(14, 4, 14, 12),
                child: Text('这页还没有过修订记录。'),
              ),
            for (final item in _items)
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 0, 12, 10),
                child: Material(
                  color: AppTheme.surface2,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  child: Theme(
                    data: Theme.of(context)
                        .copyWith(dividerColor: Colors.transparent),
                    child: ExpansionTile(
                      key: ValueKey('review-${item.id}'),
                      tilePadding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 2,
                      ),
                      title: Text(
                        item.title,
                        style: const TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      subtitle: Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text(
                          '${switch (item.action) {
                            'accepted' => '已采纳',
                            'rejected' => '已拒绝',
                            'resolved' => '已关联解决',
                            'dismissed' => '已忽略',
                            _ => item.action,
                          }} · ${_shortTime(item.createdAt)}',
                          style: TextStyle(
                            fontSize: 11.5,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      ),
                      children: [
                        Padding(
                          padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                          child: Material(
                            color: AppTheme.surface1,
                            shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(8),
                              side: BorderSide(color: AppTheme.surface3),
                            ),
                            child: Padding(
                              padding: const EdgeInsets.all(12),
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  if (item.description.isNotEmpty)
                                    Text(
                                      item.description,
                                      style: TextStyle(
                                        fontSize: 13,
                                        height: 1.5,
                                        color: AppTheme.textPrimary,
                                      ),
                                    ),
                                  if (item.note != null)
                                    _field('处理说明', item.note!),
                                  if (item.selectedParts.isNotEmpty)
                                    _field(
                                      '采纳的差异段',
                                      item.selectedParts
                                          .map((n) => n.toInt() + 1)
                                          .join('、'),
                                    ),
                                  if (item.originalApplicable != null)
                                    _field('建议适用条件', item.originalApplicable!),
                                  if (item.resultApplicable != null)
                                    _field('保存的适用条件', item.resultApplicable!),
                                  if (item.revisionId != null)
                                    _field(
                                      '关联修订',
                                      item.revisionId!,
                                      mono: true,
                                    ),
                                  const SizedBox(height: 6),
                                  _text('修改前正文', item.beforeContent),
                                  _text('原始建议', item.originalContent),
                                  _text('实际保存的正文', item.resultContent),
                                  if (item.targetSlug != null)
                                    Padding(
                                      padding: const EdgeInsets.only(top: 6),
                                      child: Align(
                                        alignment: Alignment.centerLeft,
                                        child: TextButton.icon(
                                          onPressed: () async {
                                            final page = await ref.read(
                                              wikiPageProvider(item.targetSlug!)
                                                  .future,
                                            );
                                            if (mounted && page != null)
                                              openWikiPageTab(ref, page);
                                          },
                                          icon: const Icon(
                                            Icons.open_in_new,
                                            size: 14,
                                          ),
                                          label: const Text(
                                            '打开对应知识页',
                                            style: TextStyle(fontSize: 12),
                                          ),
                                        ),
                                      ),
                                    ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            if (_more || _error != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: TextButton(
                  onPressed: _busy ? null : () => _load(),
                  child: Text(_error != null ? '重新读取' : '加载更多记录'),
                ),
              ),
          ],
        ),
      ),
    ),
  );
}
