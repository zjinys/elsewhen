import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';
import '../models/conversation.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';

class MessageArea extends ConsumerWidget {
  const MessageArea({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final messagesAsync = ref.watch(messagesProvider);
    final selectedId = ref.watch(selectedConversationIdProvider);

    if (selectedId == null) {
      return _buildEmptyState();
    }

    return Column(
      children: [
        // Messages list
        Expanded(
          child: messagesAsync.when(
            data: (messages) {
              if (messages.isEmpty) {
                return _buildNoMessages();
              }

              return ListView.builder(
                padding: const EdgeInsets.all(AppTheme.space4),
                itemCount: messages.length,
                itemBuilder: (context, index) {
                  final message = messages[index];
                  return _MessageBubble(message: message);
                },
              );
            },
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (error, stack) => Center(
              child: Text(
                '加载消息失败',
                style: TextStyle(color: AppTheme.textSecondary),
              ),
            ),
          ),
        ),

        // Input area
        _MessageInput(),
      ],
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.forum_outlined,
            size: 64,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: AppTheme.space4),
          Text(
            '选择一个对话开始聊天',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 16,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildNoMessages() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.chat_bubble_outline,
            size: 48,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: AppTheme.space3),
          Text(
            '还没有消息',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 14,
            ),
          ),
          const SizedBox(height: AppTheme.space2),
          Text(
            '在下方输入框开始对话',
            style: TextStyle(
              color: AppTheme.textTertiary,
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }
}

class _MessageBubble extends StatelessWidget {
  final Message message;

  const _MessageBubble({required this.message});

  @override
  Widget build(BuildContext context) {
    final isUser = message.isUser;

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisAlignment: isUser ? MainAxisAlignment.end : MainAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(isUser: false),
            const SizedBox(width: AppTheme.space3),
          ],

          Flexible(
            child: Column(
              crossAxisAlignment: isUser ? CrossAxisAlignment.end : CrossAxisAlignment.start,
              children: [
                // Role and time
                Padding(
                  padding: const EdgeInsets.only(
                    left: AppTheme.space2,
                    right: AppTheme.space2,
                    bottom: AppTheme.space1,
                  ),
                  child: Text(
                    '${isUser ? "我" : "AI"} • ${_formatTime(message.createdAt)}',
                    style: TextStyle(
                      color: AppTheme.textTertiary,
                      fontSize: 11,
                    ),
                  ),
                ),

                // Message bubble
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space3,
                    vertical: AppTheme.space3,
                  ),
                  decoration: BoxDecoration(
                    color: isUser ? AppTheme.accentPrimary.withValues(alpha: 0.15) : AppTheme.surface2,
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    border: Border.all(
                      color: isUser
                          ? AppTheme.accentPrimary.withValues(alpha: 0.3)
                          : AppTheme.surface3,
                      width: 1,
                    ),
                  ),
                  child: Text(
                    message.content,
                    style: TextStyle(
                      color: AppTheme.textPrimary,
                      fontSize: 14,
                      height: 1.5,
                    ),
                  ),
                ),
              ],
            ),
          ),

          if (isUser) ...[
            const SizedBox(width: AppTheme.space3),
            _buildAvatar(isUser: true),
          ],
        ],
      ),
    );
  }

  Widget _buildAvatar({required bool isUser}) {
    return Container(
      width: 32,
      height: 32,
      decoration: BoxDecoration(
        color: isUser ? AppTheme.accentPrimary.withValues(alpha: 0.2) : AppTheme.surface3,
        shape: BoxShape.circle,
      ),
      child: Icon(
        isUser ? Icons.person : Icons.smart_toy,
        size: 18,
        color: isUser ? AppTheme.accentPrimary : AppTheme.textSecondary,
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final messageDate = DateTime(time.year, time.month, time.day);

    if (messageDate == today) {
      return DateFormat('HH:mm').format(time);
    } else {
      return DateFormat('MM-dd HH:mm').format(time);
    }
  }
}

class _MessageInput extends ConsumerStatefulWidget {
  @override
  ConsumerState<_MessageInput> createState() => _MessageInputState();
}

class _MessageInputState extends ConsumerState<_MessageInput> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  bool _isSubmitting = false;

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _handleSubmit() async {
    final text = _controller.text.trim();
    if (text.isEmpty || _isSubmitting) return;

    final conversationId = ref.read(selectedConversationIdProvider);
    if (conversationId == null) return;

    setState(() => _isSubmitting = true);

    try {
      final repo = ref.read(conversationRepositoryProvider);
      await repo.sendMessage(conversationId, text);

      _controller.clear();
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);

      _focusNode.requestFocus();
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          top: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: TextField(
              controller: _controller,
              focusNode: _focusNode,
              maxLines: 4,
              minLines: 1,
              enabled: !_isSubmitting,
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 14,
              ),
              decoration: InputDecoration(
                hintText: '输入消息...',
                hintStyle: TextStyle(color: AppTheme.textTertiary),
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
                  borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
                ),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: AppTheme.space3,
                  vertical: AppTheme.space3,
                ),
              ),
              onSubmitted: (_) => _handleSubmit(),
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          IconButton(
            onPressed: _isSubmitting ? null : _handleSubmit,
            icon: _isSubmitting
                ? const SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.send),
            color: AppTheme.accentPrimary,
            iconSize: 24,
            tooltip: '发送',
          ),
        ],
      ),
    );
  }
}
