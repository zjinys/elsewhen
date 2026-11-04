import 'package:elsewhen_ui/bridge/api.dart' as api;
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

import 'support/fake_goal_repo.dart';

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
}) => Goal.fromDto(
  _dto(
    id: id,
    content: content,
    phase: phase,
    status: status,
    why: why,
    currentFocus: currentFocus,
    blocker: blocker,
    doneCriteria: doneCriteria,
  ),
);

/// 左侧导航「目标」域端到端：LeftSidebar（左栏即工作台清单）+
/// 主区 GoalListView（详情 | AI 讨论），与 main_screen 的挂载方式一致。
/// 桥接 fake 默认注入：主区详情会自动挂载 GoalChatPanel。
Widget _goalDomain({
  List<Goal> active = const [],
  List<Goal> paused = const [],
  List<Goal> completed = const [],
  bool atGoalTab = false,
  Widget? extraChild,
  FakeGoalChatRepo? goalChatRepo,
}) {
  return ProviderScope(
    overrides: [
      storageRepositoryProvider.overrideWithValue(
        goalChatRepo ?? FakeGoalChatRepo(),
      ),
      if (atGoalTab)
        sidebarTabProvider.overrideWith(() => StateHolder(SidebarTab.goal)),
      activeGoalsProvider.overrideWith((ref) async => active),
      archivedGoalsProvider.overrideWith(
        (ref) async => [...paused, ...completed],
      ),
      pausedGoalsProvider.overrideWith((ref) async => paused),
      completedGoalsProvider.overrideWith((ref) async => completed),
      if (atGoalTab)
        conversationsProvider.overrideWith(
          (ref) async => [
            Conversation(
              id: 'conv-main',
              title: '主对话流',
              tag: 'diary',
              createdAt: DateTime(2026),
              updatedAt: DateTime(2026),
              messageCount: 0,
            ),
          ],
        ),
    ],
    child: MaterialApp(
      home: Scaffold(
        body: Stack(
          children: [
            Row(
              children: [
                const SizedBox(width: 280, child: LeftSidebar()),
                const Expanded(child: GoalListView()),
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
  // 视口 640：完整宽侧栏（桌面 ≥700 会折叠成窄轨，清单退回主区）。
  void useWideSidebar(WidgetTester tester) {
    tester.view.physicalSize = const Size(640, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
  }

  // 视口 1200：主区足够宽，详情与 AI 讨论左右并排。
  void useWideLayout(WidgetTester tester) {
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
  }

  group('目标域工作台（左侧导航一级入口）', () {
    testWidgets('进入目标 tab：左栏即完整工作台，主区「详情 | 讨论」就位', (tester) async {
      useWideLayout(tester);
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          active: [
            _goal(
              'g1',
              '发布 Elsewhen 1.0',
              currentFocus: '攻克多端同步快照收敛',
              blocker: '网络分区下的分片停滞',
            ),
            _goal('g2', '锻炼身体', phase: 'mid'),
          ],
          paused: [_goal('g3', '学 Rust', status: 'paused')],
          completed: [_goal('g4', '搬家', status: 'completed')],
        ),
      );
      await tester.pumpAndSettle();

      // 左栏：标题「目标」/计数/右上「新建」/清单（原主区中列整体搬到左栏；
      // 「目标」同时是 footer 导航项名，用计数徽章与「新建」按钮定位面板）
      expect(find.text('2 / 3'), findsOneWidget);
      expect(find.text('新建'), findsOneWidget);
      expect(find.text('发布 Elsewhen 1.0'), findsWidgets);
      expect(find.text('锻炼身体'), findsWidgets);
      expect(find.text('和 AI 拆解 / 立新目标'), findsOneWidget);
      expect(find.text('添加'), findsNothing); // 表单不常驻

      // 主区自动选中首条活跃目标：详情 + 讨论就位，占位提示消失
      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(find.textContaining('当前抓手：'), findsOneWidget);
      expect(find.textContaining('网络分区下的分片停滞'), findsOneWidget);
      expect(find.textContaining('在左侧「目标」列表选择一个目标'), findsNothing);
      expect(find.byIcon(Icons.more_horiz), findsWidgets);
      // ⚙ 滚动抓手按钮已移除（入口收敛到 ⋯ 菜单与抓手条点击）
      expect(find.byIcon(Icons.tune_outlined), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('左栏过滤：暂停搁置/已达成清单，读档唤醒回显动力回顾', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          active: [_goal('g1', '主线任务')],
          paused: [
            Goal.fromDto(
              _dto(
                id: 'g-paused',
                content: '学法语',
                status: 'paused',
                why: '想去巴黎旅行交流',
                currentFocus: '背完 500 个基础词汇',
                blocker: '缺少日常发音纠正环境',
                supersededAt: '2026-10-01T12:00:00Z',
              ),
            ),
          ],
          completed: [
            Goal.fromDto(
              _dto(
                id: 'g-done',
                content: '考过驾照',
                status: 'completed',
                why: '方便自驾出游',
                doneCriteria: '科目四通过并拿本',
                supersededAt: '2026-09-15T12:00:00Z',
              ),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      // 缺省看活跃：暂停/已达成不在清单里
      expect(find.text('主线任务'), findsWidgets);
      expect(find.text('学法语'), findsNothing);

      // 切到「暂停搁置」：清单出现，点击选中进主区详情
      await tester.tap(find.textContaining('暂停搁置'));
      await tester.pumpAndSettle();
      expect(find.text('学法语'), findsOneWidget);
      expect(find.text('暂停'), findsOneWidget);

      await tester.tap(find.text('学法语'));
      await tester.pumpAndSettle();
      expect(find.textContaining('目标讨论 · 学法语'), findsOneWidget);

      // ⋯ 菜单 → 重新激活（读档唤醒）：回显历史动力与抓手
      await tester.tap(find.byIcon(Icons.more_horiz));
      await tester.pumpAndSettle();
      await tester.tap(find.text('重新激活（读档唤醒）'));
      await tester.pumpAndSettle();

      expect(find.text('读档唤醒 · 学法语'), findsOneWidget);
      expect(find.textContaining('💡 核心动力回顾：想去巴黎旅行交流'), findsOneWidget);
      expect(find.widgetWithText(TextField, '背完 500 个基础词汇'), findsOneWidget);
      expect(find.widgetWithText(TextField, '缺少日常发音纠正环境'), findsOneWidget);
      // 长切片文本：读档唤醒字段同样多行自适应
      final wakeFocus = tester.widget<TextField>(
        find.widgetWithText(TextField, '背完 500 个基础词汇'),
      );
      expect(wakeFocus.minLines, 2);
      expect(wakeFocus.maxLines, 8);
      expect(find.text('确认唤醒'), findsOneWidget);
      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();

      // 切到「已达成」：成就清单出现
      await tester.tap(find.textContaining('已达成'));
      await tester.pumpAndSettle();
      expect(find.text('考过驾照'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('左栏空态：新建入口打开底部表单并落库', (tester) async {
      useWideSidebar(tester);
      final repo = FakeGoalChatRepo();
      await tester.pumpWidget(_goalDomain(atGoalTab: true, goalChatRepo: repo));
      await tester.pumpAndSettle();

      // 左栏空态指引；主区占位（无目标可选中）
      expect(find.textContaining('还没有活跃目标'), findsOneWidget);
      expect(find.textContaining('在左侧「目标」列表选择一个目标'), findsOneWidget);

      await tester.tap(find.text('新建'));
      await tester.pumpAndSettle();

      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNotNull);

      await tester.enterText(find.byType(TextField).first, '发 1.0');
      await tester.tap(find.text('添加'));
      await tester.pumpAndSettle();

      expect(repo.createdContent, '发 1.0');
      expect(find.widgetWithText(FilledButton, '添加'), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });

  group('AI 共同立项入口（目标页）', () {
    testWidgets('点击后预填对话草稿并切回对话 tab，不自动发送', (tester) async {
      useWideSidebar(tester);
      SidebarTab? capturedTab;
      String? capturedDraft;
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          active: [_goal('g1', '发布 Elsewhen 1.0', currentFocus: '攻克多端同步快照收敛')],
          extraChild: Consumer(
            builder: (context, ref, _) {
              capturedTab = ref.watch(sidebarTabProvider);
              capturedDraft = ref.watch(
                goalCompassChatDraftProvider('conv-main'),
              );
              // 预热会话列表：startGoalCoAuthor 同步读 .value，未加载时会走
              // 「无会话」分支（真实应用中对话页/侧栏早已 watch 过它）。
              ref.watch(conversationsProvider);
              return const SizedBox.shrink();
            },
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(capturedTab, SidebarTab.goal);
      expect(capturedDraft, isNull);

      await tester.tap(find.text('和 AI 拆解 / 立新目标'));
      await tester.pumpAndSettle();

      // 切回对话 tab，且种子写入对话草稿（输入框监听后预填，不自动发送）
      expect(capturedTab, SidebarTab.conversation);
      expect(capturedDraft, isNotNull);
      expect(capturedDraft, contains('发布 Elsewhen 1.0'));
      expect(capturedDraft, contains('攻克多端同步快照收敛'));
      expect(tester.takeException(), isNull);
    });

    testWidgets('活跃 3 条时共同立项入口仍可点（满额新目标将落「暂停搁置」）', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          active: [_goal('g1', '目标一'), _goal('g2', '目标二'), _goal('g3', '目标三')],
        ),
      );
      await tester.pumpAndSettle();

      // 与「新建」同语义：满额不阻断入口；经 AI 立项的新目标由后端
      // 自动落「暂停搁置」（见 create_goal_with_facets 满额语义）。
      final btn = tester.widget<TextButton>(
        find.ancestor(
          of: find.text('和 AI 拆解 / 立新目标'),
          matching: find.byType(TextButton),
        ),
      );
      expect(btn.onPressed, isNotNull);
    });
  });

  group('主区讨论（选中驱动）', () {
    testWidgets('选择目标切换主区讨论：输入框始终留空，不预填草稿', (tester) async {
      useWideSidebar(tester);
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          active: [
            _goal('g1', '发布 Elsewhen 1.0', currentFocus: '攻克多端同步快照收敛'),
            _goal('g2', '锻炼身体', phase: 'mid'),
          ],
        ),
      );
      await tester.pumpAndSettle();

      // 自动选中 g1：讨论面板就位、输入框保持空白
      expect(find.byType(GoalChatPanel), findsOneWidget);
      String chatDraft() => tester
          .widgetList<TextField>(find.byType(TextField))
          .map((f) => f.controller?.text ?? '')
          .where((t) => t.isNotEmpty)
          .join('\n');
      expect(chatDraft(), isEmpty);

      // 点左栏 g2：讨论面板切到 g2，输入框依旧空白（不预填种子）
      await tester.tap(find.text('锻炼身体'));
      await tester.pumpAndSettle();
      expect(find.textContaining('目标讨论 · 锻炼身体'), findsOneWidget);
      expect(chatDraft(), isEmpty);

      // 再切回 g1：依旧空白
      await tester.tap(find.text('发布 Elsewhen 1.0').first);
      await tester.pumpAndSettle();
      expect(find.textContaining('目标讨论 · 发布 Elsewhen 1.0'), findsOneWidget);
      expect(chatDraft(), isEmpty);
      expect(tester.takeException(), isNull);
    });

    testWidgets('可操作选项 UI：elsewhen-choices 块渲染成点选按钮，点击即发送选项原文', (tester) async {
      useWideSidebar(tester);
      final repo = FakeGoalChatRepo()
        ..messages = [
          Message(
            id: 'm-ai-1',
            conversationId: 'goal-conv-g1',
            role: MessageRole.assistant,
            content:
                '关于署名方式，想确认你的边界：\n\n```elsewhen-choices\n'
                '[{"label": "A：可以，愿意用自己的名字说话"}, '
                '{"label": "B：可以用小号或匿名，不露脸"}]\n```',
            createdAt: DateTime(2026),
          ),
        ];
      await tester.pumpWidget(
        _goalDomain(
          atGoalTab: true,
          goalChatRepo: repo,
          active: [_goal('g1', '发布 Elsewhen 1.0')],
        ),
      );
      await tester.pumpAndSettle();

      // 自动选中 g1，讨论面板直接就位（无需再点「AI 辅助拆解」）
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

    testWidgets('宽屏：无活跃目标时主区给占位，不自动打开讨论', (tester) async {
      useWideLayout(tester);
      await tester.pumpWidget(_goalDomain(atGoalTab: true));
      await tester.pumpAndSettle();

      expect(find.textContaining('在左侧「目标」列表选择一个目标'), findsOneWidget);
      expect(find.byType(GoalChatPanel), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });
}
