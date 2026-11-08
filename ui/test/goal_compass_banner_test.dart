import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/providers/goal_provider.dart';
import 'package:elsewhen_ui/widgets/goal_compass_banner.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_goal_repo.dart';

GoalCompassNudge _sampleNudge({
  String id = 'nudge-1',
  String goalId = 'goal-1',
  String goalContent = '发布 Elsewhen 1.0',
  String currentFocus = '完成首版同步快照算法',
  int stalledDays = 4,
  String recentTopics = '配置反向代理, 排查桌面端UI偶发崩溃',
}) {
  return GoalCompassNudge(
    id: id,
    goalId: goalId,
    goalContent: goalContent,
    currentFocus: currentFocus,
    stalledDays: stalledDays,
    recentTopics: recentTopics,
    status: 'active',
    createdAt: DateTime.utc(2026, 11, 2, 10, 0),
  );
}

Goal _sampleGoal({
  String id = 'goal-1',
  String content = '发布 Elsewhen 1.0',
  String currentFocus = '完成首版同步快照算法',
  String blocker = '等待网络穿透端口确认',
}) {
  return Goal.fromDto(
    api.GoalDto(
      id: id,
      content: content,
      phase: 'near',
      status: 'active',
      currentFocus: currentFocus,
      blocker: blocker,
      createdAt: '2026-11-01T00:00:00Z',
      updatedAt: '2026-11-01T00:00:00Z',
    ),
  );
}

class FakeGoalCompassRepo extends FakeGoalChatRepo {
  FakeGoalCompassRepo({GoalCompassNudge? initialNudge}) {
    activeCompassNudge.value = initialNudge;
  }

  final List<String> dismissedNudgeIds = [];
  final List<String> realignedNudgeIds = [];
  final List<(String id, String? focus, String? blocker)> updatedFocuses = [];

  @override
  Future<void> dismissCompassNudge(String nudgeId) async {
    dismissedNudgeIds.add(nudgeId);
    activeCompassNudge.value = null;
  }

  @override
  Future<void> realignCompassNudge(String nudgeId) async {
    realignedNudgeIds.add(nudgeId);
    activeCompassNudge.value = null;
  }

  @override
  Future<void> updateGoalFocus({
    required String id,
    String? currentFocus,
    String? blocker,
  }) async {
    updatedFocuses.add((id, currentFocus, blocker));
  }
}

