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

/// 验证「发送后后台执行反馈」：
/// 用户发送消息后，消息列表尾部立即出现「AI 正在生成回复…」占位气泡、
/// 发送按钮转菊花禁用；AI 生成完成（成功入库）后占位消失、按钮恢复。
/// 用 Completer 控制 generateReply 完成时机，避免测试期间气泡/菊花造成的
/// 无限动画卡住 pumpAndSettle（统一用固定步进 pump）。
void main() {
  testWidgets('发送后 AI 生成期间显示占位气泡，完成后消失', (tester) async {
    final repo = _FakeConversationRepo();
    final replyGate = Completer<String>();
    repo.replyCompleter = replyGate;

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => <Message>[]),
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

    // 输入并发送（回车），generateReply 挂起在 replyGate 上 → 生成中
    await tester.enterText(input, 'hello');
    await tester._pressEnter();

    // ① 生成中反馈：占位气泡 + 发送按钮转菊花 + 输入框提示变化
    expect(find.text('AI 正在思考…'), findsOneWidget, reason: '发送后应立即出现「生成中」占位气泡');
    expect(
      find.byIcon(Icons.arrow_upward),
      findsNothing,
      reason: '生成中发送按钮应转菊花（禁用）',
    );
    final hint = tester.widget<TextField>(input).decoration?.hintText;
    expect(hint, 'AI 正在思考，您可以先输入下一条消息…');

    // ② 生成完成：占位消失、按钮恢复
    replyGate.complete('ok');
    await tester.pump(); // generateReply 返回 + invalidate + 清除生成状态
    await tester.pump(); // 重建
    expect(find.text('AI 正在思考…'), findsNothing, reason: 'AI 回复到位后占位气泡应消失');
    expect(
      find.byIcon(Icons.arrow_upward),
      findsOneWidget,
      reason: '生成完成后发送按钮恢复',
    );
    expect(tester.widget<TextField>(input).decoration?.hintText, '输入消息...');
  });
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());

  /// 控制 generateReply 何时完成（null 时立即返回成功）
  Completer<String>? replyCompleter;

  @override
  Future<Message> sendMessage(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) async {
    return Message(
      id: 'm1',
      conversationId: conversationId,
      parentMessageId: null,
      role: MessageRole.user,
      content: content,
      createdAt: DateTime.now(),
    );
  }

  @override
  Future<String> generateReply(String conversationId) async {
    return await (replyCompleter?.future ?? Future.value('ok'));
  }
}

extension on WidgetTester {
  Future<void> _pressEnter() async {
    await sendKeyDownEvent(LogicalKeyboardKey.enter);
    await sendKeyUpEvent(LogicalKeyboardKey.enter);
    await pump();
  }
}
