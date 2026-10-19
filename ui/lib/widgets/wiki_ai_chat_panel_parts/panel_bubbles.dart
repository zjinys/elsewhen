part of '../wiki_ai_chat_panel.dart';

//! 对话气泡组件（公共 WikiChatBubble/AiMessageBody）
/// 聊天气泡：页面 AI 对话与导入预览共用
class WikiChatBubble extends StatelessWidget {
  final ContentChatMessage message;
  final bool pending;
  final VoidCallback? onExpand;

  const WikiChatBubble({super.key, required this.message, this.onExpand})
    : pending = false;

  const WikiChatBubble.pending({super.key})
    : message = const ContentChatMessage(role: 'assistant', content: ''),
      pending = true,
      onExpand = null;

  @override
  Widget build(BuildContext context) {
    final isUser = message.role == 'user';
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space3),
      child: Row(
        mainAxisAlignment: isUser
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(),
            const SizedBox(width: 8),
          ] else
            const SizedBox(width: 48),
          Flexible(
            child: GestureDetector(
              onDoubleTap: onExpand,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: isUser
                      ? AppTheme.accentPrimary.withValues(alpha: 0.15)
                      : AppTheme.surface2,
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                ),
                child: pending
                    ? Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          SizedBox(
                            width: 12,
                            height: 12,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                          SizedBox(width: 8),
                          Text(
                            '思考中…',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                        ],
                      )
                    : isUser
                    ? ContentFontScope(
                        child: SelectableText(
                          message.content,
                          style: TextStyle(
                            fontSize: 13,
                            height: 1.55,
                            color: AppTheme.textPrimary,
                          ),
                        ),
                      )
                    : _AiMessageBody(content: message.content),
              ),
            ),
          ),
          if (isUser) const SizedBox(width: 8),
        ],
      ),
    );
  }

  Widget _buildAvatar() {
    return Container(
      width: 26,
      height: 26,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: 0.18),
        shape: BoxShape.circle,
      ),
      child: Text(
        'EW',
        style: TextStyle(
          fontSize: 10,
          fontWeight: FontWeight.w700,
          color: AppTheme.accentPrimary,
        ),
      ),
    );
  }
}

/// AI 消息正文：Markdown 渲染 + 右上角复制按钮。
/// 复制整条原始 markdown（与主对话「复制这条消息」一致），点击后给轻量反馈。
class _AiMessageBody extends StatelessWidget {
  final String content;

  const _AiMessageBody({required this.content});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.end,
      mainAxisSize: MainAxisSize.min,
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: MarkdownView(
            markdown: content,
            baseStyle: TextStyle(
              fontSize: 13,
              height: 1.55,
              color: AppTheme.textPrimary,
            ),
          ),
        ),
        // 复制入口放正文右下角，轻量不占行高。
        IconButton(
          tooltip: '复制这条消息',
          icon: Icon(
            Icons.copy_rounded,
            size: 13,
            color: AppTheme.textTertiary,
          ),
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          constraints: const BoxConstraints(minWidth: 22, minHeight: 18),
          onPressed: () async {
            await Clipboard.setData(ClipboardData(text: content));
            if (!context.mounted) return;
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: const Text('已复制消息'),
                duration: const Duration(seconds: 1),
                behavior: SnackBarBehavior.floating,
                shape: const RoundedRectangleBorder(
                  borderRadius: BorderRadius.all(Radius.circular(999)),
                ),
              ),
            );
          },
        ),
      ],
    );
  }
}
