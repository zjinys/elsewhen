import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/conversation.dart';
import '../bridge/rust_bridge_repository.dart';
import '../widgets/custom_title_bar.dart';
import 'settings_screen.dart';
import 'conversation_detail_screen.dart';

/// Timeline view with tag-based organization
class ConversationTimelineScreen extends ConsumerStatefulWidget {
  const ConversationTimelineScreen({super.key});

  @override
  ConsumerState<ConversationTimelineScreen> createState() => _ConversationTimelineScreenState();
}

class _ConversationTimelineScreenState extends ConsumerState<ConversationTimelineScreen> {
  String? _selectedTag;
  final Set<String> _availableTags = {};

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          CustomTitleBar(
            title: '对话时间线',
            actions: [
              if (_availableTags.isNotEmpty)
                PopupMenuButton<String?>(
                  icon: const Icon(Icons.filter_list, size: 20),
                  onSelected: (tag) {
                    setState(() {
                      _selectedTag = tag;
                    });
                  },
                  itemBuilder: (context) => [
                    const PopupMenuItem(
                      value: null,
                      child: Text('全部对话'),
                    ),
                    const PopupMenuDivider(),
                    ..._availableTags.map((tag) => PopupMenuItem(
                      value: tag,
                      child: Row(
                        children: [
                          _TagChip(tag: tag, small: true),
                          const SizedBox(width: 8),
                          Text(tag),
                        ],
                      ),
                    )),
                  ],
                ),
              IconButton(
                icon: const Icon(Icons.settings_outlined, size: 20),
                onPressed: () {
                  Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (context) => const SettingsScreen(),
                    ),
                  );
                },
                tooltip: '设置',
              ),
            ],
          ),
          Expanded(
            child: FutureBuilder<List<Conversation>>(
              future: _loadConversations(),
              builder: (context, snapshot) {
                if (snapshot.connectionState == ConnectionState.waiting) {
                  return const Center(child: CircularProgressIndicator());
                }

                if (snapshot.hasError) {
                  return Center(child: Text('错误: ${snapshot.error}'));
                }

                final conversations = snapshot.data ?? [];
                final filtered = _selectedTag == null
                    ? conversations
                    : conversations.where((c) => c.tag == _selectedTag).toList();

                if (filtered.isEmpty) {
                  return Center(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(Icons.chat_bubble_outline, size: 64, color: Colors.grey[400]),
                        const SizedBox(height: 16),
                        Text(
                          _selectedTag == null ? '暂无对话' : '此标签下暂无对话',
                          style: TextStyle(color: Colors.grey[600]),
                        ),
                      ],
                    ),
                  );
                }

                // Group by date
                final grouped = _groupByDate(filtered);

                return ListView.builder(
                  padding: const EdgeInsets.all(16),
                  itemCount: grouped.length,
                  itemBuilder: (context, index) {
                    final entry = grouped.entries.elementAt(index);
                    return _TimelineSection(
                      date: entry.key,
                      conversations: entry.value,
                    );
                  },
                );
              },
            ),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => _showCreateDialog(),
        icon: const Icon(Icons.add),
        label: const Text('新建对话'),
      ),
    );
  }

  Future<List<Conversation>> _loadConversations() async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final conversations = await repo.listConversations();

    // Extract unique tags
    setState(() {
      _availableTags.clear();
      for (final conv in conversations) {
        if (conv.tag != null && conv.tag!.isNotEmpty) {
          _availableTags.add(conv.tag!);
        }
      }
    });

    return conversations;
  }

  Map<String, List<Conversation>> _groupByDate(List<Conversation> conversations) {
    final grouped = <String, List<Conversation>>{};
    final now = DateTime.now();

    for (final conv in conversations) {
      String key;
      final diff = now.difference(conv.updatedAt).inDays;

      if (diff == 0) {
        key = '今天';
      } else if (diff == 1) {
        key = '昨天';
      } else if (diff < 7) {
        key = '本周';
      } else if (diff < 30) {
        key = '本月';
      } else {
        key = '${conv.updatedAt.year}年${conv.updatedAt.month}月';
      }

      grouped.putIfAbsent(key, () => []).add(conv);
    }

    return grouped;
  }

  void _showCreateDialog() {
    final titleController = TextEditingController();
    String? selectedTag;

    showDialog(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('新建对话'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                controller: titleController,
                decoration: const InputDecoration(
                  labelText: '标题 (可选)',
                  hintText: '输入对话标题',
                ),
              ),
              const SizedBox(height: 16),
              DropdownButtonFormField<String?>(
                value: selectedTag,
                decoration: const InputDecoration(
                  labelText: '标签 (可选)',
                ),
                items: [
                  const DropdownMenuItem(value: null, child: Text('无标签')),
                  ..._predefinedTags.map((tag) => DropdownMenuItem(
                    value: tag,
                    child: Row(
                      children: [
                        _TagChip(tag: tag, small: true),
                        const SizedBox(width: 8),
                        Text(tag),
                      ],
                    ),
                  )),
                ],
                onChanged: (value) {
                  setDialogState(() {
                    selectedTag = value;
                  });
                },
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () async {
                final title = titleController.text.trim().isEmpty
                    ? null
                    : titleController.text.trim();

                final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
                await repo.createConversation(title: title, tag: selectedTag);

                if (context.mounted) {
                  Navigator.pop(context);
                  setState(() {}); // Refresh list
                }
              },
              child: const Text('创建'),
            ),
          ],
        ),
      ),
    );
  }
}