void main() {
  for (final locale in [const Locale('zh'), const Locale('en')]) {
    testWidgets(
      '${locale.languageCode}: narrow compass card keeps facts and goal discussion identity',
      (tester) async {
        tester.view.physicalSize = const Size(360, 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final nudge = _sampleNudge(stalledDays: 1);
        final repo = FakeGoalCompassRepo(initialNudge: nudge);
        final container = ProviderContainer(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [_sampleGoal()]),
          ],
        );
        addTearDown(container.dispose);
        final strings = lookupAppLocalizations(locale);
        await tester.pumpWidget(
          UncontrolledProviderScope(
            container: container,
            child: MaterialApp(
              locale: locale,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: SingleChildScrollView(
                  child: GoalCompassNudgeCard(nudge: nudge),
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(find.text(strings.compassStalled(1)), findsOneWidget);
        expect(find.textContaining(nudge.goalContent), findsOneWidget);
        expect(find.textContaining(nudge.currentFocus), findsOneWidget);
        expect(
          find.text(strings.compassObstacleLine('等待网络穿透端口确认')),
          findsOneWidget,
        );
        expect(tester.takeException(), isNull);
        await tester.tap(find.text(strings.compassFocusObstacle));
        await tester.pumpAndSettle();
        expect(
          container.read(goalPanelSeedProvider('goal-1')),
          strings.compassSeedObstacle(
            nudge.goalContent,
            '等待网络穿透端口确认',
            nudge.currentFocus,
          ),
        );
        expect(repo.updatedFocuses, isEmpty);
        expect(repo.realignedNudgeIds, isEmpty);
        expect(repo.dismissedNudgeIds, isEmpty);
      },
    );
  }

  group('GoalCompassNudgeBanner (消息区顶部横幅)', () {
    testWidgets('无活跃偏离提醒时不展示横幅', (tester) async {
      final repo = FakeGoalCompassRepo(initialNudge: null);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [_sampleGoal()]),
          ],
          child: const MaterialApp(
            home: Scaffold(body: GoalCompassNudgeBanner()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(GoalCompassNudgeBanner), findsOneWidget);
      expect(find.textContaining('航向校准'), findsNothing);
    });

    testWidgets('有偏离提醒时展示目标、行动、停滞天数、困难与分散精力话题', (tester) async {
      final nudge = _sampleNudge();
      final goal = _sampleGoal();
      final repo = FakeGoalCompassRepo(initialNudge: nudge);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [goal]),
          ],
          child: const MaterialApp(
            home: Scaffold(body: GoalCompassNudgeBanner()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('航向校准 · 目标「发布 Elsewhen 1.0」'), findsOneWidget);
      expect(find.textContaining('已停滞 4 天'), findsOneWidget);
      expect(find.textContaining('困难：等待网络穿透端口确认'), findsOneWidget);
      expect(
        find.textContaining('近期注意力多在「配置反向代理, 排查桌面端UI偶发崩溃」'),
        findsOneWidget,
      );
      expect(find.text('聚焦攻克此困难'), findsOneWidget);
      expect(find.text('更新当前行动'), findsOneWidget);
      expect(find.text('暂不打扰（退避7天）'), findsOneWidget);
    });

    testWidgets('点击「聚焦攻克此困难」把引导语预填进该目标的讨论草稿（留在目标页）', (tester) async {
      final nudge = _sampleNudge();
      final goal = _sampleGoal();
      final repo = FakeGoalCompassRepo(initialNudge: nudge);
      final container = ProviderContainer(
        overrides: [
          storageRepositoryProvider.overrideWithValue(repo),
          activeGoalsProvider.overrideWith((ref) async => [goal]),
        ],
      );

      String? capturedDraft;
      final sub = container.listen<String?>(goalPanelSeedProvider('goal-1'), (
        prev,
        next,
      ) {
        if (next != null) capturedDraft = next;
      });

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(body: GoalCompassNudgeBanner()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 此时草稿应为空
      expect(capturedDraft, isNull);

      // 点击聚焦攻克此困难
      await tester.tap(find.text('聚焦攻克此困难'));
      await tester.pumpAndSettle();

      // 验证草稿已被注入
      expect(capturedDraft, isNotNull);
      expect(capturedDraft, contains('关于目标「发布 Elsewhen 1.0」卡在「等待网络穿透端口确认」'));
      expect(capturedDraft, contains('当前行动是「完成首版同步快照算法」'));
      expect(capturedDraft, contains('我们聊聊怎么破局？'));

      sub.close();
    });

    testWidgets('点击「更新当前行动」弹窗更新行动与困难，成功后校准偏离横幅消除', (tester) async {
      final nudge = _sampleNudge();
      final goal = _sampleGoal();
      final repo = FakeGoalCompassRepo(initialNudge: nudge);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [goal]),
          ],
          child: const MaterialApp(
            home: Scaffold(body: GoalCompassNudgeBanner()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 点击更新行动按钮
      await tester.tap(find.text('更新当前行动'));
      await tester.pumpAndSettle();

      // 弹出 GoalRollFocusDialog 对话框
      expect(find.text('调整当前行动 · 发布 Elsewhen 1.0'), findsOneWidget);
      expect(find.widgetWithText(TextField, '完成首版同步快照算法'), findsOneWidget);

      // 键入新行动
      await tester.enterText(
        find.widgetWithText(TextField, '完成首版同步快照算法'),
        '新行动：调试移动端无线 ADB 传输',
      );
      await tester.tap(find.text('更新行动'));
      await tester.pumpAndSettle();

      // 验证 repo 记录与横幅已消除
      expect(repo.updatedFocuses.length, 1);
      expect(repo.updatedFocuses.first.$1, 'goal-1');
      expect(repo.updatedFocuses.first.$2, '新行动：调试移动端无线 ADB 传输');
      expect(repo.realignedNudgeIds, contains('nudge-1'));
      expect(find.textContaining('航向校准'), findsNothing);
    });

    testWidgets('点击「暂不打扰」触发 7 天退避且横幅消除', (tester) async {
      final nudge = _sampleNudge();
      final goal = _sampleGoal();
      final repo = FakeGoalCompassRepo(initialNudge: nudge);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [goal]),
          ],
          child: const MaterialApp(
            home: Scaffold(body: GoalCompassNudgeBanner()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('暂不打扰（退避7天）'));
      await tester.pumpAndSettle();

      expect(repo.dismissedNudgeIds, contains('nudge-1'));
      expect(find.textContaining('航向校准'), findsNothing);
    });
  });

  group('GoalCompassNudgeCard (目标工作台卡片)', () {
    testWidgets('工作台顶部呈现偏离提醒卡片并支持滚动更新行动与暂不打扰', (tester) async {
      final nudge = _sampleNudge();
      final goal = _sampleGoal();
      final repo = FakeGoalCompassRepo(initialNudge: nudge);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            storageRepositoryProvider.overrideWithValue(repo),
            activeGoalsProvider.overrideWith((ref) async => [goal]),
            archivedGoalsProvider.overrideWith((ref) async => const []),
            pausedGoalsProvider.overrideWith((ref) async => const []),
            completedGoalsProvider.overrideWith((ref) async => const []),
          ],
          child: const MaterialApp(home: Scaffold(body: GoalListView())),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(GoalCompassNudgeCard), findsOneWidget);
      expect(find.text('航向偏离提醒'), findsOneWidget);
      expect(find.text('停滞 4 天'), findsOneWidget);
      expect(find.textContaining('当前行动「完成首版同步快照算法」已连续 4 天未推进'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byType(GoalCompassNudgeCard),
          matching: find.text('当前困难：等待网络穿透端口确认'),
        ),
        findsOneWidget,
      );

      // 点击滚动更新行动
      await tester.tap(find.text('滚动更新行动'));
      await tester.pumpAndSettle();

      expect(find.text('调整当前行动 · 发布 Elsewhen 1.0'), findsOneWidget);
      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();

      // 点击右上角暂不打扰关闭按钮
      await tester.tap(find.byTooltip('暂不打扰（7天）'));
      await tester.pumpAndSettle();

      expect(repo.dismissedNudgeIds, contains('nudge-1'));
      expect(find.byType(GoalCompassNudgeCard), findsNothing);
    });
  });
}
