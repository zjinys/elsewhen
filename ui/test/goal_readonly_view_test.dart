import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
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
Widget _goalDomain({
  List<Goal> active = const [],
  List<Goal> paused = const [],
  List<Goal> completed = const [],
  bool atGoalTab = false,
  Widget? extraChild,
}) {
  return ProviderScope(
    overrides: [
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
                    child: GoalListView(readOnly: true),
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

  group('目标域只读总览（左侧导航一级入口）', () {
    testWidgets('只读模式：无新增表单、无操作按钮、无关闭按钮，内容完整呈现', (tester) async {
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

      // 活跃/暂停内容完整呈现；达成项默认隐藏在「已达成」分栏下，
      // 切换后再断言（与 GoalListView 的归档交互一致）
      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.textContaining('攻克多端同步快照收敛'), findsWidgets);
      expect(find.textContaining('学 Rust'), findsOneWidget);
      await tester.tap(find.text('已达成 (1)'));
      await tester.pumpAndSettle();
      expect(find.textContaining('搬家'), findsOneWidget);

      // 只读：无新增表单、无添加按钮、无关闭按钮
      expect(find.byType(TextField), findsNothing);
      expect(find.text('添加'), findsNothing);
      expect(find.byIcon(Icons.close), findsNothing);
      // 只读：无卡片操作按钮
      expect(find.byIcon(Icons.tune_outlined), findsNothing);
      expect(find.byIcon(Icons.more_horiz), findsNothing);
      expect(find.byIcon(Icons.play_arrow_outlined), findsNothing);
      expect(find.byIcon(Icons.refresh), findsNothing);
      // 历史归档分栏仍在；默认 tab 是暂停搁置，「搬家」需切到已达成
      expect(find.text('历史归档'), findsOneWidget);
      await tester.tap(find.text('已达成 (1)'));
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
      expect(find.text('发布 Elsewhen 1.0'), findsWidgets);
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
  });
}