class _TimelineSection extends StatelessWidget {
  final String date;
  final List<Conversation> conversations;

  const _TimelineSection({
    required this.date,
    required this.conversations,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Text(
            date,
            style: Theme.of(context).textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.bold,
            ),
          ),
        ),
        ...conversations.map((conv) => _ConversationCard(conversation: conv)),
        const SizedBox(height: 16),
      ],
    );
  }
}

class _ConversationCard extends StatelessWidget {
  final Conversation conversation;

  const _ConversationCard({required this.conversation});

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      child: InkWell(
        onTap: () {
          Navigator.of(context).push(
            MaterialPageRoute(
              builder: (context) => ConversationDetailScreen(
                conversationId: conversation.id,
                conversationTitle: conversation.displayTitle,
              ),
            ),
          );
        },
        borderRadius: BorderRadius.circular(12),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      conversation.displayTitle,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                  ),
                  if (conversation.tag != null)
                    _TagChip(tag: conversation.tag!),
                ],
              ),
              if (conversation.lastMessagePreview != null) ...[
                const SizedBox(height: 8),
                Text(
                  conversation.lastMessagePreview!,
                  style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: Colors.grey[600],
                  ),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
              const SizedBox(height: 8),
              Row(
                children: [
                  Icon(Icons.message, size: 14, color: Colors.grey[600]),
                  const SizedBox(width: 4),
                  Text(
                    '${conversation.messageCount} 条消息',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Colors.grey[600],
                    ),
                  ),
                  const Spacer(),
                  Text(
                    _formatTime(conversation.updatedAt),
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Colors.grey[600],
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

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);

    if (diff.inMinutes < 60) {
      return '${diff.inMinutes} 分钟前';
    } else if (diff.inHours < 24) {
      return '${diff.inHours} 小时前';
    } else {
      return '${time.month}/${time.day}';
    }
  }
}

class _TagChip extends StatelessWidget {
  final String tag;
  final bool small;

  const _TagChip({required this.tag, this.small = false});

  @override
  Widget build(BuildContext context) {
    final color = _getTagColor(tag);

    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: small ? 6 : 8,
        vertical: small ? 2 : 4,
      ),
      decoration: BoxDecoration(
        color: color.withOpacity(0.1),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: color.withOpacity(0.3)),
      ),
      child: Text(
        tag,
        style: TextStyle(
          color: color,
          fontSize: small ? 10 : 12,
          fontWeight: FontWeight.w500,
        ),
      ),
    );
  }

  Color _getTagColor(String tag) {
    switch (tag) {
      case '工作':
        return Colors.blue;
      case '学习':
        return Colors.green;
      case '生活':
        return Colors.orange;
      case '创意':
        return Colors.purple;
      case '其他':
        return Colors.grey;
      default:
        return Colors.teal;
    }
  }
}

// Predefined tags
const _predefinedTags = ['工作', '学习', '生活', '创意', '其他'];
