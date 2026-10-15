import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/providers/review_provider.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/review_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 回顾页三粒度（R1）widget 测试。禁 pumpAndSettle（门禁约束）——
/// FutureProvider 一次性 resolves 后逐帧 pump。
void main() {
  final overview = api.DailyOverviewDto(
    date: '2026-10-14',
    entries: [
      api.DailyEntryDto(
        eventId: 'e1',
        inputId: null,
        messageId: null,
        rawText: '和 Zoe 敲定了发布时间',
        source: 'capture',
        eventStatus: 'ready',
        recordedAt: '2026-10-14T14:30:00+08:00',
      ),
    ],
    review: api.DailyReviewDto(
      id: 'r1',
      date: '2026-10-14',
      promptVersion: 'daily-review-v1',
      createdAt: '2026-10-14T20:00:00+08:00',
      accomplishments: [
        api.DailyReviewItemDto(text: '发布就绪检查通过', sourceEventIds: ['e1']),
      ],
      ideasDecisions: [],
      peopleProjects: [],
      followUps: [],
    ),
    todos: [
      api.TodoDto(
        id: 't1',
        title: '跑全量测试',
        status: 'open',
        priority: 'normal',
        dueAt: null,
        relatedEventId: null,
        relatedWikiSlug: null,
        note: null,
        createdAt: '2026-10-14T10:00:00+08:00',
        updatedAt: '2026-10-14T10:00:00+08:00',
      ),
    ],
    reminderHits: [
      api.RuleHitDto(
        id: 'h1',
        ruleId: 'rule-1',
        ruleContent: '发布前先跑测试',
        status: 'aggregated',
        createdAt: '2026-10-14T10:00:00+08:00',
        feedback: 'helpful',
      ),
    ],
    memoryEcho: [
      api.MemoryEchoDto(
        eventId: 'e-old',
        recordedAt: '2026-10-07T09:00:00+08:00',
        rawText: '上周说了想写周报工具',
      ),
    ],
  );

  final stats = api.PeriodStatsDto(
    periodType: 'week',
    startDate: '2026-10-12',
    endDate: '2026-10-18',
    eventCount: 3,
    todoDoneCount: 2,
    byEventType: [
      api.NamedCountDto(label: 'work', count: 2),
      api.NamedCountDto(label: 'idea', count: 1),
    ],
    byPerson: [api.NamedCountDto(label: 'Zoe', count: 3)],
    byProject: [],
    byActivity: [api.NamedCountDto(label: 'coding', count: 3)],
    dailyCounts: [
      api.DayCountDto(date: '2026-10-12', count: 1),
      api.DayCountDto(date: '2026-10-13', count: 0),
      api.DayCountDto(date: '2026-10-14', count: 2),
      api.DayCountDto(date: '2026-10-15', count: 0),
      api.DayCountDto(date: '2026-10-16', count: 0),
      api.DayCountDto(date: '2026-10-17', count: 0),
      api.DayCountDto(date: '2026-10-18', count: 0),
    ],
  );

  late _ReviewBridge bridge;

  setUp(() {
    bridge = _ReviewBridge(overview: overview, stats: stats);
  });

  ProviderContainer makeContainer() {
    final container = ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(bridge)],
    );
    addTearDown(container.dispose);
    return container;
  }

  Widget host(ProviderContainer container) => UncontrolledProviderScope(
    container: container,
    child: MaterialApp(
      theme: ThemeData.dark(useMaterial3: true),
      // 与 main_screen 一致：内容区的裸 ExpansionTile 需要最近的
      // Material 祖先承载背景与墨水波纹。
      home: Scaffold(
        body: Material(color: AppTheme.surface0, child: const ReviewView()),
      ),
    ),
  );

  testWidgets('日视图呈现五块：AI 回顾/事件/待办/提醒/记忆回引', (tester) async {
    final container = makeContainer();
    container.read(reviewAnchorDateProvider.notifier).set('2026-10-14');
    await tester.pumpWidget(host(container));
    await tester.pump();

    expect(find.text('AI 回顾'), findsOneWidget);
    expect(find.text('发布就绪检查通过'), findsOneWidget);
    expect(bridge.lastOverviewDate, '2026-10-14', reason: '读面按锚日拉取');
    expect(find.text('当日事件'), findsOneWidget);
    expect(find.text('和 Zoe 敲定了发布时间'), findsOneWidget);
    expect(find.text('待办'), findsOneWidget);
    expect(find.text('跑全量测试'), findsOneWidget);
    expect(find.text('规则提醒'), findsOneWidget);
    expect(find.text('发布前先跑测试'), findsOneWidget);
    // D1 已有反馈只呈现判定术语，不渲染内部枚举值。
    expect(find.text('有用'), findsOneWidget);
    expect(find.text('helpful'), findsNothing);
    expect(find.text('上周的今天'), findsOneWidget);
    expect(find.text('上周说了想写周报工具'), findsOneWidget);
  });

  testWidgets('日视图锚点翻页换日期拉取', (tester) async {
    final container = makeContainer();
    container.read(reviewAnchorDateProvider.notifier).set('2026-10-14');
    await tester.pumpWidget(host(container));
    await tester.pump();
    expect(bridge.lastOverviewDate, '2026-10-14');
    await tester.tap(find.byIcon(Icons.chevron_right));
    await tester.pump();
    expect(bridge.lastOverviewDate, '2026-10-15');
    await tester.tap(find.byIcon(Icons.chevron_left));
    await tester.pump();
    await tester.tap(find.byIcon(Icons.chevron_left));
    await tester.pump();
    expect(bridge.lastOverviewDate, '2026-10-13');
  });

  testWidgets('摘要行点击展开事件原文（明细区不另开工作区）', (tester) async {
    final container = makeContainer();
    container.read(reviewAnchorDateProvider.notifier).set('2026-10-14');
    await tester.pumpWidget(host(container));
    await tester.pump();
    await tester.tap(find.text('和 Zoe 敲定了发布时间'));
    await tester.pump();
    expect(find.text('和 Zoe 敲定了发布时间'), findsNWidgets(2));
  });

  testWidgets('周视图呈现统计拼装：计数/日期区间/聚合榜', (tester) async {
    final container = makeContainer();
    container
      ..read(reviewAnchorDateProvider.notifier).set('2026-10-14')
      ..read(reviewGranularityProvider.notifier).set(ReviewGranularity.week);
    await tester.pumpWidget(host(container));
    await tester.pump();

    expect(bridge.lastStatsQuery, ('week', '2026-10-14'));
    expect(find.text('2026-10-12 ~ 2026-10-18'), findsOneWidget);
    // 数字渲染正确性归 Rust 聚合测试；Dart 只验拼版（数字在榜内多处
    // 出现，不按文本唯一性断言）。
    expect(find.text('事件'), findsOneWidget);
    expect(find.text('待办完成'), findsOneWidget);
    expect(find.text('事类'), findsOneWidget);
    expect(find.text('work'), findsOneWidget);
    expect(find.text('人物'), findsOneWidget);
    expect(find.text('Zoe'), findsOneWidget);
    // 空聚合组不渲染（零信息区块删除纪律）。
    expect(find.text('项目'), findsNothing);
  });

  testWidgets('月视图复用同一统计面板（periodType 直传桥接）', (tester) async {
    final container = makeContainer();
    container
      ..read(reviewAnchorDateProvider.notifier).set('2026-10-14')
      ..read(reviewGranularityProvider.notifier).set(ReviewGranularity.month);
    await tester.pumpWidget(host(container));
    await tester.pump();
    expect(bridge.lastStatsQuery, ('month', '2026-10-14'));
    expect(find.text('2026-10-12 ~ 2026-10-18'), findsOneWidget);
  });

  testWidgets('窄屏同一读面（纵向流自适应，无窄屏专属结构）', (tester) async {
    tester.view.physicalSize = const Size(500, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    final container = makeContainer();
    container.read(reviewAnchorDateProvider.notifier).set('2026-10-14');
    await tester.pumpWidget(host(container));
    await tester.pump();
    expect(find.byType(ReviewView), findsOneWidget);
    expect(find.text('AI 回顾'), findsOneWidget);
  });
}

/// 桥接假体：只覆写回顾页读面，其余继承不触碰 FFI（基类构造无副作用）。
class _ReviewBridge extends RustBridgeRepository {
  _ReviewBridge({required this.overview, required this.stats});

  final api.DailyOverviewDto overview;
  final api.PeriodStatsDto stats;
  String? lastOverviewDate;
  (String, String)? lastStatsQuery;

  @override
  Future<api.DailyOverviewDto> getDailyOverview(String date) {
    lastOverviewDate = date;
    return Future.value(overview);
  }

  @override
  Future<api.PeriodStatsDto> getPeriodStats(String periodType, String date) {
    lastStatsQuery = (periodType, date);
    return Future.value(stats);
  }
}
