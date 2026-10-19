part of '../todo_view.dart';

//! 待办编辑对话框 + 空态提示

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
  late bool _reminder;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _title = TextEditingController(text: widget.todo.title);
    _note = TextEditingController(text: widget.todo.note ?? '');
    _due = TextEditingController(text: widget.todo.dueAt ?? '');
    _priority = widget.todo.priority;
    _reminder = widget.todo.reminder;
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
      // 提醒开关独立于 dueAt：开/关单独调 setTodoReminder，
      // 校验（过去时间/缺 due_at 拒绝）由 Rust 侧诚实报错。
      if (_reminder != widget.todo.reminder) {
        await repo.setTodoReminder(widget.todo.id, _reminder);
      }
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
      title: Text(
        '编辑待办',
        style: TextStyle(color: AppTheme.textPrimary, fontSize: 16),
      ),
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
              decoration: _inputDecoration('截止日期（可选，YYYY-MM-DD，留空清除）'),
            ),
            const SizedBox(height: AppTheme.space2),
            // 定时提醒开关（评审：需要明确到期时刻）
            Row(
              children: [
                Icon(Icons.alarm, size: 16, color: AppTheme.accentPrimary),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '到期提醒（到时弹横幅/通知）',
                    style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                  ),
                ),
                Switch(
                  value: _reminder,
                  onChanged: (v) => setState(() => _reminder = v),
                ),
              ],
            ),
            const SizedBox(height: AppTheme.space3),
            Text(
              '优先级',
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
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
                      label: Text(
                        p.$2,
                        style: TextStyle(
                          fontSize: 12,
                          color: _priority == p.$1
                              ? Colors.black
                              : AppTheme.textSecondary,
                        ),
                      ),
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
          onPressed: _saving ? null : () => Navigator.of(context).pop(false),
          child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: AppTheme.accentPrimary,
          ),
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
