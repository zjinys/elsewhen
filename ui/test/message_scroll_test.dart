import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// 验证：消息异步加载完成后，右侧消息列表应自动滚动到底部（最新消息可见），
/// 而不是停在顶部/随机位置（回归：loading 阶段空跑把 pending 状态消耗掉）。
void main() {
  testWidgets('消息加载完成后自动滚动到底部', (tester) async {
    final repo = _FakeConversationRepo();
    final completer = Completer<List<Message>>();
    final messages = List.generate(
      60,
      (i) => Message(
        id: 'm$i',
        conversationId: 'conv-1',
        role: i.isEven ? MessageRole.user : MessageRole.assistant,
        content:
            '第 $i 条消息，内容是足够长的中文文本以便撑满多个行宽，'
            '这里继续补充一些说明文字来确保气泡高度可观、列表可以滚动。'
            '（实际上有 60 条消息，但窗口默认只展示最近 20 条）',
        createdAt: DateTime(2026, 9, 15, 10, 0).add(Duration(minutes: i)),
      ),
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) => completer.future),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );

    // 数据未返回前：消息区显示 loading
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);

    // 数据到达 → ListView 渲染 → 应已自动滚到底部
    completer.complete(messages);
    await tester.pump(); // AsyncValue → data
    await tester.pump(); // 渲染 ListView
    await tester.pumpAndSettle(); // 等待滚动动画结束

    expect(find.byType(CircularProgressIndicator), findsNothing);
    final scrollable = tester.state<ScrollableState>(
      find
          .descendant(
            of: find.byType(ListView),
            matching: find.byType(Scrollable),
          )
          .first,
    );
    final pos = scrollable.position;
    expect(pos.maxScrollExtent, greaterThan(0), reason: '消息应先撑出滚动区');
    expect(
      pos.pixels,
      closeTo(pos.maxScrollExtent, 1.0),
      reason: '加载完成后应自动滚动到底部，看到最新消息',
    );

    // 更新同一会话数据（模拟收到新消息）：
    // 若之前在底部，应继续跟随到最新。
    pos.jumpTo(0); // 先模拟用户已在顶部/中部，避免干扰下一断言
  });
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());
}
