/// Conversation model
class Conversation {
  final String id;
  final String? title; // null for untitled conversations
  final String? tag; // category tag for organization
  final DateTime createdAt;
  final DateTime updatedAt;
  final int messageCount;
  final String? lastMessagePreview;
  final bool archived;
  final String? wikiPageSlug;

  const Conversation({
    required this.id,
    this.title,
    this.tag,
    required this.createdAt,
    required this.updatedAt,
    required this.messageCount,
    this.lastMessagePreview,
    this.archived = false,
    this.wikiPageSlug,
  });

  factory Conversation.fromRust(Map<String, dynamic> dto) {
    return Conversation(
      id: dto['id'] as String,
      title: dto['title'] as String?,
      tag: dto['tag'] as String?,
      createdAt: DateTime.parse(dto['created_at'] as String).toLocal(),
      updatedAt: DateTime.parse(dto['updated_at'] as String).toLocal(),
      messageCount: dto['message_count'] as int,
      lastMessagePreview: dto['last_message_preview'] as String?,
      archived: dto['archived'] as bool? ?? false,
    );
  }

  String get displayTitle => title ?? '新对话';

  /// 是否为知识页处理会话（关联了知识页）
  bool get isWikiChat => wikiPageSlug != null;
}

/// Message role enum
enum MessageRole {
  user,
  assistant,
  system;

  factory MessageRole.fromString(String role) {
    switch (role.toLowerCase()) {
      case 'user':
        return MessageRole.user;
      case 'assistant':
        return MessageRole.assistant;
      case 'system':
        return MessageRole.system;
      default:
        throw ArgumentError('Unknown role: $role');
    }
  }
}

/// Message model
class Message {
  final String id;
  final String conversationId;
  final String? parentMessageId;
  final MessageRole role;
  final String content;
  final DateTime createdAt;
  final Map<String, dynamic>? metadata;
  /// 展示配对：assistant 回复对应的输入消息 id（仅展示排序用）。
  final String? inputMessageId;

  const Message({
    required this.id,
    required this.conversationId,
    this.parentMessageId,
    required this.role,
    required this.content,
    required this.createdAt,
    this.metadata,
    this.inputMessageId,
  });

  factory Message.fromRust(Map<String, dynamic> dto) {
    return Message(
      id: dto['id'] as String,
      conversationId: dto['conversation_id'] as String,
      parentMessageId: dto['parent_message_id'] as String?,
      role: MessageRole.fromString(dto['role'] as String),
      content: dto['content'] as String,
      createdAt: DateTime.parse(dto['created_at'] as String).toLocal(),
      metadata: dto['metadata'] as Map<String, dynamic>?,
      inputMessageId: dto['input_message_id'] as String?,
    );
  }

  bool get isUser => role == MessageRole.user;
  bool get isAssistant => role == MessageRole.assistant;

  /// 展示序比较器：assistant 回复锚定到其输入消息（inputMessageId），
  /// 队列连发场景保证一问一答交错；锚条缺失时退回自身时间。
  /// 同锚点 user 恒在 assistant 前；多条回复按自身时间稳定排序。
  static int displayOrder(List<Message> messages, Message a, Message b) {
    final byId = {
      for (final m in messages) m.id: m,
    };
    DateTime anchor(Message m) {
      final input = m.inputMessageId == null ? null : byId[m.inputMessageId!];
      return input?.createdAt ?? m.createdAt;
    }

    final ka = anchor(a);
    final kb = anchor(b);
    if (!ka.isAtSameMomentAs(kb)) return ka.compareTo(kb);
    final ra = a.isAssistant ? 1 : 0;
    final rb = b.isAssistant ? 1 : 0;
    if (ra != rb) return ra.compareTo(rb);
    return a.createdAt.compareTo(b.createdAt);
  }

  /// Check if this message has a parent (is part of a branched conversation)
  bool get hasParent => parentMessageId != null;
}
