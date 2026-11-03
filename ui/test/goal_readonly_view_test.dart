import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/api.dart' show ChatProposalDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/goal_chat_panel.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
import 'package:elsewhen_ui/widgets/left_sidebar.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

api.GoalDto _dto({
  required String id,
  required String content,
  String phase = 'near',
  String status = 'active',
  String? why,
  String? currentFocus,
  String? blocker,
  String? doneCriteria,
  String? supersededAt,
}) {
  return api.GoalDto(
    id: id,
    content: content,
    phase: phase,
    status: status,
    why: why,
    currentFocus: currentFocus,
    blocker: blocker,
    doneCriteria: doneCriteria,
    createdAt: '2026-09-28T10:00:00Z',
    updatedAt: '2026-09-28T10:00:00Z',
    supersededAt: supersededAt,
  );
}

Goal _goal(
  String id,
  String content, {
  String phase = 'near',
  String status = 'active',
  String? why,
  String? currentFocus,
  String? blocker,
  String? doneCriteria,
}) =>
    Goal.fromDto(_dto(
      id: id,
      content: content,
      phase: phase,
      status: status,
      why: why,
      currentFocus: currentFocus,
      blocker: blocker,
      doneCriteria: doneCriteria,
    ));

/// 左侧导航「目标」域端到端：LeftSidebar（footer 入口 + 左栏摘要）+
/// 主区只读 GoalListView，与 main_screen 的挂载方式一致。
/// 目标会话 fake：不触碰真实桥接；durable 能力全关（走非 durable 路径）。
/// 覆写 listChatProposals，让面板头部的 ChatProposalReviewButton 探测
/// 不落到真实桥接。
class _FakeGoalChatRepo extends RustBridgeRepository {
  _FakeGoalChatRepo()
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

  /// 选项块 UI 测试：预置一条带 elsewhen-choices 块的助手消息。
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
}

Widget _goalDomain({
  List<Goal> active = const [],
  List<Goal> paused = const [],
  List<Goal> completed = const [],
  bool atGoalTab = false,
  Widget? extraChild,
  _FakeGoalChatRepo? goalChatRepo,
}) {
  return ProviderScope(
    overrides: [
      if (goalChatRepo != null)
        storageRepositoryProvider.overrideWithValue(goalChatRepo),
      if (atGoalTab)
        sidebarTabProvider.overrideWith(() => StateHolder(SidebarTab.goal)),
      activeGoalsProvider.overrideWith((ref) async => active),
      archivedGoalsProvider.overrideWith((ref) async => [...paused, ...completed]),
      pausedGoalsProvider.overrideWith((ref) async => paused),
      completedGoalsProvider.overrideWith((ref) async => completed),
      if (atGoalTab)
        conversationsProvider.overrideWith((ref) async => [
              Conversation(
                id: 'conv-main',
                title: '主对话流',
                tag: 'diary',
                createdAt: DateTime(2026),
                updatedAt: DateTime(2026),
                messageCount: 0,
              ),
            ]),
    ],
    child: MaterialApp(
      home: Scaffold(
        body: Stack(
          children: [
            Row(
              children: [
                SizedBox(
                  width: 280,
                  child: LeftSidebar(),
                ),
                const Expanded(
                  child: Align(
                    alignment: Alignment.topCenter,
                    child: GoalListView(),
                  ),
                ),
              ],
            ),
            ?extraChild,
          ],
        ),
      ),
    ),
  );
}

