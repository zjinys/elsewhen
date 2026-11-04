import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/goal_chat_panel.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
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

/// 目标 provider 覆写：三态清单 + 桥接 fake（主区详情会自动挂载
/// GoalChatPanel，测试必须不触碰真实桥接）。
// 注：riverpod 3.4+ 不再导出 Override 类型，用 ProviderScope.overrides 的元素类型
// （错报时以 analyzer 实际报错为准）；这里用 dynamic 元素列表避免引用私有类型。
// ignore: strict_top_level_inference
_goalOverrides({
  List<Goal> active = const [],
  List<Goal> archived = const [],
  RustBridgeRepository? repository,
}) {
  return [
    storageRepositoryProvider.overrideWithValue(
      repository ?? FakeGoalChatRepo(),
    ),
    activeGoalsProvider.overrideWith((ref) async => active),
    archivedGoalsProvider.overrideWith((ref) async => archived),
    pausedGoalsProvider.overrideWith(
      (ref) async =>
          archived.where((g) => g.status != GoalStatus.completed).toList(),
    ),
    completedGoalsProvider.overrideWith(
      (ref) async =>
          archived.where((g) => g.status == GoalStatus.completed).toList(),
    ),
  ];
}

/// 左栏工作台（GoalWorkbenchPanel）：独立挂载，不起整个左侧栏。
Widget _workbench({
  List<Goal> active = const [],
  List<Goal> archived = const [],
  FakeGoalChatRepo? repository,
}) {
  return ProviderScope(
    overrides: _goalOverrides(
      active: active,
      archived: archived,
      repository: repository,
    ),
    child: const MaterialApp(home: Scaffold(body: GoalWorkbenchPanel())),
  );
}

/// 主区（GoalListView）：详情 + AI 讨论。部分用例放宽到 1200 视口，
/// 避开桌面 700~1024 的窄轨形态（那会让主区左侧多渲染一份清单）。
Widget _mainView({
  List<Goal> active = const [],
  List<Goal> archived = const [],
  FakeGoalChatRepo? repository,
}) {
  return ProviderScope(
    overrides: _goalOverrides(
      active: active,
      archived: archived,
      repository: repository,
    ),
    child: const MaterialApp(home: Scaffold(body: GoalListView())),
  );
}

