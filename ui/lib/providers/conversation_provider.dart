import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/conversation.dart';
import '../bridge/rust_bridge_repository.dart';
import '../bridge/generated.dart/api.dart' show MessageRecordabilityDto;
import '../models/token_usage.dart';

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
    final active = await _bridge.listConversations();
    final archived = await _bridge.listArchivedConversations();
    final ordinary = [...active, ...archived]
        .where((conversation) => !conversation.isWikiChat)
        .toList();
    final isMain = ordinary.any(
      (conversation) =>
          conversation.id == conversationId &&
          conversation.title == '主对话流',
    );
    if (!isMain) return _bridge.listMessages(conversationId);

    final batches = await Future.wait(
      ordinary.map((conversation) => _bridge.listMessages(conversation.id)),
    );
    final messages = batches.expand((batch) => batch).toList()
      ..sort((a, b) => a.createdAt.compareTo(b.createdAt));
    return messages;
  }

  /// Create a new conversation
  Future<Conversation> createConversation() async {
    return await _bridge.createConversation();
  }

  /// Return the single user-facing main conversation, creating it on first use.
  /// Knowledge-page chats are deliberately excluded from this entry point.
  Future<Conversation> ensureMainConversation() async {
    final conversations = await _bridge.listConversations();
    final main = conversations.where((c) => !c.isWikiChat).toList();
    if (main.isNotEmpty) {
      // The reserved title makes the identity stable across restarts instead
      // of relying on whichever conversation happens to be newest.
      final marked = main.where((c) => c.title == '主对话流').toList();
      if (marked.isNotEmpty) return marked.first;
      main.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      final legacyMain = main.first;
      await _bridge.renameConversation(legacyMain.id, '主对话流');
      return Conversation(
        id: legacyMain.id,
        title: '主对话流',
        tag: legacyMain.tag,
        createdAt: legacyMain.createdAt,
        updatedAt: legacyMain.updatedAt,
        messageCount: legacyMain.messageCount,
        lastMessagePreview: legacyMain.lastMessagePreview,
        archived: legacyMain.archived,
        wikiPageSlug: legacyMain.wikiPageSlug,
      );
    }
    return _bridge.createConversation(title: '主对话流', tag: 'diary');
  }

  /// Rename a conversation
  Future<void> renameConversation(String conversationId, String title) async {
    return await _bridge.renameConversation(conversationId, title);
  }

  /// Archive or unarchive a conversation
  Future<void> setArchived(String conversationId, bool archived) async {
    return await _bridge.setConversationArchived(conversationId, archived);
  }

  Future<bool> deleteArchived(String conversationId) =>
      _bridge.deleteArchivedConversation(conversationId);

  Future<List<dynamic>> listPendingActions(String conversationId) =>
      _bridge.listPendingActions(conversationId);

  Future<bool> updatePendingActionArgs(String actionId, String argsJson) =>
      _bridge.updatePendingActionArgs(actionId, argsJson);

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

final mainConversationProvider = FutureProvider<Conversation>((ref) async {
  return ref.read(conversationRepositoryProvider).ensureMainConversation();
});

final analysisJobStatsProvider = FutureProvider((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return bridge.getAnalysisJobStats();
});

final activeAiProviderProvider = FutureProvider<String?>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  final providers = await bridge.listAiProviderConfigs();
  for (final provider in providers) {
    if (provider.isActive) return provider.name;
  }
  return null;
});

final todayTokenUsageProvider = FutureProvider<DailyTokenUsage?>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  final usage = await bridge.getDailyTokenUsage(1);
  if (usage.isEmpty) return null;
  final today = DateTime.now();
  final key = '${today.year.toString().padLeft(4, '0')}-'
      '${today.month.toString().padLeft(2, '0')}-'
      '${today.day.toString().padLeft(2, '0')}';
  for (final item in usage) {
    if (item.date == key) return item;
  }
  return null;
});

/// Selected conversation ID provider
final selectedConversationIdProvider = StateProvider<String?>((ref) => null);

final messageRecordabilityProvider =
    FutureProvider.family<MessageRecordabilityDto?, String>((
      ref,
      messageId,
    ) async {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.getMessageRecordability(messageId);
    });

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

final pendingActionsProvider = FutureProvider<List<dynamic>>((ref) async {
  final conversationId = ref.watch(selectedConversationIdProvider);
  if (conversationId == null) return const [];
  return ref
      .read(conversationRepositoryProvider)
      .listPendingActions(conversationId);
});

/// Message input provider
final messageInputProvider = StateProvider<String>((ref) => '');
