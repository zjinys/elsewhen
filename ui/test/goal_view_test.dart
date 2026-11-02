import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
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
  String? why,
  String? currentFocus,
  String? blocker,
  String? doneCriteria,
}) => Goal.fromDto(_dto(
  id: id,
  content: content,
  phase: phase,
  why: why,
  currentFocus: currentFocus,
  blocker: blocker,
  doneCriteria: doneCriteria,
));

Widget _panel({List<Goal> active = const [], List<Goal> archived = const []}) {
  return ProviderScope(
    overrides: [
      activeGoalsProvider.overrideWith((ref) async => active),
      archivedGoalsProvider.overrideWith((ref) async => archived),
      pausedGoalsProvider.overrideWith((ref) async =>
          archived.where((g) => g.status != GoalStatus.completed).toList()),
      completedGoalsProvider.overrideWith((ref) async =>
          archived.where((g) => g.status == GoalStatus.completed).toList()),
    ],
    child: const MaterialApp(home: Scaffold(body: GoalListView())),
  );
}

void main() {
  group('GoalListView', () {
    testWidgets('空态给出可操作提示，不是死路', (tester) async {
      await tester.pumpWidget(_panel());
      await tester.pumpAndSettle();

      expect(find.text('还没设定目标'), findsOneWidget);
      // 空态下新增入口必须可用
      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNotNull);
    });

    testWidgets('活跃 3 条时预先禁用新增并说明原因', (tester) async {
      await tester.pumpWidget(
        _panel(
          active: [
            _goal('g1', '发 1.0'),
            _goal('g2', '读完一本书', phase: 'mid'),
            _goal('g3', '财务自由', phase: 'long'),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('已达 3 条上限，请先归档一条'), findsOneWidget);
      // 第一道防线：UI 先挡住，但这不是唯一约束，后端触发器才是权威
      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNull);
    });

    testWidgets('活跃不足 3 条时不禁用新增', (tester) async {
      await tester.pumpWidget(_panel(active: [_goal('g1', '发 1.0')]));
      await tester.pumpAndSettle();

      final add = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '添加'),
      );
      expect(add.onPressed, isNotNull);
    });

    testWidgets('活跃目标按阶段展示中文标签', (tester) async {
      await tester.pumpWidget(
        _panel(
          active: [
            _goal('g1', '近期的事', phase: 'near'),
            _goal('g2', '中期的事', phase: 'mid'),
            _goal('g3', '长远的事', phase: 'long'),
          ],
        ),
      );
      await tester.pumpAndSettle();

      // 阶段标签在目标卡片和阶段选择器里都会出现，这里只验证都有渲染
      expect(find.text('近期'), findsWidgets);
      expect(find.text('中期'), findsWidgets);
      expect(find.text('长远'), findsWidgets);
      expect(find.text('近期的事'), findsOneWidget);
      expect(find.text('中期的事'), findsOneWidget);
      expect(find.text('长远的事'), findsOneWidget);
    });

    testWidgets('历史归档可复活', (tester) async {
      await tester.pumpWidget(
        _panel(
          active: [_goal('g1', '现在的事')],
          archived: [
            Goal.fromDto(
              _dto(
                id: 'g9',
                content: '放弃过的事',
                status: 'superseded',
                supersededAt: '2026-09-20T08:00:00Z',
              ),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('历史归档'), findsOneWidget);
      expect(find.text('放弃过的事'), findsOneWidget);
      expect(find.byTooltip('复活（需有名额）'), findsOneWidget);
    });

    testWidgets('历史为空时不占版面', (tester) async {
      await tester.pumpWidget(_panel(active: [_goal('g1', '现在的事')]));
      await tester.pumpAndSettle();
      expect(find.text('历史归档'), findsNothing);
    });

    testWidgets('活跃目标高亮呈现抓手、卡点与动力判据切片', (tester) async {
      await tester.pumpWidget(
        _panel(
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

      expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.textContaining('攻克多端同步快照收敛'), findsOneWidget);
      expect(find.textContaining('网络环境证书配置繁琐'), findsOneWidget);
      expect(find.textContaining('验证个人认知系统闭环'), findsOneWidget);
      expect(find.textContaining('双端自动化测试全绿'), findsOneWidget);
    });

    testWidgets('点击滚动抓手弹出编辑抓手与卡点弹窗', (tester) async {
      await tester.pumpWidget(
        _panel(
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
    });

    testWidgets('历史归档区分已达成与已暂停，读档回显动力回顾', (tester) async {
      final pausedGoal = Goal.fromDto(
        _dto(
          id: 'g-paused',
          content: '学法语',
          status: 'paused',
          why: '想去巴黎旅行交流',
          currentFocus: '背完 500 个基础词汇',
          blocker: '缺少日常发音纠正环境',
          supersededAt: '2026-10-01T12:00:00Z',
        ),
      );
      final completedGoal = Goal.fromDto(
        _dto(
          id: 'g-done',
          content: '考过驾照',
          status: 'completed',
          why: '方便自驾出游',
          doneCriteria: '科目四通过并拿本',
          supersededAt: '2026-09-15T12:00:00Z',
        ),
      );

      await tester.pumpWidget(
        _panel(
          active: [_goal('g1', '主线任务')],
          archived: [pausedGoal, completedGoal],
        ),
      );
      await tester.pumpAndSettle();

      // 默认在「暂停搁置」Tab
      expect(find.text('学法语'), findsOneWidget);
      expect(find.textContaining('背完 500 个基础词汇'), findsOneWidget);
      expect(find.text('考过驾照'), findsNothing);

      // 切换到「已达成」Tab
      await tester.tap(find.textContaining('已达成 (1)'));
      await tester.pumpAndSettle();

      expect(find.text('考过驾照'), findsOneWidget);
      expect(find.textContaining('科目四通过并拿本'), findsOneWidget);
      expect(find.text('学法语'), findsNothing);

      // 切回「暂停搁置」Tab 并点击「重新激活」
      await tester.tap(find.textContaining('暂停搁置 (1)'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('重新激活'));
      await tester.pumpAndSettle();

      // 验证唤醒弹窗回显历史动力与抓手
      expect(find.text('读档唤醒 · 学法语'), findsOneWidget);
      expect(find.textContaining('💡 核心动力回顾：想去巴黎旅行交流'), findsOneWidget);
      expect(find.widgetWithText(TextField, '背完 500 个基础词汇'), findsOneWidget);
      expect(find.widgetWithText(TextField, '缺少日常发音纠正环境'), findsOneWidget);
      expect(find.text('确认唤醒'), findsOneWidget);
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
