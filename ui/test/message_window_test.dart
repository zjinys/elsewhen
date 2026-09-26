import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';

/// 消息窗口化验证：默认只显示最近 N 条，点「显示更早的消息」逐页向前展开，
/// 直到全部显示后按钮消失。用假 repo + 假消息列表，无需 FFI。
void main() {
  testWidgets('message window: separates messages by day', (tester) async {
    final repo = _FakeConversationRepo();
    final messages = [
      Message(
        id: 'm1',
        conversationId: 'conv-1',
        role: MessageRole.user,
        content: '第一天的消息',
        createdAt: DateTime(2026, 9, 21, 23, 59),
      ),
      Message(
        id: 'm2',
        conversationId: 'conv-1',
        parentMessageId: 'm1',
        role: MessageRole.assistant,
        content: '第二天的消息',
        createdAt: DateTime(2026, 9, 22, 0, 1),
      ),
    ];

    await tester.binding.setSurfaceSize(const Size(900, 1200));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => messages),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('2026年09月21日'), findsOneWidget);
    expect(find.text('2026年09月22日'), findsOneWidget);
    expect(find.byType(Divider), findsNWidgets(4), reason: '每个日期标题两侧各有一条横线');
  });

  testWidgets('message window: show last 20, expand by page, button hides', (
    tester,
  ) async {
    final repo = _FakeConversationRepo();
    final total = 45;
    final messages = List.generate(
      total,
      (i) => Message(
        id: 'm$i',
        conversationId: 'conv-1',
        parentMessageId: i == 0 ? null : 'm${i - 1}',
        role: i.isEven ? MessageRole.user : MessageRole.assistant,
        content: '第 $i 条消息内容',
        createdAt: DateTime(2026, 9, 15, 10, 0).add(Duration(minutes: i)),
      ),
    );

    // 大视口：窗口内所有条目都会被真实构建，便于精确计数
    await tester.binding.setSurfaceSize(const Size(900, 6000));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => messages),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();

    // 初始：只看最近 20 条（第 25~44 条），顶部有「显示更早」按钮
    // 气泡分型：偶数序号=user（SelectableText），奇数=assistant（MarkdownView）
    expect(
      find.byWidgetPredicate((w) => w is SelectableText || w is MarkdownView),
      findsNWidgets(20),
      reason: '默认窗口为最近 20 条',
    );
    expect(find.text('第 44 条消息内容'), findsOneWidget, reason: '最新消息可见');
    expect(
      find.text('第 25 条消息内容', findRichText: true),
      findsOneWidget,
      reason: '窗口最旧的一条可见',
    );
    expect(find.text('第 24 条消息内容'), findsNothing, reason: '第 24 条在窗口外');
    expect(find.text('第 0 条消息内容'), findsNothing, reason: '最早消息被折叠');
    expect(find.text('显示更早的消息'), findsOneWidget);

    // 第一次点「显示更早」：窗口扩展到 40 条（第 5~44 条）
    await tester.tap(find.text('显示更早的消息'));
    await tester.pumpAndSettle();
    expect(
      find.byWidgetPredicate((w) => w is SelectableText || w is MarkdownView),
      findsNWidgets(40),
      reason: '向前展开一页',
    );
    expect(
      find.text('第 5 条消息内容', findRichText: true),
      findsOneWidget,
      reason: '新增的最早一条可见',
    );
    expect(find.text('第 4 条消息内容'), findsNothing, reason: '第 4 条仍被折叠');
    expect(find.text('第 0 条消息内容'), findsNothing);
    expect(find.text('显示更早的消息'), findsOneWidget, reason: '还有更早消息，按钮保留');

    // 第二次点：全部 45 条，按钮消失
    await tester.tap(find.text('显示更早的消息'));
    await tester.pumpAndSettle();
    expect(
      find.byWidgetPredicate((w) => w is SelectableText || w is MarkdownView),
      findsNWidgets(45),
      reason: '全部消息已展示',
    );
    expect(find.text('第 0 条消息内容'), findsOneWidget);
    expect(find.text('显示更早的消息'), findsNothing, reason: '没有更早消息，按钮隐藏');
  });
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());
}
