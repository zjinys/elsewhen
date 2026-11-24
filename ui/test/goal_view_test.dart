import 'support/localizations.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/models/plan.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/goal_chat_panel.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:material_ui/material_ui.dart';
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
  String? planId,
  int? stepIndex,
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
    planId: planId,
    stepIndex: stepIndex,
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
  String? planId,
  int? stepIndex,
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
    planId: planId,
    stepIndex: stepIndex,
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
  List<PlanBoardEntry> planBoard = const [],
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
    planBoardProvider.overrideWith((ref) async => planBoard),
  ];
}

/// 左栏工作台（GoalWorkbenchPanel）：独立挂载，不起整个左侧栏。
Widget _workbench({
  Locale locale = const Locale('zh'),
  List<Goal> active = const [],
  List<Goal> archived = const [],
  FakeGoalChatRepo? repository,
  List<PlanBoardEntry> planBoard = const [],
}) {
  return ProviderScope(
    overrides: _goalOverrides(
      active: active,
      archived: archived,
      repository: repository,
      planBoard: planBoard,
    ),
    child: MaterialApp(
      locale: locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: appTestLocalizationDelegates,
      home: const Scaffold(body: GoalWorkbenchPanel()),
    ),
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
    testWidgets('英文窄目标栏三态全量展示，用户目标原文保留', (tester) async {
      await tester.binding.setSurfaceSize(const Size(320, 780));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        _workbench(
          locale: const Locale('en'),
          active: [_goal('active', '保留用户的目标原文', currentFocus: '用户自己的行动')],
          archived: [_goal('paused', '另一个暂停目标', status: 'paused')],
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Goals'), findsOneWidget);
      // 两态各一枚阶段徽章：活跃强调色 + 归档转灰，不逐字读也能区分
      expect(find.text('Near term'), findsNWidgets(2));
      expect(find.text('保留用户的目标原文'), findsOneWidget);
      // 无筛选：归档态不再需要切胶囊，直接与活跃同屏可见
      expect(find.text('另一个暂停目标'), findsOneWidget);
      expect(find.text('暂停搁置'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('计划树仅显示一次步骤，头部带计划状态与进度，支持收起与选中', (tester) async {
      final plan = GoalPlan(
        id: 'p-1',
        intentText: '3 年内月入 10 万',
        vision: '',
        assumptions: '[]',
        status: PlanStatus.active,
        createdAt: DateTime(2026),
        updatedAt: DateTime(2026),
      );
      final activeStep = Goal(
        id: 'g-plan-1',
        content: '第一步：验证付费意愿',
        phase: GoalPhase.near,
        status: GoalStatus.active,
        createdAt: DateTime(2026),
        updatedAt: DateTime(2026),
        planId: 'p-1',
        stepIndex: 1,
      );
      final nextStep = Goal(
        id: 'g-plan-2',
        content: '第二步：最小可售卖方案',
        phase: GoalPhase.near,
        status: GoalStatus.paused,
        createdAt: DateTime(2026),
        updatedAt: DateTime(2026),
        planId: 'p-1',
        stepIndex: 2,
      );
      await tester.pumpWidget(
        _workbench(
          active: [activeStep],
          archived: [nextStep],
          planBoard: [
            PlanBoardEntry(
              plan: plan,
              activeStep: activeStep,
              nextStep: nextStep,
              doneCount: 0,
              totalCount: 3,
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('长程计划'), findsNothing);
      expect(find.text('3 年内月入 10 万'), findsOneWidget);
      // 计划头状态徽标：有活跃步 → 进行中 + 进度
      expect(find.text('进行中 0/3'), findsOneWidget);
      expect(find.text('第一步：验证付费意愿'), findsOneWidget);
      // 无筛选：暂停步与活跃步同屏
      expect(find.text('第二步：最小可售卖方案'), findsOneWidget);
      expect(find.text('进度 0/3'), findsNothing);

      await tester.tap(find.text('3 年内月入 10 万'));
      await tester.pumpAndSettle();
      final container = ProviderScope.containerOf(
        tester.element(find.byType(GoalWorkbenchPanel)),
      );
      expect(container.read(selectedGoalIdProvider), activeStep.id);
      await tester.tap(find.byTooltip('收起阶段'));
      await tester.pumpAndSettle();
      expect(find.text('第一步：验证付费意愿'), findsNothing);
      await tester.tap(find.byTooltip('展开阶段'));
      await tester.pumpAndSettle();
      expect(find.text('第一步：验证付费意愿'), findsOneWidget);
      expect(find.text('第二步：最小可售卖方案'), findsOneWidget);
      await tester.tap(find.text('第二步：最小可售卖方案'));
      await tester.pumpAndSettle();
      expect(container.read(selectedGoalIdProvider), nextStep.id);
    });

    testWidgets('标题「目标」+ 右上「新建」按钮；三态全量展示', (tester) async {
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

      // 无筛选：归档态直接与活跃同屏（分节徽标标状态）
      expect(find.text('学 Rust'), findsOneWidget);
      expect(find.text('搬家'), findsOneWidget);
      // 归档节徽标 + 步骤状态标签均在
      expect(find.text('暂停'), findsWidgets);
      expect(find.text('已达成'), findsWidgets);
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
    testWidgets('自动选中首个活跃目标：详情高亮阶段 + 讨论面板并排', (tester) async {
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
      // 行动条 + 左侧清单行动行（宽轨主区左缘也渲染一份清单）
      expect(find.textContaining('攻克多端同步快照收敛'), findsNWidgets(2));
      // 困难只在详情卡展示（清单不渲染 blocker 行）
      expect(find.textContaining('网络环境证书配置繁琐'), findsOneWidget);
      expect(find.textContaining('验证个人认知系统闭环'), findsOneWidget);
      expect(find.textContaining('双端自动化测试全绿'), findsOneWidget);
      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(find.textContaining('在左侧'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('空会话完整展示同计划前序讨论原文，尾部带出处分割线', (tester) async {
      useWideLayout(tester);
      final repo = _PredecessorGoalRepo();
      await tester.pumpWidget(
        _mainView(
          active: [
            _goal('g1', 'D1~D2 锁定单一标的物', planId: 'p-1', stepIndex: 1),
            _goal(
              'g2',
              'D3~D5 上线并完成首次精准触达',
              planId: 'p-1',
              stepIndex: 2,
            ),
          ],
          repository: repo,
        ),
      );
      await tester.pumpAndSettle();

      // 默认选中首个活跃目标 g1：无前序（fake 对 g1 返回 null）→ 无承接区。
      expect(find.textContaining('承接自'), findsNothing);

      // 切到第二步 g2：其会话为空且存在有讨论的前序 → 完整读原文展示。
      await tester.tap(find.text('D3~D5 上线并完成首次精准触达').last);
      await tester.pumpAndSettle();
      expect(
        find.text('以上承接自《D1~D2 锁定单一标的物》的 20 条讨论'),
        findsOneWidget,
      );
      // 完整原文（不截断）：超过 240 字符的长消息全文可见（幕僚气泡走
      // MarkdownView → RichText，需 findRichText）；限量 5 条在 fake 侧
      // 模拟（更早的 pm-0 不出现）。
      expect(
        find.textContaining('长消息结尾标记', findRichText: true),
        findsOneWidget,
      );
      expect(
        find.textContaining('先跑通第一批联系人', findRichText: true),
        findsWidgets,
      );
      expect(find.textContaining('这条更早，不该出现'), findsNothing);
      // 选中仍停留在当前目标，不跳转。
      final container = ProviderScope.containerOf(
        tester.element(find.byType(GoalChatPanel)),
      );
      expect(container.read(selectedGoalIdProvider), 'g2');
      expect(tester.takeException(), isNull);
    });

    testWidgets('点击行动条弹出调整当前行动对话框', (tester) async {
      useWideLayout(tester);
      await tester.pumpWidget(
        _mainView(
          active: [
            _goal(
              'g1',
              '发布 Elsewhen 1.0',
              currentFocus: '首发阶段 A',
              blocker: '困难 B',
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();

      // 点击行动区域弹出调整当前行动对话框（左侧清单也有一份行动文本，取详情卡的）
      await tester.tap(find.textContaining('首发阶段 A').last);
      await tester.pumpAndSettle();

      expect(find.text('调整当前行动 · 发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.widgetWithText(TextField, '首发阶段 A'), findsOneWidget);
      expect(find.widgetWithText(TextField, '困难 B'), findsOneWidget);
      expect(find.text('更新行动'), findsOneWidget);
      // 长阶段文本（AI 共同立项可能写入长句）：字段多行自适应显示
      final focusField = tester.widget<TextField>(
        find.widgetWithText(TextField, '首发阶段 A'),
      );
      expect(focusField.minLines, 2);
      expect(focusField.maxLines, 8);
    });

    testWidgets('没有活跃目标：主区默认打开立项工作室；窄轨宽度下清单退回主区左侧', (tester) async {
      // 默认视口 800 宽：桌面平台命中 700~1024 窄轨形态
      await tester.pumpWidget(_mainView());
      await tester.pumpAndSettle();

      // 立项与拆解留在目标页：未选中目标时主区是「AI 立项工作室」面板，
      // 而不是把用户推去对话页的占位指引。
      expect(find.text('AI 立项工作室'), findsWidgets);
      expect(find.byType(GoalChatPanel), findsOneWidget);
      expect(find.text('目标'), findsOneWidget); // 窄轨退回的清单标题
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

      // 活跃目标阶段完整呈现
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

/// 前序承接测试用 fake：g2 的空会话返回同计划前序 g1（20 条讨论），
/// 且 g1 的会话返回含超长消息的完整原文（验证 UI 不截断），
/// 其余目标无前序。
class _PredecessorGoalRepo extends FakeGoalChatRepo {
  @override
  Future<GoalChatPredecessor?> goalChatPredecessor(String goalId) async {
    if (goalId == 'g2') {
      return const GoalChatPredecessor(
        goalId: 'g1',
        goalContent: 'D1~D2 锁定单一标的物',
        messageCount: 20,
        conversationId: 'goal-conv-g1',
      );
    }
    return null;
  }

  @override
  Future<List<Message>> listRecentChatMessages(
    String conversationId,
    int limit,
  ) async {
    if (conversationId != 'goal-conv-g1') return const [];
    final long = '长' * 300;
    final all = [
      Message(
        id: 'pm-0',
        conversationId: conversationId,
        role: MessageRole.user,
        content: '这条更早，不该出现在最后 5 条里',
        createdAt: DateTime(2026),
      ),
      // 中间补足到 6 条，让 limit=5 生效切掉 pm-0。
      ...List.generate(
        4,
        (i) => Message(
          id: 'pm-m$i',
          conversationId: conversationId,
          role: i.isEven ? MessageRole.user : MessageRole.assistant,
          content: '中间消息 $i',
          createdAt: DateTime(2026),
        ),
      ),
      Message(
        id: 'pm-1',
        conversationId: conversationId,
        role: MessageRole.user,
        content: '获客先从已有联系人里筛 20 个',
        createdAt: DateTime(2026),
      ),
      Message(
        id: 'pm-2',
        conversationId: conversationId,
        role: MessageRole.assistant,
        content: '同意，先跑通第一批联系人再谈内容投放。$long长消息结尾标记',
        createdAt: DateTime(2026),
      ),
    ];
    return all.length > limit
        ? all.sublist(all.length - limit)
        : all;
  }
}
