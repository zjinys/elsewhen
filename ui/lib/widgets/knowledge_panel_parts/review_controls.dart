part of '../knowledge_panel.dart';

//! 版本列表、意见控制与元数据编辑。

class _RevisionList extends ConsumerWidget {
  final String slug;
  final bool canRestore;
  const _RevisionList({required this.slug, this.canRestore = true});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(knowledgeRevisionsProvider(slug))
      .when(
        error: (error, _) =>
            Center(child: Text(_error(error, context: '读取修订记录'))),
        loading: () => const Center(
          child: Padding(
            padding: EdgeInsets.all(16),
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
        data: (items) => ListView(
          shrinkWrap: true,
          physics: const NeverScrollableScrollPhysics(),
          children: [
            for (final revision in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Material(
                  color: AppTheme.surface2,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: Theme(
                    data: Theme.of(context)
                        .copyWith(dividerColor: Colors.transparent),
                    child: ExpansionTile(
                      tilePadding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 2,
                      ),
                      leading: Container(
                        padding: const EdgeInsets.all(6),
                        decoration: BoxDecoration(
                          color: AppTheme.surface3,
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: Icon(
                          Icons.history,
                          size: 16,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                      title: Text(
                        revision.reason,
                        style: const TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      subtitle: Text(
                        _shortTime(revision.createdAt),
                        style: TextStyle(
                          fontSize: 11.5,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                      children: [
                        Container(
                          width: double.infinity,
                          padding: const EdgeInsets.all(12),
                          margin: const EdgeInsets.fromLTRB(10, 0, 10, 8),
                          decoration: BoxDecoration(
                            color: AppTheme.surface1,
                            borderRadius: BorderRadius.circular(6),
                          ),
                          child: SelectionArea(
                            child: MarkdownView(markdown: revision.contentMd),
                          ),
                        ),
                        if (canRestore)
                          Padding(
                            padding: const EdgeInsets.only(
                              bottom: 8,
                              left: 10,
                              right: 10,
                            ),
                            child: Align(
                              alignment: Alignment.centerRight,
                              child: FilledButton.tonalIcon(
                                icon: const Icon(Icons.restore, size: 16),
                                label: const Text('生成恢复提案'),
                                style: FilledButton.styleFrom(
                                  visualDensity: VisualDensity.compact,
                                  textStyle: const TextStyle(fontSize: 12),
                                ),
                                onPressed: () async {
                                  try {
                                    await ref
                                        .read(knowledgeRepositoryProvider)
                                        .restore(slug, revision.id);
                                    if (context.mounted) {
                                      _refresh(ref, slug);
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                            const SnackBar(
                                              content: Text(
                                                '恢复提案已放入产出页，确认后生效；当前来源和规则设置保留。',
                                              ),
                                            ),
                                          );
                                    }
                                  } catch (error) {
                                    if (context.mounted) {
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                            SnackBar(
                                              content: Text(
                                                _error(
                                                  error,
                                                  context: '准备恢复',
                                                  showDetail: true,
                                                ),
                                              ),
                                            ),
                                          );
                                    }
                                  }
                                },
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
      );
}

class _OpinionControl extends ConsumerWidget {
  final WikiPage page;
  final String? opinion;
  const _OpinionControl({required this.page, this.opinion});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Row(
    children: [
      Text(
        '材料评价',
        style: TextStyle(
          fontSize: 12.5,
          fontWeight: FontWeight.w600,
          color: AppTheme.textSecondary,
        ),
      ),
      const SizedBox(width: 10),
      for (final value in ['endorse', 'reject']) ...[
        ChoiceChip(
          label: Text(value == 'endorse' ? '认可' : '不认可'),
          avatar: Icon(
            value == 'endorse'
                ? (opinion == value ? Icons.thumb_up : Icons.thumb_up_outlined)
                : (opinion == value
                      ? Icons.thumb_down
                      : Icons.thumb_down_outlined),
            size: 14,
            color: opinion == value
                ? (value == 'endorse' ? AppTheme.accentPrimary : AppTheme.error)
                : AppTheme.textTertiary,
          ),
          visualDensity: VisualDensity.compact,
          labelStyle: TextStyle(
            fontSize: 12,
            fontWeight: opinion == value ? FontWeight.w600 : FontWeight.w400,
            color: opinion == value
                ? (value == 'endorse' ? AppTheme.accentPrimary : AppTheme.error)
                : AppTheme.textSecondary,
          ),
          selectedColor: value == 'endorse'
              ? AppTheme.accentPrimary.withValues(alpha: 0.15)
              : AppTheme.error.withValues(alpha: 0.15),
          backgroundColor: AppTheme.surface2,
          selected: opinion == value,
          onSelected: (_) async {
            try {
              await api.setWikiOpinion(
                slug: page.slug,
                opinion: opinion == value ? null : value,
              );
              if (context.mounted) _refresh(ref, page.slug);
            } catch (error) {
              if (context.mounted) {
                await showKnowledgeText(
                  context,
                  '操作失败',
                  _error(error, context: '记录观点'),
                );
              }
            }
          },
        ),
        const SizedBox(width: 8),
      ],
    ],
  );
}

class _MetadataEditor extends ConsumerStatefulWidget {
  final String slug;
  final api.KnowledgeMetadata metadata;
  const _MetadataEditor({required this.slug, required this.metadata});
  @override
  ConsumerState<_MetadataEditor> createState() => _MetadataEditorState();
}

class _MetadataEditorState extends ConsumerState<_MetadataEditor> {
  late final _conditions = TextEditingController(
    text: widget.metadata.applicableWhen,
  );
  late String _strength = widget.metadata.strength;
  bool _busy = false;
  String? _failure;
  @override
  void dispose() {
    _conditions.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_strength == 'rule' && widget.metadata.strength != 'rule') {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('确认作为规则使用？'),
          content: const Text('AI 会在符合适用条件时对照这条规则。请确认内容可靠且适用条件准确。'),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('确认升级'),
            ),
          ],
        ),
      );
      if (confirmed != true || !mounted) return;
    }
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .metadata(widget.slug, _conditions.text, _strength);
      if (mounted) _refresh(ref, widget.slug);
    } catch (error) {
      if (mounted) setState(() => _failure = _error(error, context: '保存元数据'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        TextField(
          controller: _conditions,
          maxLines: 3,
          maxLength: 600,
          decoration: const InputDecoration(labelText: '适用条件'),
        ),
        DropdownButton<String>(
          value: _strength,
          onChanged: _busy
              ? null
              : (value) => setState(() => _strength = value!),
          items: const [
            DropdownMenuItem(value: 'reference', child: Text('参考材料')),
            DropdownMenuItem(value: 'method', child: Text('可复用方法')),
            DropdownMenuItem(value: 'rule', child: Text('已确认规则')),
          ],
        ),
        if (_failure != null) Text(_failure!),
        FilledButton(
          onPressed: _busy ? null : _save,
          child: const Text('保存适用条件与强度'),
        ),
      ],
    ),
  );
}

/// 打开全局「知识审阅」对话框（待确认修改 + 来源检查 + 队列 + 记录）。
/// 供首页入口与按钮共用。
void showKnowledgeMaintenance(BuildContext context) => showDialog<void>(
  context: context,
  builder: (_) => const _MaintenanceDialog(),
);
