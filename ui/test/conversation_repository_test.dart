import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';

void main() {
  group('ConversationRepository', () {
    late RustBridgeRepository bridge;
    late ConversationRepository repository;

    setUpAll(() async {
      bridge = RustBridgeRepository();
      await bridge.initialize();
      repository = ConversationRepository(bridge);
    });

    test('should create a new conversation', () async {
      final conversation = await repository.createConversation();

      expect(conversation.id, isNotEmpty);
      expect(conversation.messageCount, equals(0));
      expect(conversation.createdAt, isNotNull);
    });

    test('should list conversations', () async {
      final conversations = await repository.getConversations();

      expect(conversations, isNotNull);
      expect(conversations, isA<List<Conversation>>());
    });

    test('should send a message and retrieve it', () async {
      // Create a conversation
      final conversation = await repository.createConversation();

      // Send a message
      final message = await repository.sendMessage(
        conversation.id,
        'Hello from test',
      );

      expect(message.id, isNotEmpty);
      expect(message.conversationId, equals(conversation.id));
      expect(message.content, equals('Hello from test'));
      expect(message.role, equals(MessageRole.user));

      // Retrieve messages
      final messages = await repository.getMessages(conversation.id);

      expect(messages, isNotEmpty);
      expect(messages.first.content, equals('Hello from test'));
    });

    test('should get messages for a conversation', () async {
      final conversation = await repository.createConversation();
      await repository.sendMessage(conversation.id, 'First message');
      await repository.sendMessage(conversation.id, 'Second message');

      final messages = await repository.getMessages(conversation.id);

      expect(messages.length, equals(2));
      expect(messages[0].content, equals('First message'));
      expect(messages[1].content, equals('Second message'));
    });
  });
}
