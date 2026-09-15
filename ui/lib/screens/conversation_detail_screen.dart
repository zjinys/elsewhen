import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/conversation.dart';
import '../bridge/rust_bridge_repository.dart';
import '../widgets/custom_title_bar.dart';
import '../widgets/message_tree_view.dart';

class ConversationDetailScreen extends ConsumerStatefulWidget {
  final String conversationId;
  final String? conversationTitle;

  const ConversationDetailScreen({
    super.key,
    required this.conversationId,
    this.conversationTitle,
  });

  @override
  ConsumerState<ConversationDetailScreen> createState() =>
      _ConversationDetailScreenState();
}

class _ConversationDetailScreenState
    extends ConsumerState<ConversationDetailScreen> {
  String? _selectedMessageId;
  List<Message> _childMessages = [];
  bool _showingBranches = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          CustomTitleBar(
            title: widget.conversationTitle ?? '对话详情',
            actions: [
              if (_showingBranches)
                IconButton(
                  icon: const Icon(Icons.arrow_back, size: 20),
                  onPressed: () {
                    setState(() {
                      _showingBranches = false;
                      _childMessages = [];
                    });
                  },
                  tooltip: '返回主线',
                ),
              IconButton(
                icon: const Icon(Icons.history, size: 20),
                onPressed: () => _showMessageChain(),
                tooltip: '查看消息链',
              ),
            ],
          ),
          Expanded(
            child: Container(
              decoration: const BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [
                    Color(0xFF05070C),
                    Color(0xFF0F131C),
                    Color(0xFF161D2B),
                  ],
                ),
              ),
              child: _showingBranches
                  ? _buildBranchView()
                  : _buildMainView(),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildMainView() {
    return FutureBuilder<List<Message>>(
      future: _loadMessages(),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.waiting) {
          return const Center(
            child: CircularProgressIndicator(
              color: Color(0xFF38BDF8),
            ),
          );
        }

        if (snapshot.hasError) {
          return Center(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  Icons.error_outline,
                  size: 48,
                  color: Colors.red.withValues(alpha: 0.7),
                ),
                const SizedBox(height: 16),
                Text(
                  '加载失败: ${snapshot.error}',
                  style: const TextStyle(color: Color(0xFFE9ECEF)),
                ),
              ],
            ),
          );
        }

        final messages = snapshot.data ?? [];

        if (messages.isEmpty) {
          return const Center(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  Icons.chat_bubble_outline,
                  size: 64,
                  color: Color(0xFF6C7A89),
                ),
                SizedBox(height: 16),
                Text(
                  '暂无消息',
                  style: TextStyle(
                    color: Color(0xFFADB5BD),
                    fontSize: 16,
                  ),
                ),
              ],
            ),
          );
        }

        return MessageTreeView(
          messages: messages,
          selectedMessageId: _selectedMessageId,
          onMessageTap: (message) {
            setState(() {
              _selectedMessageId = message.id;
            });
          },
          onBranchTap: (message) => _showBranches(message),
        );
      },
    );
  }

  Widget _buildBranchView() {
    return Column(
      children: [
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: const Color(0xFF0A0D12).withValues(alpha: 0.6),
            border: Border(
              bottom: BorderSide(
                color: const Color(0xFF38BDF8).withValues(alpha: 0.1),
              ),
            ),
          ),
          child: Row(
            children: [
              const Icon(
                Icons.call_split,
                color: Color(0xFFE9A568),
                size: 20,
              ),
              const SizedBox(width: 8),
              Text(
                '分支对话 (${_childMessages.length} 条)',
                style: const TextStyle(
                  color: Color(0xFFE9ECEF),
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: MessageTreeView(
            messages: _childMessages,
            selectedMessageId: _selectedMessageId,
            onMessageTap: (message) {
              setState(() {
                _selectedMessageId = message.id;
              });
            },
          ),
        ),
      ],
    );
  }

  Future<List<Message>> _loadMessages() async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    return await repo.listMessages(widget.conversationId);
  }

  Future<void> _showBranches(Message parentMessage) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final children = await repo.getChildMessages(parentMessage.id);

    setState(() {
      _childMessages = children;
      _showingBranches = true;
    });
  }

  Future<void> _showMessageChain() async {
    if (_selectedMessageId == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: const Text('请先选择一条消息'),
          backgroundColor: const Color(0xFFE9A568),
          behavior: SnackBarBehavior.floating,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(999),
          ),
        ),
      );
      return;
    }

    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final chain = await repo.getMessageChain(_selectedMessageId!);

    if (!mounted) return;

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xFF0F131C),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(16),
        ),
        title: const Row(
          children: [
            Icon(Icons.timeline, color: Color(0xFF38BDF8)),
            SizedBox(width: 8),
            Text(
              '消息追溯链',
              style: TextStyle(color: Color(0xFFE9ECEF)),
            ),
          ],
        ),
        content: SizedBox(
          width: 600,
          child: ListView.builder(
            shrinkWrap: true,
            itemCount: chain.length,
            itemBuilder: (context, index) {
              final message = chain[index];
              final isLast = index == chain.length - 1;

              return Column(
                children: [
                  Container(
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: message.id == _selectedMessageId
                          ? const Color(0xFF38BDF8).withValues(alpha: 0.2)
                          : const Color(0xFF0A0D12).withValues(alpha: 0.6),
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(
                        color: message.id == _selectedMessageId
                            ? const Color(0xFF38BDF8)
                            : const Color(0xFF38BDF8).withValues(alpha: 0.1),
                      ),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 6,
                                vertical: 2,
                              ),
                              decoration: BoxDecoration(
                                color: message.isUser
                                    ? const Color(0xFF38BDF8)
                                        .withValues(alpha: 0.2)
                                    : const Color(0xFF6EE7B7)
                                        .withValues(alpha: 0.2),
                                borderRadius: BorderRadius.circular(4),
                              ),
                              child: Text(
                                message.isUser ? 'User' : 'Assistant',
                                style: TextStyle(
                                  fontSize: 10,
                                  fontWeight: FontWeight.w600,
                                  color: message.isUser
                                      ? const Color(0xFF38BDF8)
                                      : const Color(0xFF6EE7B7),
                                ),
                              ),
                            ),
                            const Spacer(),
                            Text(
                              '#${index + 1}',
                              style: TextStyle(
                                fontSize: 11,
                                color: const Color(0xFFADB5BD)
                                    .withValues(alpha: 0.6),
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 8),
                        Text(
                          message.content,
                          style: const TextStyle(
                            fontSize: 13,
                            color: Color(0xFFE9ECEF),
                          ),
                          maxLines: 3,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    ),
                  ),
                  if (!isLast)
                    const Padding(
                      padding: EdgeInsets.symmetric(vertical: 8),
                      child: Icon(
                        Icons.arrow_downward,
                        size: 16,
                        color: Color(0xFF38BDF8),
                      ),
                    ),
                ],
              );
            },
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('关闭'),
          ),
        ],
      ),
    );
  }
}
