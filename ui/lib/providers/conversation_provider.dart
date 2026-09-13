import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/conversation.dart';
import '../bridge/rust_bridge_repository.dart';

/// Mock conversation repository (will be replaced with Rust bridge)
class ConversationRepository {
  // ignore: unused_field
  final RustBridgeRepository _bridge;

  ConversationRepository(this._bridge);

  /// Get all conversations
  Future<List<Conversation>> getConversations() async {
    // TODO: Replace with bridge call when implemented
    // return await _bridge.listConversations();

    // Mock data for now
    await Future.delayed(const Duration(milliseconds: 300));
    final now = DateTime.now();
    return [
      Conversation(
        id: '1',
        title: 'Flutter 开发讨论',
        createdAt: now.subtract(const Duration(days: 2)),
        updatedAt: now.subtract(const Duration(hours: 1)),
        messageCount: 15,
        lastMessagePreview: '好的，我会实现左右布局...',
      ),
      Conversation(
        id: '2',
        title: null, // Untitled
        createdAt: now.subtract(const Duration(days: 1)),
        updatedAt: now.subtract(const Duration(hours: 3)),
        messageCount: 7,
        lastMessagePreview: '这个功能很有意思',
      ),
      Conversation(
        id: '3',
        title: 'Rust 后端优化',
        createdAt: now.subtract(const Duration(days: 5)),
        updatedAt: now.subtract(const Duration(days: 1)),
        messageCount: 23,
        lastMessagePreview: '性能测试结果显示...',
      ),
    ];
  }

  /// Get messages for a conversation
  Future<List<Message>> getMessages(String conversationId) async {
    // TODO: Replace with bridge call when implemented
    // return await _bridge.listMessages(conversationId);

    // Mock data for now
    await Future.delayed(const Duration(milliseconds: 200));
    final now = DateTime.now();
    return [
      Message(
        id: 'm1',
        conversationId: conversationId,
        role: MessageRole.user,
        content: '你好，我想了解一下这个功能的实现方式',
        createdAt: now.subtract(const Duration(minutes: 10)),
      ),
      Message(
        id: 'm2',
        conversationId: conversationId,
        role: MessageRole.assistant,
        content: '好的，我来详细解释一下。这个功能主要分为三个部分：\n\n1. 数据模型层\n2. 状态管理层\n3. UI 展示层\n\n每一层都有明确的职责...',
        createdAt: now.subtract(const Duration(minutes: 9)),
      ),
      Message(
        id: 'm3',
        conversationId: conversationId,
        role: MessageRole.user,
        content: '明白了，那具体的代码实现呢？',
        createdAt: now.subtract(const Duration(minutes: 5)),
      ),
      Message(
        id: 'm4',
        conversationId: conversationId,
        role: MessageRole.assistant,
        content: '代码实现如下：\n\n```dart\nclass Example {\n  final String id;\n  // ...\n}\n```\n\n这样可以确保类型安全和可维护性。',
        createdAt: now.subtract(const Duration(minutes: 4)),
      ),
    ];
  }

  /// Create a new conversation
  Future<Conversation> createConversation() async {
    // TODO: Replace with bridge call when implemented
    // return await _bridge.createConversation();

    // Mock data for now
    await Future.delayed(const Duration(milliseconds: 100));
    final now = DateTime.now();
    return Conversation(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      title: null,
      createdAt: now,
      updatedAt: now,
      messageCount: 0,
      lastMessagePreview: null,
    );
  }

  /// Send a message in a conversation
  Future<Message> sendMessage(String conversationId, String content) async {
    // TODO: Replace with bridge call when implemented
    // return await _bridge.sendMessage(conversationId, content);

    // Mock data for now
    await Future.delayed(const Duration(milliseconds: 500));
    return Message(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      conversationId: conversationId,
      role: MessageRole.user,
      content: content,
      createdAt: DateTime.now(),
    );
  }
}

// Providers

/// Conversation repository provider
final conversationRepositoryProvider = Provider<ConversationRepository>((ref) {
  final bridge = ref.read(rustBridgeRepositoryProvider);
  return ConversationRepository(bridge);
});

/// Conversations list provider
final conversationsProvider = FutureProvider<List<Conversation>>((ref) async {
  final repo = ref.read(conversationRepositoryProvider);
  return await repo.getConversations();
});

/// Selected conversation ID provider
final selectedConversationIdProvider = StateProvider<String?>((ref) => null);

/// Messages for selected conversation provider
final messagesProvider = FutureProvider<List<Message>>((ref) async {
  final conversationId = ref.watch(selectedConversationIdProvider);
  if (conversationId == null) return [];

  final repo = ref.read(conversationRepositoryProvider);
  return await repo.getMessages(conversationId);
});

/// Message input provider
final messageInputProvider = StateProvider<String>((ref) => '');
