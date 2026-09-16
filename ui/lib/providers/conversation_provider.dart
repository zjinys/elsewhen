import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/conversation.dart';
import '../bridge/rust_bridge_repository.dart';

/// Mock conversation repository (will be replaced with Rust bridge)
class ConversationRepository {
  final RustBridgeRepository _bridge;

  ConversationRepository(this._bridge);

  /// Get all conversations
  Future<List<Conversation>> getConversations() async {
    return await _bridge.listConversations();
  }

  /// Get archived conversations
  Future<List<Conversation>> getArchivedConversations() async {
    return await _bridge.listArchivedConversations();
  }

  /// Get messages for a conversation
  Future<List<Message>> getMessages(String conversationId) async {
    return await _bridge.listMessages(conversationId);
  }

  /// Create a new conversation
  Future<Conversation> createConversation() async {
    return await _bridge.createConversation();
  }

  /// Rename a conversation
  Future<void> renameConversation(String conversationId, String title) async {
    return await _bridge.renameConversation(conversationId, title);
  }

  /// Archive or unarchive a conversation
  Future<void> setArchived(String conversationId, bool archived) async {
    return await _bridge.setConversationArchived(conversationId, archived);
  }

  /// Send a message in a conversation
  Future<Message> sendMessage(String conversationId, String content) async {
    return await _bridge.sendMessage(conversationId, 'user', content);
  }

  /// Trigger AI generation of an assistant reply for the conversation
  Future<String> generateReply(String conversationId) async {
    return await _bridge.generateReply(conversationId);
  }
}

// Providers

/// Conversation repository provider
final conversationRepositoryProvider = Provider<ConversationRepository>((ref) {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return ConversationRepository(bridge);
});

/// Whether the sidebar is showing archived conversations (true) or active ones (false)
final showArchivedProvider = StateProvider<bool>((ref) => false);

/// Conversations list provider（跟随归档视图切换）
final conversationsProvider = FutureProvider<List<Conversation>>((ref) async {
  final repo = ref.read(conversationRepositoryProvider);
  final showArchived = ref.watch(showArchivedProvider);
  return showArchived ? await repo.getArchivedConversations() : await repo.getConversations();
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
