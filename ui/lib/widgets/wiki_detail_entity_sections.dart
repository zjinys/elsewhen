part of 'wiki_page_detail_view.dart';

class _WorkItemPanel extends ConsumerWidget {
  final WikiPage page;

  const _WorkItemPanel({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final todos = ref.watch(todosProvider).value ?? const <Todo>[];
    Todo? todo;
    for (final item in todos) {
      if (item.relatedWikiSlug == page.slug) {
        todo = item;
        break;
      }
    }
    if (todo == null) return const SizedBox.shrink();
    final workItem = todo;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;

    Future<void> updateFields({String? priority, String? dueAt}) async {
      // FFI 失败不能静默丢弃：SnackBar 反馈，成功后刷新（ConsumerWidget 无
      // State.mounted，用 context.mounted 守住 post-await 的 context 使用）。
      try {
        await repo.updateTodo(
          id: workItem.id,
          title: workItem.title,
          note: workItem.note,
          priority: priority ?? workItem.priority,
          dueAt: dueAt ?? workItem.dueAt,
        );
        if (!context.mounted) return;
        ref.invalidate(todosProvider);
      } catch (e) {
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('更新待办失败：${e.toString()}'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    }

    Future<void> updateStatus(String status) async {
      try {
        await repo.updateTodoStatus(workItem.id, status);
        if (!context.mounted) return;
        ref.invalidate(todosProvider);
      } catch (e) {
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('更新状态失败：${e.toString()}'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    }

    return Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        PopupMenuButton<String>(
          tooltip: '更新状态',
          onSelected: (status) => updateStatus(status),
          itemBuilder: (_) => [
            for (final status in TodoStatus.values)
              PopupMenuItem(value: status.wire, child: Text(status.label)),
          ],
          child: _WorkItemChip(
            icon: workItem.isDone
                ? Icons.check_circle
                : Icons.radio_button_unchecked,
            label: workItem.status.label,
            color: workItem.isDone ? AppTheme.success : AppTheme.accentPrimary,
          ),
        ),
        PopupMenuButton<String>(
          tooltip: '更新优先级',
          onSelected: (priority) => updateFields(priority: priority),
          itemBuilder: (_) => const [
            PopupMenuItem(value: 'high', child: Text('高优先级')),
            PopupMenuItem(value: 'normal', child: Text('普通优先级')),
            PopupMenuItem(value: 'low', child: Text('低优先级')),
          ],
          child: _WorkItemChip(
            icon: Icons.flag_outlined,
            label: workItem.priority == 'high'
                ? '高优先级'
                : workItem.priority == 'low'
                ? '低优先级'
                : '普通优先级',
            color: AppTheme.textSecondary,
          ),
        ),
        InkWell(
          onTap: () async {
            final now = DateTime.now();
            final selected = await showDatePicker(
              context: context,
              initialDate: workItem.dueAt == null
                  ? now
                  : DateTime.tryParse(workItem.dueAt!) ?? now,
              firstDate: DateTime(now.year - 1),
              lastDate: DateTime(now.year + 5),
              helpText: '选择截止日期',
            );
            if (selected != null) {
              await updateFields(dueAt: selected.toIso8601String());
            }
          },
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: _WorkItemChip(
            icon: Icons.event_outlined,
            label: workItem.dueAt == null
                ? '设置截止日期'
                : '截止 ${_fmtWorkItemDate(workItem.dueAt!)}',
            color: AppTheme.textSecondary,
          ),
        ),
      ],
    );
  }
}

class _WorkItemChip extends StatelessWidget {
  final IconData icon;
  final String label;
  final Color color;

  const _WorkItemChip({
    required this.icon,
    required this.label,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 14, color: color),
          const SizedBox(width: 5),
          Text(label, style: TextStyle(fontSize: 11, color: color)),
        ],
      ),
    );
  }
}

String _fmtWorkItemDate(String value) {
  final date = DateTime.tryParse(value)?.toLocal();
  if (date == null) return value;
  return '${date.year}-${date.month.toString().padLeft(2, '0')}-${date.day.toString().padLeft(2, '0')}';
}

class _EntityMergeControls extends ConsumerWidget {
  final WikiPage page;
  const _EntityMergeControls({required this.page});

