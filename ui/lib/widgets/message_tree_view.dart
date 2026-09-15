import 'package:flutter/material.dart';
import '../models/conversation.dart';

/// A tree node representing a message and its children
class MessageTreeNode {
  final Message message;
  final List<MessageTreeNode> children;
  int depth;

  MessageTreeNode({
    required this.message,
    List<MessageTreeNode>? children,
    this.depth = 0,
  }) : children = children ?? [];
}

/// Widget to display messages in a tree structure showing parent-child relationships
class MessageTreeView extends StatelessWidget {
  final List<Message> messages;
  final String? selectedMessageId;
  final Function(Message)? onMessageTap;
  final Function(Message)? onBranchTap;

  const MessageTreeView({
    super.key,
    required this.messages,
    this.selectedMessageId,
    this.onMessageTap,
    this.onBranchTap,
  });

  @override
  Widget build(BuildContext context) {
    final tree = _buildMessageTree(messages);

    return ListView.builder(
      padding: const EdgeInsets.all(16),
      itemCount: tree.length,
      itemBuilder: (context, index) {
        return _MessageTreeNodeWidget(
          node: tree[index],
          selectedMessageId: selectedMessageId,
          onMessageTap: onMessageTap,
          onBranchTap: onBranchTap,
        );
      },
    );
  }

  /// Build a tree structure from flat message list
  List<MessageTreeNode> _buildMessageTree(List<Message> messages) {
    final Map<String, MessageTreeNode> nodeMap = {};
    final List<MessageTreeNode> roots = [];

    // First pass: create all nodes
    for (final message in messages) {
      nodeMap[message.id] = MessageTreeNode(message: message);
    }

    // Second pass: link parents and children
    for (final message in messages) {
      final node = nodeMap[message.id]!;

      if (message.parentMessageId == null) {
        // Root message
        roots.add(node);
      } else {
        // Child message - add to parent's children
        final parent = nodeMap[message.parentMessageId];
        if (parent != null) {
          parent.children.add(node);
        } else {
          // Parent not found, treat as root
          roots.add(node);
        }
      }
    }

    // Calculate depths
    _calculateDepths(roots, 0);

    return roots;
  }

  void _calculateDepths(List<MessageTreeNode> nodes, int depth) {
    for (final node in nodes) {
      node.depth = depth;
      _calculateDepths(node.children, depth + 1);
    }
  }
}

class _MessageTreeNodeWidget extends StatelessWidget {
  final MessageTreeNode node;
  final String? selectedMessageId;
  final Function(Message)? onMessageTap;
  final Function(Message)? onBranchTap;

  const _MessageTreeNodeWidget({
    required this.node,
    this.selectedMessageId,
    this.onMessageTap,
    this.onBranchTap,
  });

  @override
  Widget build(BuildContext context) {
    final isSelected = node.message.id == selectedMessageId;
    final hasBranches = node.children.length > 1;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // The message itself
        Padding(
          padding: EdgeInsets.only(left: node.depth * 32.0),
          child: GestureDetector(
            onTap: () => onMessageTap?.call(node.message),
            child: Container(
              margin: const EdgeInsets.only(bottom: 8),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: isSelected
                    ? const Color(0xFF38BDF8).withValues(alpha: 0.2)
                    : const Color(0xFF0A0D12).withValues(alpha: 0.6),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: isSelected
                      ? const Color(0xFF38BDF8)
                      : const Color(0xFF38BDF8).withValues(alpha: 0.1),
                  width: isSelected ? 2 : 1,
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      // Role indicator
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 4,
                        ),
                        decoration: BoxDecoration(
                          color: node.message.isUser
                              ? const Color(0xFF38BDF8).withValues(alpha: 0.2)
                              : const Color(0xFF6EE7B7).withValues(alpha: 0.2),
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: Text(
                          node.message.isUser ? 'User' : 'Assistant',
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            color: node.message.isUser
                                ? const Color(0xFF38BDF8)
                                : const Color(0xFF6EE7B7),
                          ),
                        ),
                      ),
                      if (hasBranches) ...[
                        const SizedBox(width: 8),
                        // Branch indicator
                        GestureDetector(
                          onTap: () => onBranchTap?.call(node.message),
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 2,
                            ),
                            decoration: BoxDecoration(
                              color: const Color(0xFFE9A568).withValues(alpha: 0.2),
                              borderRadius: BorderRadius.circular(4),
                              border: Border.all(
                                color: const Color(0xFFE9A568).withValues(alpha: 0.3),
                              ),
                            ),
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                const Icon(
                                  Icons.call_split,
                                  size: 12,
                                  color: Color(0xFFE9A568),
                                ),
                                const SizedBox(width: 4),
                                Text(
                                  '${node.children.length} 个分支',
                                  style: const TextStyle(
                                    fontSize: 10,
                                    fontWeight: FontWeight.w500,
                                    color: Color(0xFFE9A568),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                      const Spacer(),
                      // Timestamp
                      Text(
                        _formatTime(node.message.createdAt),
                        style: TextStyle(
                          fontSize: 11,
                          color: const Color(0xFFADB5BD).withValues(alpha: 0.6),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  // Message content
                  Text(
                    node.message.content,
                    style: const TextStyle(
                      fontSize: 14,
                      color: Color(0xFFE9ECEF),
                      height: 1.5,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        // Child messages
        ...node.children.map((child) => _MessageTreeNodeWidget(
          node: child,
          selectedMessageId: selectedMessageId,
          onMessageTap: onMessageTap,
          onBranchTap: onBranchTap,
        )),
      ],
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);

    if (diff.inMinutes < 1) {
      return '刚刚';
    } else if (diff.inMinutes < 60) {
      return '${diff.inMinutes} 分钟前';
    } else if (diff.inHours < 24) {
      return '${diff.inHours} 小时前';
    } else if (diff.inDays < 7) {
      return '${diff.inDays} 天前';
    } else {
      return '${time.month}/${time.day}';
    }
  }
}
