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
  Future<Message> sendMessage(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) async {
    return await _bridge.submitConversationInput(
      conversationId,
      content,
      idempotencyKey: idempotencyKey,
    );
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
  return showArchived
      ? await repo.getArchivedConversations()
      : await repo.getConversations();
});

/// Selected conversation ID provider
final selectedConversationIdProvider = StateProvider<String?>((ref) => null);

/// 会话内的临时提示气泡（如 AI 回复失败）。
/// 仅存在于内存、不写库 —— 不会进入对话历史、记忆注入或后续 AI 上下文。
/// key 为 conversationId；发送新消息或切库刷新后即可清空。
final conversationNoticeProvider = StateProvider<Map<String, List<String>>>(
  (ref) => const {},
);

/// 追加一条会话临时提示（需在 widget 内调用，传入可选 ref 直接取 notifier）
void addConversationNotice(WidgetRef ref, String conversationId, String text) {
  final notifier = ref.read(conversationNoticeProvider.notifier);
  final current = notifier.state;
  notifier.state = {
    ...current,
    conversationId: [...(current[conversationId] ?? const []), text],
  };
}

/// 清空某会话的全部临时提示（如用户重新发送消息时）
void clearConversationNotices(WidgetRef ref, String conversationId) {
  final notifier = ref.read(conversationNoticeProvider.notifier);
  if (!notifier.state.containsKey(conversationId)) return;
  final next = Map<String, List<String>>.from(notifier.state)
    ..remove(conversationId);
  notifier.state = next;
}

/// 滚动请求信号：临时提示追加后消息列表需滚到底部。
/// 因为提示不写库、不触发 messagesProvider 变化，通知消息列表自行滚动。
final scrollRequestProvider = StateProvider<int>((ref) => 0);

/// 正在生成 AI 回复的会话集合（内存态，仅用于 UI 反馈，不写库）。
/// 「发送消息 → AI 在后台生成」期间让消息列表展示生成中占位气泡、
/// 输入区发送按钮转菊花，直到生成结束（成功入库或失败）清除。
final aiGeneratingProvider = StateProvider<Set<String>>((ref) => const {});

/// 标记某会话进入/离开「AI 生成中」状态（需在 widget 存活时调用）
void setAiGenerating(WidgetRef ref, String conversationId, bool generating) {
  final notifier = ref.read(aiGeneratingProvider.notifier);
  final current = notifier.state;
  final next = {...current};
  if (generating) {
    next.add(conversationId);
  } else {
    next.remove(conversationId);
  }
  notifier.state = next;
}

/// Messages for selected conversation provider
final messagesProvider = FutureProvider<List<Message>>((ref) async {
  final conversationId = ref.watch(selectedConversationIdProvider);
  if (conversationId == null) return [];

  final repo = ref.read(conversationRepositoryProvider);
  return await repo.getMessages(conversationId);
});

/// Message input provider
final messageInputProvider = StateProvider<String>((ref) => '');
