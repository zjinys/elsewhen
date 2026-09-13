/// Conversation model
class Conversation {
  final String id;
  final String? title; // null for untitled conversations
  final DateTime createdAt;
  final DateTime updatedAt;
  final int messageCount;
  final String? lastMessagePreview;

  const Conversation({
    required this.id,
    this.title,
    required this.createdAt,
    required this.updatedAt,
    required this.messageCount,
    this.lastMessagePreview,
  });

  factory Conversation.fromRust(Map<String, dynamic> dto) {
    return Conversation(
      id: dto['id'] as String,
      title: dto['title'] as String?,
      createdAt: DateTime.parse(dto['created_at'] as String),
      updatedAt: DateTime.parse(dto['updated_at'] as String),
      messageCount: dto['message_count'] as int,
      lastMessagePreview: dto['last_message_preview'] as String?,
    );
  }

  String get displayTitle => title ?? '新对话';
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
  final MessageRole role;
  final String content;
  final DateTime createdAt;
  final Map<String, dynamic>? metadata;

  const Message({
    required this.id,
    required this.conversationId,
    required this.role,
    required this.content,
    required this.createdAt,
    this.metadata,
  });

  factory Message.fromRust(Map<String, dynamic> dto) {
    return Message(
      id: dto['id'] as String,
      conversationId: dto['conversation_id'] as String,
      role: MessageRole.fromString(dto['role'] as String),
      content: dto['content'] as String,
      createdAt: DateTime.parse(dto['created_at'] as String),
      metadata: dto['metadata'] as Map<String, dynamic>?,
    );
  }

  bool get isUser => role == MessageRole.user;
  bool get isAssistant => role == MessageRole.assistant;
}
