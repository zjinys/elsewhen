import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// 习惯打卡回答（habit_checkin）附着在问候卡片上显示（「已记录：…」），
/// 不再重复渲染一条「用户发的」气泡；问候卡被窗口裁掉时回退为普通消息。
void main() {
  const nudgeRef = 'nudge-1:2026-10-23';

  Message nudge() => Message(
    id: 'n1',
    conversationId: 'conv-1',
    role: MessageRole.assistant,
    content: '今天中午饭后散步习惯了吗?',
    kind: 'habit_nudge',
    messageRef: nudgeRef,
    createdAt: DateTime(2026, 10, 23, 14, 16),
  );

  Message checkin() => Message(
    id: 'c1',
    conversationId: 'conv-1',
    parentMessageId: 'n1',
    role: MessageRole.user,
    content: '今天饭后散步习惯了',
    kind: 'habit_checkin',
    messageRef: nudgeRef,
    createdAt: DateTime(2026, 10, 23, 14, 42),
  );

  Message filler(int i) => Message(
    id: 'f$i',
    conversationId: 'conv-1',
    role: i.isEven ? MessageRole.user : MessageRole.assistant,
    content: '填充消息 $i',
    createdAt: DateTime(2026, 10, 23, 15, 0).add(Duration(minutes: i)),
  );

  Future<void> pump(WidgetTester tester, List<Message> messages) async {
    await tester.binding.setSurfaceSize(const Size(900, 4000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(
            _FakeConversationRepo(),
          ),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => messages),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('checkin hides beside its nudge card', (tester) async {
    await pump(tester, [nudge(), checkin()]);

    // 回答附着在问候卡上（「已记录：…」），不再出现独立的用户气泡。
    expect(find.text('已记录：今天饭后散步习惯了'), findsOneWidget);
    expect(find.text('今天饭后散步习惯了'), findsNothing);
    expect(find.textContaining('重新生成'), findsNothing);
  });

  testWidgets(
    'checkin falls back to a plain bubble when its card was trimmed away',
    (tester) async {
      // nudge(0)、checkin(1)、再 19 条填充 = 21 条；默认窗口是最近 20 条，
      // 恰好把 nudge 裁出窗口而 checkin 仍在窗口内。
      final messages = [nudge(), checkin(), ...List.generate(19, filler)];
      await pump(tester, messages);

      // 卡不在窗口里：回答必须仍以普通消息显示，不能跟着消失。
      expect(find.text('今天饭后散步习惯了'), findsOneWidget);
      expect(find.text('已记录：今天饭后散步习惯了'), findsNothing);

      // 展开更早消息后卡片可见，回答随即附着到卡片上隐藏本体。
      await tester.tap(find.text('显示更早的消息'));
      await tester.pumpAndSettle();

      expect(find.text('今天中午饭后散步习惯了吗?'), findsOneWidget);
      expect(find.text('已记录：今天饭后散步习惯了'), findsOneWidget);
      expect(find.text('今天饭后散步习惯了'), findsNothing);
    },
  );
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());
}