void main() {
  // LeftSidebar 在桌面 ≥700 宽时折叠成窄轨（rail），左栏摘要只在宽侧栏渲染。
  // 测试视口固定 640 宽，走完整宽侧栏形态。
  void useWideSidebar(WidgetTester tester) {
    tester.view.physicalSize = const Size(640, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
  }

  group('目标域工作台（左侧导航一级入口，可编辑）', () {
    testWidgets('工作台完整呈现：活跃竖排卡片 + 状态过滤 + 写入口可用', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(_goalDomain(
        active: [
          _goal('g1', '发布 Elsewhen 1.0',
              currentFocus: '攻克多端同步快照收敛', blocker: '网络分区下的分片停滞'),
          _goal('g2', '锻炼身体', phase: 'mid'),
        ],
        paused: [_goal('g3', '学 Rust', status: 'paused')],
        completed: [_goal('g4', '搬家', status: 'completed')],
      ));
      await tester.pumpAndSettle();

      // 缺省「活跃」过滤：活跃目标竖排完整呈现
      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.textContaining('攻克多端同步快照收敛'), findsWidgets);
      expect(find.text('锻炼身体'), findsOneWidget);

      // 可编辑工作台：新增表单、卡片级 AI 辅助、卡片操作菜单都在；
      // ⚙ 滚动抓手按钮已移除（入口收敛到 ⋯ 菜单与抓手条点击）
      expect(find.text('添加'), findsOneWidget);
      expect(find.byType(TextField), findsWidgets);
      expect(find.text('AI 辅助拆解'), findsWidgets);
      expect(find.byIcon(Icons.tune_outlined), findsNothing);
      expect(find.byIcon(Icons.more_horiz), findsWidgets);

      // 切到「暂停搁置」过滤：归档内容呈现，激活后仍可复活
      await tester.tap(find.textContaining('暂停搁置'));
      await tester.pumpAndSettle();
      expect(find.textContaining('学 Rust'), findsOneWidget);
      await tester.tap(find.textContaining('已达成'));
      await tester.pumpAndSettle();
      expect(find.textContaining('搬家'), findsOneWidget);
    });

    testWidgets('左栏目标摘要：活跃目标计数、愿景与抓手速览', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        active: [
          _goal('g1', '发布 Elsewhen 1.0',
              phase: 'near', currentFocus: '攻克多端同步快照收敛'),
          _goal('g2', '锻炼身体', phase: 'mid'),
        ],
      ));
      await tester.pumpAndSettle();

      // 左栏导航摘要可见
      expect(find.text('活跃目标 2 / 3'), findsOneWidget);
      // 左栏卡片与主区各渲染一次 → 用 findsWidgets
      expect(find.text('发布 Elsewhen 1.0'), findsWidgets); // tab 芯片 + 卡片
      expect(find.textContaining('攻克多端同步快照收敛'), findsWidgets);
      expect(find.text('锻炼身体'), findsWidgets);
    });

    testWidgets('左栏目标摘要：空态给出指引', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(_goalDomain(atGoalTab: true));
      await tester.pumpAndSettle();

      expect(find.textContaining('还没有设定目标'), findsOneWidget);
    });
  });

  group('AI 共同立项入口（目标页）', () {
    testWidgets('点击后预填对话草稿并切回对话 tab，不自动发送', (tester) async {
      useWideSidebar(tester);
      SidebarTab? capturedTab;
      String? capturedDraft;
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        active: [
          _goal('g1', '发布 Elsewhen 1.0', currentFocus: '攻兡多端同步快照收敛'),
        ],
        extraChild: Consumer(builder: (context, ref, _) {
          capturedTab = ref.watch(sidebarTabProvider);
          capturedDraft = ref.watch(goalCompassChatDraftProvider('conv-main'));
          // 预热会话列表：startGoalCoAuthor 同步读 .value，未加载时会走
          // 「无会话」分支（真实应用中对话页/侧栏早已 watch 过它）。
          ref.watch(conversationsProvider);
          return const SizedBox.shrink();
        }),
      ));
      await tester.pumpAndSettle();

      expect(capturedTab, SidebarTab.goal);
      expect(capturedDraft, isNull);

      await tester.tap(find.text('和 AI 拆解 / 立新目标'));
      await tester.pumpAndSettle();

      // 切回对话 tab，且种子写入对话草稿（输入框监听后预填，不自动发送）
      expect(capturedTab, SidebarTab.conversation);
      expect(capturedDraft, isNotNull);
      expect(capturedDraft, contains('发布 Elsewhen 1.0'));
      expect(capturedDraft, contains('攻兡多端同步快照收敛'));
      expect(tester.takeException(), isNull);
    });

    testWidgets('活跃 3 条时禁用共同立项入口', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        active: [
          _goal('g1', '目标一'),
          _goal('g2', '目标二'),
          _goal('g3', '目标三'),
        ],
      ));
      await tester.pumpAndSettle();

      final btn = tester.widget<TextButton>(
        find.ancestor(
          of: find.text('和 AI 拆解 / 立新目标'),
          matching: find.byType(TextButton),
        ),
      );
      expect(btn.onPressed, isNull);
    });

    testWidgets('多目标竖排：每张卡都有独立 AI 辅助入口，点击卡内展开讨论面板，不切对话 tab，种子只预填草稿', (tester) async {
      useWideSidebar(tester);
      String? capturedDraft;
      SidebarTab? capturedTab;
      final repo = _FakeGoalChatRepo();
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        goalChatRepo: repo,
        active: [
          _goal('g1', '发布 Elsewhen 1.0', currentFocus: '攻兡快照收敛'),
          _goal('g2', '锻炼身体', phase: 'mid'),
        ],
        extraChild: Consumer(builder: (context, ref, _) {
          capturedDraft = ref.watch(goalCompassChatDraftProvider('conv-main'));
          capturedTab = ref.watch(sidebarTabProvider);
          ref.watch(conversationsProvider);
          return const SizedBox.shrink();
        }),
      ));
      await tester.pumpAndSettle();

      // 两张卡片各自带 AI 辅助入口；主区各渲染一次 + 左栏摘要
      expect(find.text('发布 Elsewhen 1.0'), findsNWidgets(2));
      expect(find.text('锻炼身体'), findsNWidgets(2));
      expect(find.text('AI 辅助拆解'), findsNWidgets(2));
      expect(find.byType(GoalChatPanel), findsNothing);

      // 点第二张卡片的 AI 辅助：目标页内直接展开 GoalChatPanel，
      // 不切对话 tab，对话草稿保持为空
      await tester.tap(find.text('AI 辅助拆解').last);
      await tester.pumpAndSettle();

      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(capturedTab, SidebarTab.goal);
      expect(capturedDraft, isNull);
      // 种子只预填面板输入框草稿（不自动发送）：fake 的 ensureGoalChat
      // 返回 goal-conv-g2，面板输入框应出现带 g2 上下文的种子文本
      final fields = tester.widgetList<TextField>(find.byType(TextField));
      final seedText = fields
          .map((f) => f.controller?.text ?? '')
          .where((t) => t.isNotEmpty)
          .join('\n');
      expect(seedText, contains('锻炼身体'));
      expect(seedText, isNot(contains('发布 Elsewhen 1.0')));

      // 再点收起（按钮文本切换）→ 面板折叠
      await tester.tap(find.text('收起 AI 讨论'));
      await tester.pumpAndSettle();
      expect(find.byType(GoalChatPanel), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('可操作选项 UI：elsewhen-choices 块渲染成点选按钮，点击即发送选项原文',
        (tester) async {
      useWideSidebar(tester);
      final repo = _FakeGoalChatRepo()
        ..messages = [
          Message(
            id: 'm-ai-1',
            conversationId: 'goal-conv-g1',
            role: MessageRole.assistant,
            content: '关于署名方式，想确认你的边界：\n\n```elsewhen-choices\n'
                '[{"label": "A：可以，愿意用自己的名字说话"}, '
                '{"label": "B：可以用小号或匿名，不露脸"}]\n```',
            createdAt: DateTime(2026),
          ),
        ];
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        goalChatRepo: repo,
        active: [_goal('g1', '发布 Elsewhen 1.0')],
      ));
      await tester.pumpAndSettle();

      // 展开讨论面板（主区卡片上的入口，取 last 避开左栏摘要）
      await tester.tap(find.text('AI 辅助拆解').last);
      await tester.pumpAndSettle();

      // 选项渲染成可点击 chip；原始 JSON 与围栏不渲染
      expect(find.text('A：可以，愿意用自己的名字说话'), findsOneWidget);
      expect(find.text('B：可以用小号或匿名，不露脸'), findsOneWidget);
      expect(find.textContaining('elsewhen-choices'), findsNothing);
      expect(find.textContaining('"label"'), findsNothing);

      // 点击选项 → 选项原文作为用户消息发送（非 durable 路径同步生成）
      await tester.tap(find.text('B：可以用小号或匿名，不露脸'));
      await tester.pumpAndSettle();

      final sent = repo.messages.where((m) => m.isUser).toList();
      expect(sent, isNotEmpty);
      expect(sent.last.content, 'B：可以用小号或匿名，不露脸');
      expect(tester.takeException(), isNull);
    });

    testWidgets('宽屏分栏：右栏占位提示；点 AI 辅助拆解后在右栏展开讨论，窄屏回退卡内嵌',
        (tester) async {
      // 主区宽度 ≥ 760（总视口 1200 − 侧栏 280）触发分栏
      tester.view.physicalSize = const Size(1200, 1000);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);
      final repo = _FakeGoalChatRepo();
      await tester.pumpWidget(_goalDomain(
        atGoalTab: true,
        goalChatRepo: repo,
        active: [_goal('g1', '发布 Elsewhen 1.0')],
      ));
      await tester.pumpAndSettle();

      // 未展开时右栏给占位提示
      expect(find.textContaining('点击左侧卡片的「AI 辅助拆解」'), findsOneWidget);
      expect(find.byType(GoalChatPanel), findsNothing);

      // 展开后面板在右栏（fillHeight），卡内不内嵌
      await tester.tap(find.text('AI 辅助拆解'));
      await tester.pumpAndSettle();
      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(find.textContaining('点击左侧卡片的「AI 辅助拆解」'), findsNothing);

      // 收起 → 回到占位
      await tester.tap(find.text('收起 AI 讨论'));
      await tester.pumpAndSettle();
      expect(find.byType(GoalChatPanel), findsNothing);
      expect(find.textContaining('点击左侧卡片的「AI 辅助拆解」'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });
}
