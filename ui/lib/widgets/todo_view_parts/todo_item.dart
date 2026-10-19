part of '../todo_view.dart';

//! 待办条目（勾选/编辑/删除 + 展开详情）

class _TodoItem extends StatelessWidget {
  final Todo todo;
  final VoidCallback onToggle;
  final VoidCallback onDelete;
  final VoidCallback onEdit;
  final VoidCallback? onOpenWiki;
  final VoidCallback? onOpenWorkItem;

  const _TodoItem({
    required this.todo,
    required this.onToggle,
    required this.onDelete,
    required this.onEdit,
    this.onOpenWiki,
    this.onOpenWorkItem,
  });

  @override
  Widget build(BuildContext context) {
    final priorityColor = switch (todo.priority) {
      'high' => AppTheme.error,
      'low' => AppTheme.textTertiary,
      _ => AppTheme.accentPrimary,
    };
    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onOpenWorkItem ?? onToggle,
        child: Container(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space3,
            AppTheme.space2,
            AppTheme.space2,
            AppTheme.space2,
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // 勾选框
              Padding(
                padding: const EdgeInsets.only(top: 1),
                child: InkWell(
                  onTap: onToggle,
                  borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                  child: Padding(
                    padding: const EdgeInsets.all(2),
                    child: Icon(
                      todo.isDone
                          ? Icons.check_circle
                          : Icons.radio_button_unchecked,
                      size: 18,
                      color: todo.isDone
                          ? AppTheme.accentPrimary
                          : AppTheme.textTertiary,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: AppTheme.space2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        if (todo.priority != 'normal') ...[
                          Container(
                            width: 6,
                            height: 6,
                            decoration: BoxDecoration(
                              color: priorityColor,
                              shape: BoxShape.circle,
                            ),
                          ),
                          const SizedBox(width: 4),
                        ],
                        Expanded(
                          child: Text(
                            todo.title,
                            style: TextStyle(
                              color: AppTheme.textPrimary,
                              fontSize: 13.5,
                              fontWeight: FontWeight.w500,
                              decoration: todo.isDone
                                  ? TextDecoration.lineThrough
                                  : null,
                              decorationColor: AppTheme.textTertiary,
                            ),
                          ),
                        ),
                      ],
                    ),
                    if ((todo.dueAt != null && todo.dueAt!.isNotEmpty) ||
                        (todo.note != null && todo.note!.isNotEmpty) ||
                        onOpenWiki != null) ...[
                      const SizedBox(height: 2),
                      Row(
                        children: [
                          if (onOpenWiki != null)
                            InkWell(
                              onTap: onOpenWiki,
                              child: Padding(
                                padding: const EdgeInsets.only(right: 8),
                                child: Row(
                                  children: [
                                    Icon(
                                      Icons.menu_book_outlined,
                                      size: 12,
                                      color: AppTheme.accentPrimary,
                                    ),
                                    const SizedBox(width: 3),
                                    Text(
                                      '知识页',
                                      style: TextStyle(
                                        fontSize: 11,
                                        color: AppTheme.accentPrimary,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          if (todo.reminder)
                            Padding(
                              padding: const EdgeInsets.only(right: 8),
                              child: Icon(Icons.alarm,
                                  size: 13, color: AppTheme.accentPrimary),
                            ),
                          if (todo.dueAt != null && todo.dueAt!.isNotEmpty)
                            Text(
                              '${todo.reminder ? "提醒" : "截止"} ${_fmtDate(todo.dueAt!)}',
                              style: TextStyle(
                                fontSize: 11,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                          if (todo.note != null && todo.note!.isNotEmpty)
                            Expanded(
                              child: Text(
                                todo.note!,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 11,
                                  color: AppTheme.textTertiary,
                                ),
                              ),
                            ),
                        ],
                      ),
                    ],
                  ],
                ),
              ),
              PopupMenuButton<String>(
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                splashRadius: 16,
                icon: Icon(
                  Icons.more_horiz,
                  size: 16,
                  color: AppTheme.textTertiary,
                ),
                color: AppTheme.surface2,
                onSelected: (action) {
                  switch (action) {
                    case 'edit':
                      onEdit();
                    case 'delete':
                      onDelete();
                  }
                },
                itemBuilder: (context) => [
                  PopupMenuItem(
                    value: 'edit',
                    child: Row(
                      children: [
                        Icon(
                          Icons.edit_outlined,
                          size: 16,
                          color: AppTheme.accentPrimary,
                        ),
                        const SizedBox(width: AppTheme.space2),
                        Text(
                          '编辑',
                          style: TextStyle(color: AppTheme.textPrimary),
                        ),
                      ],
                    ),
                  ),
                  PopupMenuItem(
                    value: 'delete',
                    child: Row(
                      children: [
                        Icon(
                          Icons.delete_outline,
                          size: 16,
                          color: AppTheme.error,
                        ),
                        const SizedBox(width: AppTheme.space2),
                        Text(
                          '删除',
                          style: TextStyle(color: AppTheme.textPrimary),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _fmtDate(String iso) {
    final t = DateTime.tryParse(iso);
    if (t == null) return iso;
    final l = t.toLocal();
    return '${l.month}-${l.day.toString().padLeft(2, '0')}';
  }
}
