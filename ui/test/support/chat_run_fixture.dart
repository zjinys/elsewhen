import 'dart:convert';

import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';

ChatRunDto fixtureRun({
  String id = 'run-1',
  String conversation = 'conv-1',
  String input = 'input-1',
  String status = 'running',
  int sequence = 1,
  String content = '',
}) => ChatRunDto(
  id: id,
  conversationId: conversation,
  inputMessageId: input,
  status: status,
  phase: status == 'running' ? 'requesting' : status,
  sequence: sequence,
  requestCount: 1,
  providerBasisJson: '[]',
  content: content,
  finalMessageId: status == 'completed' ? 'answer-$id' : null,
  createdAt: '2026-10-01T00:00:00Z',
  updatedAt: '2026-10-01T00:00:00Z',
);

ChatRunEventDto fixtureEvent(
  int sequence,
  String kind, {
  String run = 'run-1',
  Map<String, Object?> payload = const {},
}) => ChatRunEventDto(
  runId: run,
  sequence: sequence,
  kind: kind,
  payloadJson: jsonEncode(payload),
);

class RunTestRepository extends ConversationRepository {
  RunTestRepository() : super(RustBridgeRepository());
  final Map<String, ChatRunDto> runs = {};
  final List<ChatRunEventDto> events = [];
  final List<(String, String, String)> starts = [];
  final List<(String, String)> stops = [];
  final List<Message> messages = [];
  final List<int> pageLimits = [];
  final Map<String, ChatRunDto> requests = {};
  bool loseStartResponse = false;
  int snapshots = 0;
  int legacyCalls = 0;
  Future<ChatRunEventPageDto> Function(String, String, int)? pageOverride;

  @override
  bool get supportsChatRuns => true;
  @override
  Future<List<Message>> getMessages(String conversationId) async =>
      List.of(messages);
  @override
  Future<List<Conversation>> getConversations() async => [];
  @override
  Future<List<Conversation>> getArchivedConversations() async => [];
  @override
  Future<List<dynamic>> listPendingActions(String conversationId) async => [];
  @override
  Future<Message> sendMessage(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) async {
    final message = Message(
      id: 'input-${messages.length + 1}',
      conversationId: conversationId,
      role: MessageRole.user,
      content: content,
      createdAt: DateTime(2026, 10, 1),
    );
    messages.add(message);
    return message;
  }

  @override
  Future<String> generateReply(String conversationId) async {
    legacyCalls++;
    throw StateError('Durable chat must not use the legacy path');
  }

  @override
  Future<ChatRunDto?> getLatestChatRun(String conversationId) async =>
      runs[conversationId];
  @override
  Future<ChatRunDto?> getChatRun(String conversationId, String runId) async {
    snapshots++;
    final run = runs[conversationId];
    return run?.id == runId ? run : null;
  }

  @override
  Future<ChatRunDto> startChatRun(
    String conversationId,
    String inputMessageId,
    String idempotencyKey,
  ) async {
    starts.add((conversationId, inputMessageId, idempotencyKey));
    final run = requests.putIfAbsent(
      idempotencyKey,
      () => fixtureRun(
        id: 'run-${requests.length + 1}',
        conversation: conversationId,
        input: inputMessageId,
      ),
    );
    runs[conversationId] = run;
    if (loseStartResponse) {
      loseStartResponse = false;
      throw StateError('synthetic lost response');
    }
    return run;
  }

  @override
  Future<ChatRunEventPageDto> listChatRunEvents(
    String conversationId,
    String runId,
    int afterSequence, {
    int limit = 64,
  }) async {
    pageLimits.add(limit);
    if (pageOverride != null) {
      return pageOverride!(conversationId, runId, afterSequence);
    }
    final remaining = events
        .where((e) => e.runId == runId && e.sequence > afterSequence)
        .toList();
    final page = remaining.take(limit).toList();
    return ChatRunEventPageDto(
      runId: runId,
      events: page,
      nextSequence: page.isEmpty ? afterSequence : page.last.sequence,
      hasMore: remaining.length > page.length,
    );
  }

  @override
  Future<ChatRunDto?> cancelChatRun(String conversationId, String runId) async {
    stops.add((conversationId, runId));
    final run = runs[conversationId];
    if (run == null || run.id != runId) return null;
    final cancelled = fixtureRun(
      id: run.id,
      conversation: conversationId,
      input: run.inputMessageId,
      status: 'cancelled',
      sequence: run.sequence + 1,
      content: run.content,
    );
    runs[conversationId] = cancelled;
    return cancelled;
  }
}
