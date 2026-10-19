part of '../wiki_ai_chat_panel.dart';

//! build 与消息渲染
extension _PanelRender on _WikiAiChatPanelState {
  Widget _buildMessage(Message message, {bool canGenerate = false}) {
    final isProject =
        ref.watch(wikiPageProvider(widget.slug)).value?.kind == 'project';
    final bubble = WikiChatBubble(
      message: ContentChatMessage(
        role: message.role.name,
        content: message.content,
      ),
      onExpand: message.role == MessageRole.assistant
          ? () => _showExpandedReply(message.content)
          : null,
    );
    if (canGenerate) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          bubble,
          TextButton(
            onPressed: () => ref
                .read(chatRunControllerProvider(message.conversationId))
                .start(message.id),
            child: const Text('生成回复'),
          ),
        ],
      );
    }
    if (message.role != MessageRole.assistant ||
        message.content.trim().isEmpty) {
      return bubble;
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        bubble,
        Padding(
          padding: const EdgeInsets.only(left: 40, bottom: AppTheme.space2),
          child: Wrap(
            spacing: 4,
            children: [
              if (message.content.contains('[['))
                KnowledgeCitationsButton(messageId: message.id),
              if (kSuggestionFeedbackEnabled)
                SuggestionFeedbackControl(
                  messageId: message.id,
                  content: message.content,
                ),
              TextButton.icon(
                onPressed: (_busy || _saving)
                    ? null
                    : () => _saveReply(message),
                icon: Icon(
                  isProject
                      ? Icons.find_replace_outlined
                      : Icons.bookmark_add_outlined,
                  size: 14,
                ),
                label: Text(isProject ? '替换正文' : '保存为产出'),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void _showExpandedReply(String content) {
    showDialog<void>(
      context: context,
      builder: (context) => Dialog(
        child: ConstrainedBox(
          constraints: BoxConstraints(
            maxWidth: 900,
            maxHeight: MediaQuery.sizeOf(context).height * .88,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 12, 12, 8),
                child: Row(
                  children: [
                    const Expanded(child: Text('AI 回复')),
                    IconButton(
                      tooltip: '关闭',
                      onPressed: () => Navigator.pop(context),
                      icon: const Icon(Icons.close),
                    ),
                  ],
                ),
              ),
              const Divider(height: 1),
              Flexible(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(24),
                  child: SelectionArea(child: MarkdownView(markdown: content)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _returnToMainConversation() async {
    final main = await ref.read(mainConversationProvider.future);
    if (!mounted) return;
    ref.read(selectedConversationIdProvider.notifier).set(main.id);
    ref.read(sidebarTabProvider.notifier).set(SidebarTab.conversation);
  }
}
