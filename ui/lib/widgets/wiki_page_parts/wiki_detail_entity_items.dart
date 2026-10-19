part of '../wiki_page_detail_view.dart';

//! 实体工作项面板/合并控件/关联待办

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
