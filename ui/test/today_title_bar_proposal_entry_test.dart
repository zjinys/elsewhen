import 'package:elsewhen_ui/bridge/generated.dart/api/chat_proposals.dart'
    show ChatProposalDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/todo_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/screens/main_screen.dart';
import 'package:elsewhen_ui/widgets/chat_proposal_review.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 移动端标题栏「今天」行必须有「审阅提议」入口：_NowStatus 整栏在移动端
/// 隐藏，输入区/联系人 banner 文案都指向「审阅提议」，缺入口则提议永远
/// pending（真机 2026-10-30）。
void main() {
  Future<ProviderContainer> mount(WidgetTester tester, FakeRepo repo) async {
    final container = ProviderContainer(
      overrides: [
        conversationRepositoryProvider.overrideWithValue(repo),
        selectedConversationIdProvider.overrideWith(() => StateHolder('c1')),
        activeGoalsProvider.overrideWith((ref) async => const []),
        todosProvider.overrideWith((ref) async => const []),
      ],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: TodayTitleBarActions())),
      ),
    );
    await tester.pumpAndSettle();
    return container;
  }

  testWidgets('移动端今天行出现审阅提议入口（含待审徽标）', (tester) async {
    final repo = FakeRepo()..pending = 1;
    await mount(tester, repo);
    expect(find.byType(ChatProposalReviewButton), findsOneWidget);
    // 有待审提议时 Badge 出现（与桌面同款按钮）。
    expect(find.byType(Badge), findsOneWidget);
  });

  testWidgets('无会话时不显示审阅提议入口', (tester) async {
    final container = ProviderContainer(
      overrides: [
        conversationRepositoryProvider
            .overrideWithValue(FakeRepo()..pending = 1),
        selectedConversationIdProvider.overrideWith(() => StateHolder(null)),
        activeGoalsProvider.overrideWith((ref) async => const []),
        todosProvider.overrideWith((ref) async => const []),
      ],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: TodayTitleBarActions())),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byType(ChatProposalReviewButton), findsNothing);
  });
}

class FakeRepo extends ConversationRepository {
  FakeRepo() : super(RustBridgeRepository());
  int pending = 0;

  @override
  Future<List<ChatProposalDto>> listChatProposals(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) async => List.filled(pending, _stub(), growable: false);

  /// 徽标只看 rows.isNotEmpty，行内容不会被判读；构造器参数太多，
  /// 这里用本地反射式占位不现实——改为最小真实 DTO。
  static ChatProposalDto _stub() => ChatProposalDto(
    kind: 'action',
    id: 'stub',
    conversationId: 'c1',
    action: 'create_todo',
    argsJson: '{}',
    version: 1,
    createdAt: '2026-10-30T00:00:00Z',
    approvalToken: 'stub-token',
    targetBasisJson: '[]',
  );
}
