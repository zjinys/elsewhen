import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

class EventInput extends StatefulWidget {
  final Function(String) onSubmit;

  const EventInput({
    super.key,
    required this.onSubmit,
  });

  @override
  State<EventInput> createState() => _EventInputState();
}

class _EventInputState extends State<EventInput> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  bool _isSubmitting = false;

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_isSubmitting || _controller.text.trim().isEmpty) return;

    setState(() => _isSubmitting = true);

    try {
      await widget.onSubmit(_controller.text);
      _controller.clear();
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
        _focusNode.requestFocus();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Expanded(
          child: Container(
            constraints: const BoxConstraints(
              minHeight: 48,
              maxHeight: 120,
            ),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
            ),
            child: TextField(
              controller: _controller,
              focusNode: _focusNode,
              maxLines: null,
              textInputAction: TextInputAction.send,
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 15,
                height: 1.6,
              ),
              decoration: InputDecoration(
                hintText: '记录此刻发生的事情…（可用 @人名 标注人物、#事情 标注项目）',
                hintStyle: TextStyle(
                  color: AppTheme.textTertiary,
                  fontSize: 15,
                ),
                border: InputBorder.none,
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: AppTheme.space4,
                  vertical: AppTheme.space3,
                ),
              ),
              onSubmitted: (_) => _submit(),
            ),
          ),
        ),
        IconButton(
          tooltip: '对话格式',
          onPressed: () => _showConversationGuide(context),
          icon: Icon(Icons.help_outline, size: 19, color: AppTheme.textTertiary),
        ),
        const SizedBox(width: AppTheme.space3),
        SizedBox(
          height: 48,
          child: ElevatedButton(
            onPressed: _isSubmitting ? null : _submit,
            style: ElevatedButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
              foregroundColor: AppTheme.surface0,
              elevation: 0,
              padding: const EdgeInsets.symmetric(
                horizontal: AppTheme.space6,
              ),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
            ),
            child: _isSubmitting
                ? SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      valueColor: AlwaysStoppedAnimation<Color>(
                        AppTheme.surface0,
                      ),
                    ),
                  )
                : Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.send,
                        size: 18,
                      ),
                      const SizedBox(width: AppTheme.space2),
                      const Text(
                        '记录',
                        style: TextStyle(
                          fontWeight: FontWeight.w600,
                          fontSize: 15,
                        ),
                      ),
                    ],
                  ),
          ),
        ),
      ],
    );
  }

  Future<void> _showConversationGuide(BuildContext context) => showDialog<void>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: const Text('怎么记录'),
      content: const SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Text(
            '@人名：明确标注人物\n'
            '#事情：明确标注项目、事项或主题\n\n'
            '例如：@张伟 正在负责 #付款流程\n\n'
            '记一下：明确保存一条经历或进展\n'
            '保存到知识库：把结论沉淀下来\n'
            '帮我建待办：创建后续行动\n\n'
            '人物关系、知识页和待办会先给你看草稿。回复“好”才保存；回复“不要”或“取消”则放弃。',
          ),
        ),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(dialogContext), child: const Text('知道了')),
      ],
    ),
  );
}
