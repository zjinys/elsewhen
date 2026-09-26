import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// 验证「AI 回复失败的重发入口」：『重新生成』按钮放在**最后一条用户消息**上，
/// 仅当该消息没有 AI 回复（生成失败/未生成）且当前不在生成中时显示。
/// ① 发送后 AI 失败：最后一条（用户）消息上出现「重新生成」+ 错误气泡文案；
/// ② 再点仍失败：按钮保留（可反复重试）；
/// ③ 重试成功：生成中占位出现（按钮隐藏）→ 完成后 AI 回复入库，
///    最后一条变成 AI 消息，按钮与错误气泡都消失。
/// 用 Completer 门控成功时机，固定步进 pump（菊花动画会让 pumpAndSettle 超时）。
void main() {
  testWidgets('最后一条用户消息(无AI回复)显示重新生成，生成成功隐藏', (tester) async {
    final repo = _FakeConversationRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => repo.messages),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pump();

    final input = find.byWidgetPredicate(
      (widget) =>
          widget is TextField &&
          (widget.decoration?.hintText == '输入消息...' ||
              widget.decoration?.hintText?.startsWith('AI 正在思考') == true),
    );
    expect(input, findsOneWidget);

    // ① 发送后 AI 失败：最后一条用户消息上出现「重新生成」
    await tester.enterText(input, '帮我记一下');
    await tester._pressEnter();
    await tester.pump();
    expect(repo.aiCalls, 1, reason: '发送后应触发一次 AI 生成');
    expect(repo.messages.length, 1, reason: '用户消息应已入库');
    expect(
      find.textContaining('AI 回复失败'),
      findsOneWidget,
      reason: '失败原因仍以会话内气泡呈现',
    );
    expect(
      find.text('重新生成'),
      findsOneWidget,
      reason: '最后一条用户消息（无 AI 回复）应显示重新生成',
    );

    // ② 点「重新生成」再次失败 → 按钮保留，可继续重试
    repo.failNext = true;
    await tester.tap(find.text('重新生成'));
    await tester.pump();
    await tester.pump();
    expect(repo.aiCalls, 2, reason: '点击重新生成应再次调用 AI');
    expect(find.text('重新生成'), findsOneWidget, reason: '再次失败后按钮应保留');
    expect(find.textContaining('AI 回复失败'), findsOneWidget);

    // ③ 第三次重试成功：生成中按钮隐藏 → 完成后 AI 回复到位、按钮消失
    final gate = Completer<String>();
    repo.gate = gate;
    repo.failNext = false;
    await tester.tap(find.text('重新生成'));
    await tester.pump();
    expect(repo.aiCalls, 3);
    expect(find.text('AI 正在思考…'), findsOneWidget, reason: '重试进行中应显示占位气泡');
    expect(find.text('重新生成'), findsNothing, reason: '生成中不显示重新生成按钮');

    gate.complete('收到！');
    // messagesProvider 是 async provider，普通 pump() 只推进 fake-async 一帧，
    // 等不到它重新解析新消息列表。runAsync 跳出 fake-async 让真实异步完成，
    // 再 pump 触发重建——最后一条变成 AI 回复，「重新生成」才消失。
    await tester.runAsync(() async {});
    await tester.pump();
    await tester.pump();
    expect(repo.messages.length, 2, reason: 'AI 回复应已入库');
    expect(find.text('AI 正在思考…'), findsNothing);
    expect(find.text('重新生成'), findsNothing, reason: '最后一条已是 AI 回复，无需重新生成');
    expect(find.textContaining('AI 回复失败'), findsNothing);
    expect(find.byIcon(Icons.arrow_upward), findsOneWidget, reason: '生成结束后发送按钮恢复');
  });
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());

  final List<Message> messages = [];
  int aiCalls = 0;

  /// 下一次 generateReply 是否失败；失败后置回 false
  bool failNext = true;

  /// 成功路径的完成闸门（null 时立即返回）
  Completer<String>? gate;

  @override
  Future<Message> sendMessage(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) async {
    final msg = Message(
      id: 'm${messages.length + 1}',
      conversationId: conversationId,
      parentMessageId: null,
      role: MessageRole.user,
      content: content,
      createdAt: DateTime.now(),
    );
    messages.add(msg);
    return msg;
  }

  // getMessages 必须覆写：基类实现会查询真实 bridge（主对话流合并），
  // 单测里 frb 未初始化会抛 StateError。这里直接返回内存消息列表。
  @override
  Future<List<Message>> getMessages(String conversationId) async => messages;

  @override
  Future<String> generateReply(String conversationId) async {
    aiCalls++;
    if (failNext) {
      failNext = false;
      throw Exception('AI 调用失败(模拟)');
    }
    final reply = await (gate?.future ?? Future.value('收到！'));
    messages.add(
      Message(
        id: 'ai$aiCalls',
        conversationId: conversationId,
        parentMessageId: null,
        role: MessageRole.assistant,
        content: reply,
        createdAt: DateTime.now(),
      ),
    );
    return reply;
  }
}

extension on WidgetTester {
  Future<void> _pressEnter() async {
    await sendKeyDownEvent(LogicalKeyboardKey.enter);
    await sendKeyUpEvent(LogicalKeyboardKey.enter);
    await pump();
  }
}
