import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/todo.dart';
import '../providers/todo_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';

part 'todo_view_parts/todo_item.dart';
part 'todo_view_parts/todo_edit_dialog.dart';
// ── 待办视图按条目/编辑对话框拆分（纯类段平移，零行为变化）──

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
      await repo.createTodo(title: title, note: note.isEmpty ? null : note);
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
        SnackBar(
          content: Text(e.toString()),
          behavior: SnackBarBehavior.floating,
        ),
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
      builder: (dialogContext) => _TodoEditDialog(todo: todo),
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
        for (final todo in todos)
          _TodoItem(
            todo: todo,
            onToggle: () => _toggle(todo),
            onDelete: () => _delete(todo),
            onEdit: () => _edit(todo),
            onOpenWiki: todo.relatedWikiSlug == null
                ? null
                : () {
                    final repo = ref.read(
                      storageRepositoryProvider,
                    ) as RustBridgeRepository;
                    repo.getWikiPage(todo.relatedWikiSlug!).then((page) {
                      if (page == null || !mounted) return;
                      ref
                          .read(sidebarTabProvider.notifier)
                          .set(SidebarTab.wiki);
                      openWikiPageTab(ref, page);
                    });
                  },
            onOpenWorkItem: () async {
              // FFI 失败（打开工作项对应知识页失败等）不能变成 unhandled rejection：
              // 用 SnackBar 反馈，与 _add/_toggle 的错误路径保持一致。
              try {
                final repo =
                    ref.read(storageRepositoryProvider) as RustBridgeRepository;
                final page = await repo.openTodoWorkItem(todo.id);
                if (!mounted) return;
                ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
                openWikiPageTab(ref, page);
                ref.invalidate(todosProvider);
              } catch (e) {
                if (!mounted) return;
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text(e.toString()),
                    behavior: SnackBarBehavior.floating,
                  ),
                );
              }
            },
          ),
      ],
    );
  }
}
