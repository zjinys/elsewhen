import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../bridge/rust_bridge_repository.dart';
import '../models/todo.dart';
import '../providers/todo_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';

/// 侧栏「待办」tab：新建/勾选/删除待办 + 按状态分组
class TodoListView extends ConsumerStatefulWidget {
  const TodoListView({super.key});

  @override
  ConsumerState<TodoListView> createState() => _TodoListViewState();
}

class _TodoListViewState extends ConsumerState<TodoListView> {
  final _inputController = TextEditingController();
  final _noteController = TextEditingController();
  bool _saving = false;

  @override
  void dispose() {
    _inputController.dispose();
    _noteController.dispose();
    super.dispose();
  }

  Future<void> _add() async {
    final title = _inputController.text.trim();
    if (title.isEmpty || _saving) return;
    setState(() => _saving = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final note = _noteController.text.trim();
      await repo.createTodo(
        title: title,
        note: note.isEmpty ? null : note,
      );
      if (!mounted) return;
      _inputController.clear();
      _noteController.clear();
      ref.invalidate(todosProvider);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(e.toString().replaceFirst('Exception: ', '')),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _toggle(Todo todo) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateTodoStatus(
        todo.id,
        todo.isDone ? TodoStatus.open.wire : TodoStatus.done.wire,
      );
      if (!mounted) return;
      ref.invalidate(todosProvider);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(e.toString()), behavior: SnackBarBehavior.floating),
      );
    }
  }

  Future<void> _delete(Todo todo) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final ok = await repo.deleteTodo(todo.id);
    if (ok && mounted) ref.invalidate(todosProvider);
  }

  /// 编辑待办：标题 / 说明 / 优先级 / 截止时间（截止留空即清除）
  Future<void> _edit(Todo todo) async {
    final saved = await showDialog<bool>(
      context: context,
      builder: (dialogContext) =>
          _TodoEditDialog(todo: todo),
    );
    if (saved == true && mounted) ref.invalidate(todosProvider);
  }

  @override
  Widget build(BuildContext context) {
    final todosAsync = ref.watch(todosProvider);
    return Column(
      children: [
        // 新建输入
        Padding(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space3,
            AppTheme.space2,
            AppTheme.space3,
            AppTheme.space1,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              TextField(
                controller: _inputController,
                enabled: !_saving,
                onSubmitted: (_) => _add(),
                decoration: InputDecoration(
                  hintText: '记一条要跟进的事…（回车添加）',
                  hintStyle: TextStyle(
                    fontSize: 12.5,
                    color: AppTheme.textTertiary,
                  ),
                  prefixIcon: const Icon(Icons.add_task, size: 16),
                  prefixIconConstraints: const BoxConstraints(
                    minWidth: 32,
                    minHeight: 32,
                  ),
                  isDense: true,
                  contentPadding: const EdgeInsets.symmetric(vertical: 8),
                  filled: true,
                  fillColor: AppTheme.surface2,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                  focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    borderSide: BorderSide(
                      color: AppTheme.accentPrimary,
                      width: 1.5,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: AppTheme.space1),
              TextField(
                controller: _noteController,
                enabled: !_saving,
                decoration: InputDecoration(
                  hintText: '补充说明（可选）',
                  hintStyle: TextStyle(
                    fontSize: 12,
                    color: AppTheme.textTertiary,
                  ),
                  isDense: true,
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 6,
                  ),
                  filled: true,
                  fillColor: AppTheme.surface1,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: todosAsync.when(
            data: (todos) {
              if (todos.isEmpty) {
                return const _EmptyHint(
                  icon: Icons.fact_check_outlined,
                  message: '还没有待办\n\n可以手动添加，或在对话里让 AI 分析出\n需要跟进的事（需确认后创建）',
                );
              }
              final open = todos.where((t) => !t.isDone).toList()
                ..sort((a, b) => a.priorityRank.compareTo(b.priorityRank));
              final done = todos.where((t) => t.isDone).toList();
              return ListView(
                padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
                children: [
                  if (open.isNotEmpty)
                    _buildGroup('进行中 · ${open.length}', open),
                  if (done.isNotEmpty)
                    _buildGroup('已完成 · ${done.length}', done),
                ],
              );
            },
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (error, stack) => Center(
              child: Text(
                '加载失败',
                style: TextStyle(color: AppTheme.textSecondary),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildGroup(String title, List<Todo> todos) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space3,
            AppTheme.space2,
            AppTheme.space3,
            AppTheme.space1,
          ),
          child: Text(
            title,
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w600,
              letterSpacing: 0.5,
              color: AppTheme.accentMuted,
            ),
          ),
        ),
        for (final todo in todos) _TodoItem(
          todo: todo,
          onToggle: () => _toggle(todo),
          onDelete: () => _delete(todo),
          onEdit: () => _edit(todo),
          onOpenWiki: todo.relatedWikiSlug == null
              ? null
              : () {
                  final repo = ref
                      .read(storageRepositoryProvider) as RustBridgeRepository;
                  repo.getWikiPage(todo.relatedWikiSlug!).then((page) {
                    if (page == null || !mounted) return;
                    ref.read(sidebarTabProvider.notifier).state =
                        SidebarTab.wiki;
                    openWikiPageTab(ref, page);
                });
              },
          onOpenWorkItem: () async {
            final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
            final page = await repo.openTodoWorkItem(todo.id);
            if (!mounted) return;
            ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
            openWikiPageTab(ref, page);
            ref.invalidate(todosProvider);
          },
        ),
      ],
    );
  }
}

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
                                    Icon(Icons.menu_book_outlined,
                                        size: 12, color: AppTheme.accentPrimary),
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
                          if (todo.dueAt != null && todo.dueAt!.isNotEmpty)
                            Text(
                              '截止 ${_fmtDate(todo.dueAt!)}',
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
                icon: Icon(Icons.more_horiz, size: 16, color: AppTheme.textTertiary),
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
                        Icon(Icons.edit_outlined, size: 16, color: AppTheme.accentPrimary),
                        const SizedBox(width: AppTheme.space2),
                        Text('编辑', style: TextStyle(color: AppTheme.textPrimary)),
                      ],
                    ),
                  ),
                  PopupMenuItem(
                    value: 'delete',
                    child: Row(
                      children: [
                        Icon(Icons.delete_outline, size: 16, color: AppTheme.error),
                        const SizedBox(width: AppTheme.space2),
                        Text('删除', style: TextStyle(color: AppTheme.textPrimary)),
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

/// 待办编辑对话框：修改标题 / 说明 / 优先级 / 截止（截止留空即清除）
class _TodoEditDialog extends ConsumerStatefulWidget {
  final Todo todo;

  const _TodoEditDialog({required this.todo});

  @override
  ConsumerState<_TodoEditDialog> createState() => _TodoEditDialogState();
}

class _TodoEditDialogState extends ConsumerState<_TodoEditDialog> {
  late final TextEditingController _title;
  late final TextEditingController _note;
  late final TextEditingController _due;
  late String _priority;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _title = TextEditingController(text: widget.todo.title);
    _note = TextEditingController(text: widget.todo.note ?? '');
    _due = TextEditingController(text: widget.todo.dueAt ?? '');
    _priority = widget.todo.priority;
  }

  @override
  void dispose() {
    _title.dispose();
    _note.dispose();
    _due.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final title = _title.text.trim();
    if (title.isEmpty || _saving) return;
    setState(() => _saving = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final due = _due.text.trim();
      await repo.updateTodo(
        id: widget.todo.id,
        title: title,
        note: _note.text.trim().isEmpty ? null : _note.text.trim(),
        priority: _priority,
        dueAt: due.isEmpty ? null : due,
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _saving = false);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(e.toString().replaceFirst('Exception: ', '')),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.surface1,
      title: Text('编辑待办', style: TextStyle(color: AppTheme.textPrimary, fontSize: 16)),
      content: SizedBox(
        width: 360,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: _title,
              autofocus: true,
              style: TextStyle(color: AppTheme.textPrimary),
              cursorColor: AppTheme.accentPrimary,
              decoration: _inputDecoration('要跟进的事…'),
            ),
            const SizedBox(height: AppTheme.space2),
            TextField(
              controller: _note,
              style: TextStyle(color: AppTheme.textPrimary),
              cursorColor: AppTheme.accentPrimary,
              decoration: _inputDecoration('补充说明（可选）'),
            ),
            const SizedBox(height: AppTheme.space2),
            TextField(
              controller: _due,
              style: TextStyle(color: AppTheme.textPrimary),
              cursorColor: AppTheme.accentPrimary,
              decoration: _inputDecoration(
                  '截止日期（可选，YYYY-MM-DD，留空清除）'),
            ),
            const SizedBox(height: AppTheme.space3),
            Text('优先级',
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary)),
            const SizedBox(height: AppTheme.space1),
            Row(
              children: [
                for (final p in const [
                  ('high', '高'),
                  ('normal', '中'),
                  ('low', '低'),
                ])
                  Padding(
                    padding: const EdgeInsets.only(right: AppTheme.space2),
                    child: ChoiceChip(
                      label: Text(p.$2,
                          style: TextStyle(
                            fontSize: 12,
                            color: _priority == p.$1
                                ? Colors.black
                                : AppTheme.textSecondary,
                          )),
                      selected: _priority == p.$1,
                      showCheckmark: false,
                      selectedColor: AppTheme.accentPrimary,
                      backgroundColor: AppTheme.surface2,
                      side: BorderSide(color: AppTheme.surface3),
                      onSelected: (_) => setState(() => _priority = p.$1),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _saving
              ? null
              : () => Navigator.of(context).pop(false),
          child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
        ),
        FilledButton(
          style: FilledButton.styleFrom(backgroundColor: AppTheme.accentPrimary),
          onPressed: _saving ? null : _save,
          child: _saving
              ? const SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('保存'),
        ),
      ],
    );
  }

  InputDecoration _inputDecoration(String hint) => InputDecoration(
        hintText: hint,
        hintStyle: TextStyle(color: AppTheme.textTertiary),
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
        filled: true,
        fillColor: AppTheme.surface2,
        enabledBorder: OutlineInputBorder(
          borderSide: BorderSide(color: AppTheme.surface3),
        ),
        focusedBorder: OutlineInputBorder(
          borderSide: BorderSide(color: AppTheme.accentPrimary),
        ),
      );
}

class _EmptyHint extends StatelessWidget {
  final IconData icon;
  final String message;

  const _EmptyHint({required this.icon, required this.message});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space6),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 48, color: AppTheme.textTertiary),
            const SizedBox(height: AppTheme.space3),
            Text(
              message,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 13,
                height: 1.6,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
