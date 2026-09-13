import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';
import '../models/conversation.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';

class ConversationList extends ConsumerWidget {
  const ConversationList({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final conversationsAsync = ref.watch(conversationsProvider);
    final selectedId = ref.watch(selectedConversationIdProvider);

    return Container(
      width: 280,
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          right: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Column(
        children: [
          // Header
          _buildHeader(context, ref),

          // Conversation list
          Expanded(
            child: conversationsAsync.when(
              data: (conversations) {
                if (conversations.isEmpty) {
                  return _buildEmptyState();
                }

                // Auto-select first conversation if none selected
                if (selectedId == null && conversations.isNotEmpty) {
                  WidgetsBinding.instance.addPostFrameCallback((_) {
                    ref.read(selectedConversationIdProvider.notifier).state =
                        conversations.first.id;
                  });
                }

                return ListView.builder(
                  itemCount: conversations.length,
                  itemBuilder: (context, index) {
                    final conversation = conversations[index];
                    final isSelected = conversation.id == selectedId;
                    return _ConversationItem(
                      conversation: conversation,
                      isSelected: isSelected,
                      onTap: () {
                        ref.read(selectedConversationIdProvider.notifier).state =
                            conversation.id;
                      },
                    );
                  },
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
      ),
    );
  }

  Widget _buildHeader(BuildContext context, WidgetRef ref) {
    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
      decoration: BoxDecoration(
        border: Border(
          bottom: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          Text(
            'Elsewhen',
            style: Theme.of(context).textTheme.titleLarge?.copyWith(
              color: AppTheme.textPrimary,
              fontWeight: FontWeight.w600,
            ),
          ),
          const Spacer(),
          IconButton(
            icon: const Icon(Icons.add_circle_outline),
            color: AppTheme.accentPrimary,
            onPressed: () async {
              final repo = ref.read(conversationRepositoryProvider);
              final newConv = await repo.createConversation();
              ref.invalidate(conversationsProvider);
              ref.read(selectedConversationIdProvider.notifier).state = newConv.id;
            },
            tooltip: '新建对话',
          ),
        ],
      ),
    );
  }

  Widget _buildEmptyState() {
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
            '还没有对话',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 14,
            ),
          ),
        ],
      ),
    );
  }
}

class _ConversationItem extends StatelessWidget {
  final Conversation conversation;
  final bool isSelected;
  final VoidCallback onTap;

  const _ConversationItem({
    required this.conversation,
    required this.isSelected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Material(
      color: isSelected ? AppTheme.surface2 : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.all(AppTheme.space3),
          decoration: BoxDecoration(
            border: Border(
              left: BorderSide(
                color: isSelected ? AppTheme.accentPrimary : Colors.transparent,
                width: 3,
              ),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Title and timestamp
              Row(
                children: [
                  Expanded(
                    child: Text(
                      conversation.displayTitle,
                      style: TextStyle(
                        color: AppTheme.textPrimary,
                        fontSize: 14,
                        fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: AppTheme.space2),
                  Text(
                    _formatTime(conversation.updatedAt),
                    style: TextStyle(
                      color: AppTheme.textTertiary,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),

              // Preview and count
              if (conversation.lastMessagePreview != null) ...[
                const SizedBox(height: AppTheme.space1),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        conversation.lastMessagePreview!,
                        style: TextStyle(
                          color: AppTheme.textSecondary,
                          fontSize: 12,
                        ),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ],
                ),
              ],

              const SizedBox(height: AppTheme.space1),
              Text(
                '${conversation.messageCount} 条消息',
                style: TextStyle(
                  color: AppTheme.textTertiary,
                  fontSize: 11,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);

    if (diff.inMinutes < 60) {
      return '${diff.inMinutes}分钟前';
    } else if (diff.inHours < 24) {
      return '${diff.inHours}小时前';
    } else if (diff.inDays < 7) {
      return '${diff.inDays}天前';
    } else {
      return DateFormat('MM-dd').format(time);
    }
  }
}
