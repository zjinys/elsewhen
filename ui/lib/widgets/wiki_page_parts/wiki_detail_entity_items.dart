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
        showAppError(context, context.l10n.save, e);
      }
    }

    Future<void> updateStatus(String status) async {
      try {
        await repo.updateTodoStatus(workItem.id, status);
        if (!context.mounted) return;
        ref.invalidate(todosProvider);
      } catch (e) {
        if (!context.mounted) return;
        showAppError(context, context.l10n.save, e);
      }
    }

    return Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        PopupMenuButton<String>(
          tooltip: context.l10n.workItemUpdateStatus,
          onSelected: (status) => updateStatus(status),
          itemBuilder: (_) => [
            for (final status in TodoStatus.values)
              PopupMenuItem(
                value: status.wire,
                child: Text(_workItemStatus(context, status)),
              ),
          ],
          child: _WorkItemChip(
            icon: workItem.isDone
                ? Icons.check_circle
                : Icons.radio_button_unchecked,
            label: _workItemStatus(context, workItem.status),
            color: workItem.isDone ? AppTheme.success : AppTheme.accentPrimary,
          ),
        ),
        PopupMenuButton<String>(
          tooltip: context.l10n.workItemUpdatePriority,
          onSelected: (priority) => updateFields(priority: priority),
          itemBuilder: (_) => [
            PopupMenuItem(
              value: 'high',
              child: Text(context.l10n.workItemPriority('high')),
            ),
            PopupMenuItem(
              value: 'normal',
              child: Text(context.l10n.workItemPriority('normal')),
            ),
            PopupMenuItem(
              value: 'low',
              child: Text(context.l10n.workItemPriority('low')),
            ),
          ],
          child: _WorkItemChip(
            icon: Icons.flag_outlined,
            label: context.l10n.workItemPriority(workItem.priority),
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
              helpText: context.l10n.workItemChooseDue,
            );
            if (selected != null) {
              await updateFields(dueAt: selected.toIso8601String());
            }
          },
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: _WorkItemChip(
            icon: Icons.event_outlined,
            label: workItem.dueAt == null
                ? context.l10n.workItemSetDue
                : context.l10n.todoDueDate(
                    _fmtWorkItemDate(context, workItem.dueAt!),
                  ),
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

String _workItemStatus(BuildContext context, TodoStatus status) =>
    switch (status) {
      TodoStatus.open => context.l10n.todoStatusOpen,
      TodoStatus.done => context.l10n.completed,
      TodoStatus.archived => context.l10n.todoStatusArchived,
    };

String _fmtWorkItemDate(BuildContext context, String value) {
  final date = DateTime.tryParse(value)?.toLocal();
  if (date == null) return value;
  return MaterialLocalizations.of(context).formatCompactDate(date);
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
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.merge_type, size: 17, color: AppTheme.warning),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    merge == null
                        ? context.l10n.entityMerged
                        : context.l10n.entityMergedInto(merge.targetSlug),
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ),
              ],
            ),
            if (merge != null)
              Wrap(
                spacing: 8,
                runSpacing: 4,
                children: [
                  TextButton(
                    onPressed: () =>
                        _openTarget(context, ref, merge.targetSlug),
                    child: Text(context.l10n.entityViewTarget),
                  ),
                  TextButton(
                    onPressed: () => _undo(context, ref, merge.targetSlug),
                    child: Text(context.l10n.entityUndoMerge),
                  ),
                ],
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
        label: Text(context.l10n.entityMerge),
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.entityNoMergeCandidates)),
      );
      return;
    }
    String target = pages.first.slug;
    final selected = await showDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setState) => AlertDialog(
          title: Text(context.l10n.entityChooseTarget),
          content: DropdownButtonFormField<String>(
            initialValue: target,
            isExpanded: true,
            decoration: InputDecoration(
              labelText: context.l10n.entityMergeInto,
            ),
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
              child: Text(dialogContext.l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, target),
              child: Text(context.l10n.entityContinue),
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
        title: Text(dialogContext.l10n.entityMergeQuestion(targetPage.title)),
        content: Text(dialogContext.l10n.entityMergeHelp),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(dialogContext.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(dialogContext.l10n.entityConfirmMerge),
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
          SnackBar(
            content: Text(context.l10n.entityMergeSaved(targetPage.title)),
          ),
        );
      }
    } catch (error) {
      if (context.mounted) {
        showAppError(context, context.l10n.entityMerge, error);
      }
    }
  }

  Future<void> _openTarget(
    BuildContext context,
    WidgetRef ref,
    String slug,
  ) async {
    try {
      final target = await ref.read(wikiPageProvider(slug).future);
      if (context.mounted && target != null) openWikiPageTab(ref, target);
    } catch (error) {
      if (context.mounted) showAppError(context, context.l10n.load, error);
    }
  }

  Future<void> _undo(
    BuildContext context,
    WidgetRef ref,
    String targetSlug,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(dialogContext.l10n.entityUndoMergeQuestion),
        content: Text(dialogContext.l10n.entityUndoMergeHelp),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(dialogContext.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(context.l10n.entityUndoMerge),
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
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(context.l10n.entityMergeUndone)));
      }
    } catch (error) {
      if (context.mounted) {
        showAppError(context, context.l10n.entityUndoMerge, error);
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
            context.l10n.entityRelatedTodos,
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
                      context.l10n.todoDueDate(
                        _fmtWorkItemDate(context, todo.dueAt!),
                      ),
                      style: TextStyle(
                        fontSize: 10.5,
                        color: AppTheme.textTertiary,
                      ),
                    ),
              secondary: IconButton(
                tooltip: context.l10n.entityOpenWorkItem,
                icon: Icon(
                  Icons.open_in_new,
                  size: 16,
                  color: AppTheme.accentPrimary,
                ),
                onPressed: () async {
                  final repo = ref.read(
                    storageRepositoryProvider,
                  ) as RustBridgeRepository;
                  try {
                    final workItem = await repo.openTodoWorkItem(todo.id);
                    if (!context.mounted) return;
                    openWikiPageTab(ref, workItem);
                  } catch (error) {
                    if (context.mounted) {
                      showAppError(context, context.l10n.load, error);
                    }
                  }
                },
              ),
              onChanged: todo.status == TodoStatus.archived
                  ? null
                  : (_) => _toggle(context, ref, todo),
            ),
        ],
      ),
    );
  }

  Future<void> _toggle(BuildContext context, WidgetRef ref, Todo todo) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateTodoStatus(
        todo.id,
        todo.status == TodoStatus.done ? 'open' : 'done',
      );
      if (context.mounted) ref.invalidate(todosProvider);
    } catch (error) {
      if (context.mounted) showAppError(context, context.l10n.save, error);
    }
  }
}

/// 相关指导（D4 聚合视图）：guidance 里引用本页的事件分析要点。