  bool get _isEntity =>
      const ['person', 'project', 'topic'].contains(page.kind);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!_isEntity) return const SizedBox.shrink();
    final merge = ref.watch(entityMergeStatusProvider(page.slug)).value;
    if (merge != null || page.status == 'merged') {
      return Container(
        width: double.infinity,
        margin: const EdgeInsets.only(bottom: AppTheme.space4),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: AppTheme.warning.withValues(alpha: .08),
          border: Border.all(color: AppTheme.warning.withValues(alpha: .3)),
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        ),
        child: Row(
          children: [
            Icon(Icons.merge_type, size: 17, color: AppTheme.warning),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                merge == null ? '此实体已合并' : '此实体已合并到 ${merge.targetSlug}',
                style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
              ),
            ),
            if (merge != null)
              TextButton(
                onPressed: () => _openTarget(ref, merge.targetSlug),
                child: const Text('查看目标'),
              ),
            if (merge != null)
              TextButton(
                onPressed: () => _undo(context, ref, merge.targetSlug),
                child: const Text('撤销合并'),
              ),
          ],
        ),
      );
    }
    return Align(
      alignment: Alignment.centerRight,
      child: TextButton.icon(
        onPressed: () => _merge(context, ref),
        icon: const Icon(Icons.merge_type, size: 15),
        label: const Text('合并实体'),
      ),
    );
  }

  Future<void> _merge(BuildContext context, WidgetRef ref) async {
    final pages = (await ref.read(wikiPagesProvider.future))
        .where(
          (candidate) =>
              candidate.kind == page.kind &&
              candidate.slug != page.slug &&
              candidate.status != 'merged',
        )
        .toList();
    if (!context.mounted) return;
    if (pages.isEmpty) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('没有可合并的同类型实体')));
      return;
    }
    String target = pages.first.slug;
    final selected = await showDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setState) => AlertDialog(
          title: const Text('选择目标实体'),
          content: DropdownButtonFormField<String>(
            initialValue: target,
            isExpanded: true,
            decoration: const InputDecoration(labelText: '合并到'),
            items: [
              for (final candidate in pages)
                DropdownMenuItem(
                  value: candidate.slug,
                  child: Text(
                    '${candidate.title}  ·  ${candidate.slug}',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (value) {
              if (value != null) setState(() => target = value);
            },
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, target),
              child: const Text('继续'),
            ),
          ],
        ),
      ),
    );
    if (selected == null || !context.mounted) return;
    final targetPage = pages.firstWhere(
      (candidate) => candidate.slug == selected,
    );
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('合并到“${targetPage.title}”？'),
        content: const Text(
          '事实、别名和关系会迁移到目标实体；重复内容会合并。原始事件不会改变。只要迁移后的内容没有被修改，就可以从旧实体页撤销。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('确认合并'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.mergeEntity(page.kind, page.slug, selected);
      _refresh(ref, selected);
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('已合并到“${targetPage.title}”，原始事件仍保留')),
        );
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(reportUiError('合并实体', error))));
      }
    }
  }

  Future<void> _openTarget(WidgetRef ref, String slug) async {
    final target = await ref.read(wikiPageProvider(slug).future);
    if (target != null) openWikiPageTab(ref, target);
  }

  Future<void> _undo(
    BuildContext context,
    WidgetRef ref,
    String targetSlug,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('撤销这次合并？'),
        content: const Text(
          '只恢复本次合并迁移的事实、别名和关系。若这些内容合并后已被修改，系统会拒绝操作以保护新数据。原始事件不会改变。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('撤销合并'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.undoEntityMerge(page.slug);
      _refresh(ref, targetSlug);
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text('合并已撤销，实体内容已恢复')));
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(reportUiError('撤销合并', error))));
      }
    }
  }

  void _refresh(WidgetRef ref, String targetSlug) {
    for (final slug in [page.slug, targetSlug]) {
      ref.invalidate(wikiPageProvider(slug));
      ref.invalidate(entityFactsProvider(slug));
      ref.invalidate(entityAliasesProvider(slug));
      ref.invalidate(pageRelationsProvider(slug));
      ref.invalidate(entityMergeStatusProvider(slug));
    }
    ref.invalidate(wikiPagesProvider);
  }
}

class _EntityRelatedTodos extends ConsumerWidget {
  final WikiPage page;
  const _EntityRelatedTodos({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!const ['person', 'project', 'topic'].contains(page.kind)) {
      return const SizedBox.shrink();
    }
    final todos = (ref.watch(todosProvider).value ?? const <Todo>[])
        .where((todo) => todo.relatedWikiSlug == page.slug)
        .toList();
    if (todos.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '关联待办',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textTertiary,
            ),
          ),
          const SizedBox(height: 6),
          for (final todo in todos)
            CheckboxListTile(
              value: todo.status == TodoStatus.done,
              dense: true,
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              title: Text(
                todo.title,
                style: TextStyle(
                  fontSize: 12,
                  decoration: todo.status == TodoStatus.done
                      ? TextDecoration.lineThrough
                      : null,
                  color: todo.status == TodoStatus.done
                      ? AppTheme.textTertiary
                      : AppTheme.textSecondary,
                ),
              ),
              subtitle: todo.dueAt == null
                  ? null
                  : Text(
                      '截止 ${todo.dueAt}',
                      style: TextStyle(
                        fontSize: 10.5,
                        color: AppTheme.textTertiary,
                      ),
                    ),
              secondary: IconButton(
                tooltip: '打开工作项',
                icon: Icon(
                  Icons.open_in_new,
                  size: 16,
                  color: AppTheme.accentPrimary,
                ),
                onPressed: () async {
                  final repo = ref.read(
                    storageRepositoryProvider,
                  ) as RustBridgeRepository;
                  final workItem = await repo.openTodoWorkItem(todo.id);
                  if (!context.mounted) return;
                  openWikiPageTab(ref, workItem);
                },
              ),
              onChanged: todo.status == TodoStatus.archived
                  ? null
                  : (_) => _toggle(ref, todo),
            ),
        ],
      ),
    );
  }

  Future<void> _toggle(WidgetRef ref, Todo todo) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.updateTodoStatus(
      todo.id,
      todo.status == TodoStatus.done ? 'open' : 'done',
    );
    ref.invalidate(todosProvider);
  }
}

/// 相关指导（D4 聚合视图）：guidance 里引用本页的事件分析要点。
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