void main() {
  void useWideLayout(WidgetTester tester) {
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
  }

  group('GoalWorkbenchPanel（左栏工作台）', () {
    testWidgets('标题「目标」+ 右上「新建」按钮；过滤多选且单行三枚', (tester) async {
      await tester.pumpWidget(
        _workbench(
          active: [
            _goal('g1', '发布 Elsewhen 1.0', currentFocus: '攻克多端同步快照收敛'),
            _goal('g2', '锻炼身体', phase: 'mid'),
          ],
          archived: [
            _goal('g3', '学 Rust', status: 'paused'),
            _goal('g4', '搬家', status: 'completed'),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('目标'), findsOneWidget);
      expect(find.text('长程目标与航向'), findsNothing);
      expect(find.text('2 / 3'), findsOneWidget);
      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.text('锻炼身体'), findsOneWidget);
      expect(find.text('和 AI 拆解 / 立新目标'), findsOneWidget);
      // 新建按钮在标题行右侧，且始终可点
      final newButton = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '新建'),
      );
      expect(newButton.onPressed, isNotNull);
      // 表单不常驻：看不到愿景输入与「添加」
      expect(find.text('添加'), findsNothing);
      expect(find.byType(TextField), findsNothing);

      // 状态胶囊必须同处一行（三个标签顶边对齐）
      final activeTop = tester.getTopLeft(find.text('活跃')).dy;
      expect(tester.getTopLeft(find.text('暂停搁置')).dy, activeTop);
      expect(tester.getTopLeft(find.text('已达成')).dy, activeTop);

      // 多选：叠加展示暂停/已达成清单；取消活跃后其清单收起
      await tester.tap(find.text('暂停搁置'));
      await tester.pumpAndSettle();
      expect(find.text('学 Rust'), findsOneWidget);
      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget); // 活跃仍选中

      await tester.tap(find.text('已达成').first);
      await tester.pumpAndSettle();
      expect(find.text('搬家'), findsOneWidget);

      await tester.tap(find.text('活跃'));
      await tester.pumpAndSettle();
      expect(find.text('发布 Elsewhen 1.0'), findsNothing);
      expect(find.text('学 Rust'), findsOneWidget);
      expect(find.text('搬家'), findsOneWidget);

      // 最后一个选中项不可被取消：过滤集永不为空
      await tester.tap(find.text('暂停搁置'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('已达成').first);
      await tester.pumpAndSettle();
      expect(find.text('搬家'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('空态：说明 + 「新建」打开底部表单并落库', (tester) async {
      final repo = FakeGoalChatRepo();
      await tester.pumpWidget(_workbench(repository: repo));
      await tester.pumpAndSettle();

      expect(find.textContaining('还没有活跃目标'), findsOneWidget);

      await tester.tap(find.text('新建'));
      await tester.pumpAndSettle();

      // 底部表单打开：添加可用；落库后自动关闭并提示
      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNotNull);

      await tester.enterText(find.byType(TextField).first, '发 1.0');
      await tester.tap(find.text('添加'));
      await tester.pumpAndSettle();

      expect(repo.createdContent, '发 1.0');
      expect(find.widgetWithText(FilledButton, '添加'), findsNothing);
      expect(find.textContaining('已设立新愿景'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('活跃已满 3 条：新建按钮仍可点，表单说明将以暂停搁置创建', (tester) async {
      final repo = FakeGoalChatRepo();
      await tester.pumpWidget(
        _workbench(
          active: [
            _goal('g1', '发 1.0'),
            _goal('g2', '读完一本书', phase: 'mid'),
            _goal('g3', '财务自由', phase: 'long'),
          ],
          repository: repo,
        ),
      );
      await tester.pumpAndSettle();

      final newButton = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '新建'),
      );
      expect(newButton.onPressed, isNotNull);

      await tester.tap(find.text('新建'));
      await tester.pumpAndSettle();

      // 表单可用（不再禁用），文案说明满额语义：新目标落「暂停搁置」
      expect(find.textContaining('活跃已满，新目标将先放入「暂停搁置」'), findsOneWidget);
      expect(find.textContaining('将以「暂停搁置」状态创建，可随时读档唤醒'), findsOneWidget);
      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNotNull);

      await tester.enterText(find.byType(TextField).first, '第四个愿景');
      await tester.tap(find.text('添加'));
      await tester.pumpAndSettle();

      expect(repo.createdContent, '第四个愿景');
      expect(find.textContaining('已放入「暂停搁置」'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('活跃目标按阶段展示中文标签', (tester) async {
      await tester.pumpWidget(
        _workbench(
          active: [
            _goal('g1', '近期的事', phase: 'near'),
            _goal('g2', '中期的事', phase: 'mid'),
            _goal('g3', '长远的事', phase: 'long'),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('近期'), findsWidgets);
      expect(find.text('中期'), findsWidgets);
      expect(find.text('长远'), findsWidgets);
      expect(find.text('近期的事'), findsOneWidget);
      expect(find.text('中期的事'), findsOneWidget);
      expect(find.text('长远的事'), findsOneWidget);
    });
  });

  group('GoalListView（主区：详情 | AI 讨论）', () {
    testWidgets('自动选中首个活跃目标：详情高亮切片 + 讨论面板并排', (tester) async {
      useWideLayout(tester);
      await tester.pumpWidget(
        _mainView(
          active: [
            _goal(
              'g1',
              '发布 Elsewhen 1.0',
              phase: 'near',
              why: '验证个人认知系统闭环',
              currentFocus: '攻克多端同步快照收敛',
              blocker: '网络环境证书配置繁琐',
              doneCriteria: '双端自动化测试全绿',
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('发布 Elsewhen 1.0'), findsWidgets); // 详情卡 + 讨论面板标题
      expect(find.textContaining('攻克多端同步快照收敛'), findsOneWidget); // 抓手条
      expect(find.textContaining('网络环境证书配置繁琐'), findsOneWidget);
      expect(find.textContaining('验证个人认知系统闭环'), findsOneWidget);
      expect(find.textContaining('双端自动化测试全绿'), findsOneWidget);
      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(find.textContaining('在左侧'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('点击抓手条弹出滚动抓手对话框', (tester) async {
      useWideLayout(tester);
      await tester.pumpWidget(
        _mainView(
          active: [
            _goal(
              'g1',
              '发布 Elsewhen 1.0',
              currentFocus: '首发切片 A',
              blocker: '卡点 B',
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      // 点击抓手区域弹出滚动抓手对话框
      await tester.tap(find.textContaining('首发切片 A'));
      await tester.pumpAndSettle();

      expect(find.text('滚动抓手 · 发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.widgetWithText(TextField, '首发切片 A'), findsOneWidget);
      expect(find.widgetWithText(TextField, '卡点 B'), findsOneWidget);
      expect(find.text('更新抓手'), findsOneWidget);
      // 长切片文本（AI 共同立项可能写入长句）：字段多行自适应显示
      final focusField = tester.widget<TextField>(
        find.widgetWithText(TextField, '首发切片 A'),
      );
      expect(focusField.minLines, 2);
      expect(focusField.maxLines, 8);
    });

    testWidgets('没有活跃目标：主区占位；窄轨宽度下清单退回主区左侧', (tester) async {
      // 默认视口 800 宽：桌面平台命中 700~1024 窄轨形态
      await tester.pumpWidget(_mainView());
      await tester.pumpAndSettle();

      expect(find.textContaining('在左侧「目标」列表选择一个目标'), findsOneWidget);
      expect(find.text('目标'), findsOneWidget); // 窄轨退回的清单标题
      expect(find.byType(GoalChatPanel), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });

  group('速览弹窗（quickGlance）', () {
    testWidgets('只读无写入口无 AI 入口，仅活跃目标', (tester) async {
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            activeGoalsProvider.overrideWith(
              (ref) async => [
                _goal(
                  'g1',
                  '发布 Elsewhen 1.0',
                  currentFocus: '攻克快照收敛',
                  blocker: '证书配置繁琐',
                ),
              ],
            ),
            archivedGoalsProvider.overrideWith((ref) async => const []),
            pausedGoalsProvider.overrideWith((ref) async => const []),
            completedGoalsProvider.overrideWith((ref) async => const []),
          ],
          child: MaterialApp(
            home: Scaffold(
              body: Builder(
                builder: (context) => Center(
                  child: TextButton(
                    onPressed: () => openGoalDialog(context),
                    child: const Text('打开目标面板'),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 打开目标面板（真实 openGoalDialog 路径，quickGlance 形态）
      await tester.tap(find.text('打开目标面板'));
      await tester.pumpAndSettle();

      // 活跃目标切片完整呈现
      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.textContaining('攻克快照收敛'), findsOneWidget);
      expect(find.textContaining('证书配置繁琐'), findsOneWidget);

      // 零写入口、零 AI 入口、无历史归档、无新增表单
      expect(find.text('AI 辅助拆解'), findsNothing);
      expect(find.textContaining('和 AI'), findsNothing);
      expect(find.text('添加'), findsNothing);
      expect(find.text('新建目标'), findsNothing);
      expect(find.byIcon(Icons.tune_outlined), findsNothing);
      expect(find.byIcon(Icons.more_horiz), findsNothing);
      expect(find.byType(TextField), findsNothing);
      expect(find.textContaining('暂停搁置'), findsNothing);
      expect(find.textContaining('已达成'), findsNothing);
      expect(find.textContaining('活跃 ('), findsNothing);

      expect(tester.takeException(), isNull);
    });
  });

  group('「今天」栏目标入口', () {
    Future<void> pumpTodayBar(
      WidgetTester tester, {
      List<Goal> active = const [],
      Size size = const Size(900, 900),
    }) async {
      await tester.binding.setSurfaceSize(size);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            conversationRepositoryProvider.overrideWithValue(_FakeRepo()),
            selectedConversationIdProvider.overrideWith(
              () => StateHolder('conv-1'),
            ),
            messagesProvider.overrideWith((ref) async => []),
            activeGoalsProvider.overrideWith((ref) async => active),
            archivedGoalsProvider.overrideWith((ref) async => const []),
            pausedGoalsProvider.overrideWith((ref) async => const []),
            completedGoalsProvider.overrideWith((ref) async => const []),
          ],
          child: const MaterialApp(home: Scaffold(body: MessageArea())),
        ),
      );
      await tester.pumpAndSettle();
    }

    // 目标没有独立一级 tab，这行是唯一入口，空态下消失就等于功能不可发现。
    testWidgets('0 条目标时入口仍可见', (tester) async {
      await pumpTodayBar(tester);
      expect(find.text('目标'), findsOneWidget);
    });

    testWidgets('有目标时显示条数', (tester) async {
      await pumpTodayBar(
        tester,
        active: [
          _goal('g1', '发 1.0'),
          _goal('g2', '读书', phase: 'mid'),
        ],
      );
      expect(find.text('目标2'), findsOneWidget);
    });

    // 窄屏（360px）第一行放不下三个带文案的按钮，目标入口改落第二行。
    testWidgets('窄屏下入口仍在且不溢出', (tester) async {
      await pumpTodayBar(
        tester,
        active: [_goal('g1', '发 1.0')],
        size: const Size(360, 760),
      );
      expect(find.text('目标1'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('窄屏空态入口同样可见', (tester) async {
      await pumpTodayBar(tester, size: const Size(360, 760));
      expect(find.text('目标'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });
}

class _FakeRepo extends ConversationRepository {
  _FakeRepo() : super(RustBridgeRepository());

  // 基类实现会走真实 bridge，单测里 frb 未初始化会抛 StateError。
  @override
  Future<List<Message>> getMessages(String conversationId) async =>
      const <Message>[];
}
