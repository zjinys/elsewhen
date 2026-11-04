import 'package:elsewhen_ui/bridge/api.dart' show ChatProposalDto, GoalDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';

/// 目标相关 widget 测试共用的桥接 fake：不触碰真实桥接。
///
/// - durable 能力全关（`supportsChatRuns` / `supportsChatQueue` = false），
///   目标讨论面板走非 durable 兼容路径；
/// - `ensureGoalChat` 返回确定性会话（id = `goal-conv-<goalId>`），
///   覆盖真实桥接调用；
/// - `listChatProposals` 覆写为空，让面板头部 ChatProposalReviewButton
///   探测不落到真实桥接；
/// - `createGoal` 捕获愿景文本，供「新建目标」底部表单落库断言。
class FakeGoalChatRepo extends RustBridgeRepository {
  FakeGoalChatRepo()
      : super(runBackgroundWorker: false, runChatQueueWorker: false);

  @override
  bool get supportsChatRuns => false;

  @override
  bool get supportsChatQueue => false;

  @override
  Future<Conversation> ensureGoalChat(String goalId) async => Conversation(
        id: 'goal-conv-$goalId',
        title: '目标讨论',
        tag: 'diary',
        createdAt: DateTime(2026),
        updatedAt: DateTime(2026),
        messageCount: 0,
        goalId: goalId,
      );

  @override
  Future<List<Message>> listMessages(String conversationId) async => messages;

  /// 非 durable 兼容路径的同步生成回复（选项块 UI 测试用）。
  @override
  Future<String> generateReply(
    String conversationId, {
    String? providerType,
    String? memoryType,
    int? memoryWindowSize,
  }) async =>
      reply;

  /// 选项块 UI 测试：预置带 elsewhen-choices 块的助手消息 / 生成回复。
  List<Message> messages = const [];
  String reply = '';

  @override
  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) async {
    final saved = Message(
      id: 'm-${(messages.length + 1).toRadixString(36)}',
      conversationId: conversationId,
      role: MessageRole.fromString(role),
      content: content,
      createdAt: DateTime(2026),
    );
    messages = [...messages, saved];
    return saved;
  }

  @override
  Future<List<ChatProposalDto>> listChatProposals(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) async => const [];

  /// 最近一次 createGoal 的愿景文本（null = 尚未调用）。
  String? createdContent;

  @override
  Future<Goal> createGoal({
    required String content,
    required GoalPhase phase,
    String? why,
    String? currentFocus,
    String? blocker,
    String? doneCriteria,
  }) async {
    createdContent = content;
    return Goal.fromDto(GoalDto(
      id: 'g-created',
      content: content,
      phase: phase.wire,
      status: 'active',
      why: why,
      currentFocus: currentFocus,
      blocker: blocker,
      doneCriteria: doneCriteria,
      createdAt: '2026-11-04T00:00:00Z',
      updatedAt: '2026-11-04T00:00:00Z',
    ));
  }
}
