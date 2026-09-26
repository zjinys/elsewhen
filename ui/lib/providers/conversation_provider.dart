import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'state_holder.dart';

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
    final ordinary = [
      ...active,
      ...archived,
    ].where((conversation) => !conversation.isWikiChat).toList();
    final isMain = ordinary.any(
      (conversation) =>
          conversation.id == conversationId && conversation.title == '主对话流',
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

  /// 纯读：找出主对话流（不含知识页对话），不存在返回 null。**不写库**。
  Future<Conversation?> findMainConversation() async {
    final conversations = await _bridge.listConversations();
    final main = conversations.where((c) => !c.isWikiChat).toList();
    if (main.isEmpty) return null;
    // The reserved title makes the identity stable across restarts instead
    // of relying on whichever conversation happens to be newest.
    final marked = main.where((c) => c.title == '主对话流').toList();
    if (marked.isNotEmpty) return marked.first;
    main.sort((a, b) => a.createdAt.compareTo(b.createdAt));
    return main.first;
  }

  /// **写操作**：确保主对话流存在且带保留标题。只在启动期显式调一次
  /// （见 appInitializationProvider）——此前由 mainConversationProvider 的
  /// compute 函数顺带触发，导致每次 invalidate/rebuild 都可能重跑
  /// rename/create，且完成时机脱离 widget 生命周期（P18）。
  Future<Conversation> ensureMainConversation() async {
    final existing = await findMainConversation();
    if (existing == null) {
      return _bridge.createConversation(title: '主对话流', tag: 'diary');
    }
    if (existing.title == '主对话流') return existing;
    await _bridge.renameConversation(existing.id, '主对话流');
    return Conversation(
      id: existing.id,
      title: '主对话流',
      tag: existing.tag,
      createdAt: existing.createdAt,
      updatedAt: existing.updatedAt,
      messageCount: existing.messageCount,
      lastMessagePreview: existing.lastMessagePreview,
      archived: existing.archived,
      wikiPageSlug: existing.wikiPageSlug,
    );
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

  Future<String> confirmKnowledgeDraft(
    String conversationId,
    String actionId,
  ) => _bridge.confirmKnowledgeDraft(conversationId, actionId);

  Future<void> declineKnowledgeDraft(String conversationId, String actionId) =>
      _bridge.declineKnowledgeDraft(conversationId, actionId);

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
final showArchivedProvider = NotifierProvider<StateHolder<bool>, bool>(
  () => StateHolder(false),
);

/// Conversations list provider（跟随归档视图切换）
final conversationsProvider = FutureProvider<List<Conversation>>((ref) async {
  final repo = ref.read(conversationRepositoryProvider);
  final showArchived = ref.watch(showArchivedProvider);
  return showArchived
      ? await repo.getArchivedConversations()
      : await repo.getConversations();
});

/// 主对话流：**纯读**。建会话/改名这类写操作只在启动期走
/// [ConversationRepository.ensureMainConversation]（appInitializationProvider
/// 里显式调一次），provider 自身不再改库——否则任意 invalidate 都会重跑
/// 一次写路径，且其完成时机脱离 widget 生命周期（P18）。
final mainConversationProvider = FutureProvider<Conversation>((ref) async {
  final main = await ref.read(conversationRepositoryProvider).findMainConversation();
  if (main == null) {
    // 正常路径到不了这里：启动期已 ensure 过。落到这里说明初始化未完成
    // 或主对话被删除——显式报错，不在 provider 内静默补建。
    throw StateError('主对话流不存在：应用初始化未完成或主对话已被删除');
  }
  return main;
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
  final key =
      '${today.year.toString().padLeft(4, '0')}-'
      '${today.month.toString().padLeft(2, '0')}-'
      '${today.day.toString().padLeft(2, '0')}';
  for (final item in usage) {
    if (item.date == key) return item;
  }
  return null;
});

/// Selected conversation ID provider
final selectedConversationIdProvider =
    NotifierProvider<StateHolder<String?>, String?>(() => StateHolder(null));

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
final conversationNoticeProvider =
    NotifierProvider<
      StateHolder<Map<String, List<String>>>,
      Map<String, List<String>>
    >(() => StateHolder(const {}));

/// 追加一条会话临时提示（需在 widget 内调用，传入可选 ref 直接取 notifier）
void addConversationNotice(WidgetRef ref, String conversationId, String text) {
  ref
      .read(conversationNoticeProvider.notifier)
      .update(
        (current) => {
          ...current,
          conversationId: [...(current[conversationId] ?? const []), text],
        },
      );
}

/// 清空某会话的全部临时提示（如用户重新发送消息时）
void clearConversationNotices(WidgetRef ref, String conversationId) {
  ref.read(conversationNoticeProvider.notifier).update((current) {
    if (!current.containsKey(conversationId)) return current;
    return Map<String, List<String>>.from(current)..remove(conversationId);
  });
}

/// 滚动请求信号：临时提示追加后消息列表需滚到底部。
/// 因为提示不写库、不触发 messagesProvider 变化，通知消息列表自行滚动。
final scrollRequestProvider = NotifierProvider<StateHolder<int>, int>(
  () => StateHolder(0),
);

/// 生成失败标记：conversationId → 该轮失败时**待回复的那条 user 消息 id**。
/// 内存态、不写库。它让「重新生成」入口跟着消息 id 走，而不是跟着列表位置
/// 走——位置条件（是不是最后一条）会被追加的消息、搜索/日期筛选改掉，
/// 一旦不成立，失败就变成看得见却再也点不到（P9）。
final failedReplyMessageIdProvider =
    NotifierProvider<StateHolder<Map<String, String>>, Map<String, String>>(
      () => StateHolder(const {}),
    );

/// 记下本轮生成失败对应的消息（重新生成成功、或用户发新消息时清掉）。
void markReplyFailed(WidgetRef ref, String conversationId, String messageId) {
  ref
      .read(failedReplyMessageIdProvider.notifier)
      .update((current) => {...current, conversationId: messageId});
}

void clearReplyFailed(WidgetRef ref, String conversationId) {
  ref.read(failedReplyMessageIdProvider.notifier).update((current) {
    if (!current.containsKey(conversationId)) return current;
    return Map<String, String>.from(current)..remove(conversationId);
  });
}

/// 正在生成 AI 回复的会话集合（内存态，仅用于 UI 反馈，不写库）。
/// 「发送消息 → AI 在后台生成」期间让消息列表展示生成中占位气泡、
/// 输入区发送按钮转菊花，直到生成结束（成功入库或失败）清除。
final aiGeneratingProvider =
    NotifierProvider<StateHolder<Set<String>>, Set<String>>(
      () => StateHolder(const {}),
    );

/// 标记某会话进入/离开「AI 生成中」状态（需在 widget 存活时调用）
void setAiGenerating(WidgetRef ref, String conversationId, bool generating) {
  ref.read(aiGeneratingProvider.notifier).update((current) {
    final next = {...current};
    if (generating) {
      next.add(conversationId);
    } else {
      next.remove(conversationId);
    }
    return next;
  });
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
final messageInputProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder(''),
);
