import 'app_error_notice.dart';

import 'dart:io' show Platform;

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/goal.dart';
import '../providers/goal_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'adaptive_panel.dart';
import 'goal_chat_panel.dart';
import 'goal_compass_banner.dart';
import '../l10n/localization.dart';

// 目标页 AI 面板的预填草稿机制见 providers/goal_provider.dart
// （goalPanelSeedProvider）：立项与拆解全程留在本页，不跳对话 tab。

/// 活跃目标上限，与后端 `MAX_ACTIVE_GOALS` 及数据库触发器一致。
const int maxActiveGoals = 3;

/// 目标域「AI 共同立项 / 共同分解」统一入口：把指令预填到**目标页** AI 面板
/// 的输入框并确保停在「目标」tab，**绝不自动发送**——用户可改可发
/// （与习惯问候预填同一纪律）。立项与拆解全程留在目标页：不跳对话 tab、
/// 不混进普通对话清单与存储（工作室走保留会话 `goal-studio`）。
///
/// [goalId] 为空 = 立项工作室（新意图拆解）；非空 = 该目标的专属讨论面板。
/// 实际落库仍由 AI 走 create_plan / update_goal_facets 的 WriteConfirm
/// 提议卡完成，行动与计划永远由用户确认才落库。
void startGoalCoAuthor(WidgetRef ref, String seed, {String? goalId}) {
  final scope = goalId ?? goalStudioId;
  ref.read(sidebarTabProvider.notifier).set(SidebarTab.goal);
  ref.read(selectedGoalIdProvider.notifier).set(scope);
  ref.read(goalPanelSeedProvider(scope).notifier).set(seed);
}

/// 目标域「AI 共同立项 / 共同分解」的 Toast 文案。
void showGoalCoAuthorToast(BuildContext context) {
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text(context.l10n.goalCoAuthorReady),
      behavior: SnackBarBehavior.floating,
      duration: Duration(seconds: 3),
    ),
  );
}

/// 目标写操作后的统一刷新（活跃/暂停/已达成三态清单 + 航向校准提醒）。
/// 左栏工作台、主区详情与「新建目标」底部表单共用。
void refreshGoalProviders(WidgetRef ref) {
  ref.invalidate(activeGoalsProvider);
  ref.invalidate(archivedGoalsProvider);
  ref.invalidate(pausedGoalsProvider);
  ref.invalidate(completedGoalsProvider);
  ref.invalidate(activeCompassNudgeProvider);
}

/// 目标页主区（FR-PES-005-01 / Roadmap Phase G3）：选中目标的详情与
/// AI 讨论。
///
/// 目标 tab 的布局（[quickGlance] = false）：
/// - 主区是「详情卡 | AI 讨论面板」左右并排（宽度不足时上下堆叠回退）；
/// - master 清单、状态过滤与「新建目标」入口在左栏
///   [GoalWorkbenchPanel]，选中经 [selectedGoalIdProvider] 跨栏共享；
/// - 桌面窄轨形态（700~1024 窗口，左栏只剩图标导航）时清单退回主区
///   左侧渲染，保证该宽度下依然可选目标。
///
/// 对话界面「目标」弹窗（[quickGlance] = true）：只读速览——仅活跃
/// 目标阶段，零写操作、零 AI 入口、无历史归档。对话中瞄一眼目标
/// 现状用；所有操作引流到左 tab 工作台。
///
/// 单一写入口纪律：目标写操作只经对话流（WriteConfirm 提议卡）与
/// 左 tab 工作台（含「新建目标」底部表单），避免同一功能多入口语义漂移。
class GoalListView extends ConsumerStatefulWidget {
  const GoalListView({super.key, this.quickGlance = false});

  /// 只读速览模式：供对话界面目标弹窗使用，只呈现活跃目标阶段。
  final bool quickGlance;

  @override
  ConsumerState<GoalListView> createState() => _GoalListViewState();
}

class _GoalListViewState extends ConsumerState<GoalListView> {
  /// 本页生命周期内是否已自动选中过首个活跃目标（只做一次：进入目标页
  /// 主区直接落到第一条活跃目标，之后跟随左栏清单的选择变化）。
  bool _chatAutoSelected = false;

  /// 左栏在此宽度下是否退化为窄轨（只有图标导航、无内容区）。
  /// 与 [LeftSidebar] 的 rail 判定保持一致：桌面平台 700~1024 窗口。
  /// 窄轨时目标清单退回主区左侧渲染，避免无处可选目标。
  static bool _sidebarRailMode(BuildContext context) {
    if (kIsWeb) return false;
    final desktop = Platform.isLinux || Platform.isMacOS || Platform.isWindows;
    final width = MediaQuery.maybeSizeOf(context)?.width ?? 1024;
    return desktop && width >= 700 && width < 1024;
  }

  void _toast(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), behavior: SnackBarBehavior.floating),
    );
  }

  void _refreshAll() => refreshGoalProviders(ref);

  /// 按 ID 从三个状态清单的已加载值里找目标：选中项不在当前过滤视图时
  /// 也仍能找到（切过滤不闪空，详情与讨论跟随选中）。
  Goal? _goalById(String id) {
    for (final goals in [
      ref.read(activeGoalsProvider).value,
      ref.read(pausedGoalsProvider).value,
      ref.read(completedGoalsProvider).value,
    ]) {
      if (goals == null) continue;
      for (final g in goals) {
        if (g.id == id) return g;
      }
    }
    return null;
  }

  Future<void> _pause(Goal goal) async {
    if (widget.quickGlance) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.pauseGoal(goal.id);
      if (!mounted) return;
      _refreshAll();
      _toast(context.l10n.goalPaused(goal.content));
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _complete(Goal goal) async {
    if (widget.quickGlance) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.completeGoal(goal.id);
      if (!mounted) return;
      _refreshAll();
      _toast(context.l10n.goalCompletedToast(goal.content));
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _rollFocus(Goal goal) async {
    if (widget.quickGlance) return;
    final result = await showDialog<({String? focus, String? blocker})>(
      context: context,
      builder: (_) => GoalRollFocusDialog.forGoal(goal),
    );
    if (result == null || !mounted) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateGoalFocus(
        id: goal.id,
        currentFocus: result.focus,
        blocker: result.blocker,
      );
      if (!mounted) return;
      _refreshAll();
      _toast(context.l10n.goalActionSaved);
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _editFull(Goal goal) async {
    if (widget.quickGlance) return;
    final result =
        await showDialog<
          ({
            String content,
            GoalPhase phase,
            String? why,
            String? focus,
            String? blocker,
            String? done,
          })
        >(
          context: context,
          builder: (_) => _GoalEditFullDialog(goal: goal),
        );
    if (result == null) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateGoalFull(
        id: goal.id,
        content: result.content,
        phase: result.phase,
        why: result.why,
        currentFocus: result.focus,
        blocker: result.blocker,
        doneCriteria: result.done,
      );
      if (!mounted) return;
      _refreshAll();
      _toast(context.l10n.goalDetailsSaved);
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _reactivate(Goal goal, int activeCount) async {
    if (widget.quickGlance) return;
    if (activeCount >= maxActiveGoals) {
      _toast(context.l10n.goalCapacity(maxActiveGoals));
      return;
    }
    final result = await showDialog<({String? focus, String? blocker})>(
      context: context,
      builder: (_) => _GoalReactivateDialog(goal: goal),
    );
    if (result == null) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.reactivateGoal(
        goal.id,
        currentFocus: result.focus,
        blocker: result.blocker,
      );
      if (!mounted) return;
      _refreshAll();
      _toast(context.l10n.goalReactivated(goal.content));
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  @override
  Widget build(BuildContext context) {
    final activeAsync = ref.watch(activeGoalsProvider);
    // 订阅暂停/已达成清单：目标在活跃 ⇄ 暂停/已达成之间流转时，
    // 详情区与讨论面板随之刷新（选中项的查找横跨三个清单）。
    ref.watch(pausedGoalsProvider);
    ref.watch(completedGoalsProvider);
    final nudge = ref.watch(activeCompassNudgeProvider);
    final selectedId = ref.watch(selectedGoalIdProvider);
    final activeCount = activeAsync.value?.length ?? 0;

    // 速览模式：只看活跃目标阶段，零操作零 AI 入口
    if (widget.quickGlance) {
      return SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  Icons.track_changes_outlined,
                  size: 17,
                  color: AppTheme.accentPrimary,
                ),
                const SizedBox(width: 7),
                Text(
                  context.l10n.activeGoalCount(activeCount, maxActiveGoals),
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
                const Spacer(),
                Text(
                  context.l10n.goalManageInPage,
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
            const SizedBox(height: 12),
            _buildActiveSection(activeAsync),
          ],
        ),
      );
    }

    // 进入页面自动选中第一条活跃目标（只做一次），主区不再空着要用户
    // 先点一次。build 期不能改 provider，延迟到帧后写入。
    if (selectedId == null &&
        !_chatAutoSelected &&
        (activeAsync.value?.isNotEmpty ?? false)) {
      _chatAutoSelected = true;
      final firstId = activeAsync.value!.first.id;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        final notifier = ref.read(selectedGoalIdProvider.notifier);
        if (notifier.isMounted && ref.read(selectedGoalIdProvider) == null) {
          notifier.set(firstId);
        }
      });
    }

    return LayoutBuilder(
      builder: (context, constraints) {
        final detail = _buildDetailRegion(nudge, selectedId);
        if (!_sidebarRailMode(context)) return detail;
        // 窄轨侧栏（桌面 700~1024）：左栏只剩图标导航、没有内容区，
        // 工作台清单退回主区左侧渲染（旧中列形态），保证依然可选目标。
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(width: 320, child: GoalWorkbenchPanel()),
            Expanded(child: detail),
          ],
        );
      },
    );
  }

  /// 主区：选中目标的详情卡 + AI 讨论面板（左右并排，窄则上下堆叠）。
  /// 未选中（或选中立项工作室）时给「AI 立项工作室」——立项与拆解就在
  /// 目标页完成，不跳对话页。
  Widget _buildDetailRegion(dynamic nudge, String? selectedId) {
    final goal = selectedId == null || selectedId == goalStudioId
        ? null
        : _goalById(selectedId);
    if (goal == null) {
      return _buildStudioRegion();
    }
    final detailContent = Column(
      key: ValueKey('goal-detail-${goal.id}'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (nudge != null && nudge.goalId == goal.id) ...[
          GoalCompassNudgeCard(nudge: nudge),
          const SizedBox(height: 10),
        ],
        _ActiveGoalCard(
          goal: goal,
          onRollFocus: () => _rollFocus(goal),
          onEditFull: () => _editFull(goal),
          // 活跃目标才有暂停/达成操作；暂停/已达成目标改用重新激活。
          onPause: goal.status == GoalStatus.active
              ? () => _pause(goal)
              : () {},
          onComplete: goal.status == GoalStatus.active
              ? () => _complete(goal)
              : () {},
          onReactivate: goal.status == GoalStatus.active
              ? null
              : () => _reactivate(
                  goal,
                  ref.read(activeGoalsProvider).value?.length ?? 0,
                ),
        ),
      ],
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth < 720) {
          // 窄主区（手机 / 窄窗）：详情与讨论上下堆叠，整页滚动。
          return SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                detailContent,
                const SizedBox(height: 10),
                GoalChatPanel(
                  key: ValueKey('goal-chat-${goal.id}'),
                  goalId: goal.id,
                  goalContent: goal.content,
                ),
              ],
            ),
          );
        }
        // 宽主区：详情与 AI 讨论左右并排，讨论面板占满栏高。
        return Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                flex: 5,
                child: SingleChildScrollView(child: detailContent),
              ),
              const SizedBox(width: 12),
              Expanded(
                flex: 6,
                child: GoalChatPanel(
                  key: ValueKey('goal-chat-${goal.id}'),
                  goalId: goal.id,
                  goalContent: goal.content,
                  fillHeight: true,
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  /// 未选中目标的默认主区：AI 立项工作室。
  ///
  /// 用户把一句意图交给 AI，拆解与落库（确认制提案）全在目标页完成；
  /// 面板走保留会话 `goal-studio`，不混进普通对话清单与界面。
  Widget _buildStudioRegion() {
    final intro = Container(
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Icon(Icons.auto_awesome, size: 18, color: AppTheme.accentPrimary),
              const SizedBox(width: 6),
              Text(
                context.l10n.goalStudio,
                style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            context.l10n.goalStudioHelp,
            style: TextStyle(
              fontSize: 12,
              height: 1.6,
              color: AppTheme.textSecondary,
            ),
          ),
          const SizedBox(height: 8),
          Text(
            context.l10n.goalTreeHelp,
            style: TextStyle(
              fontSize: 11.5,
              height: 1.6,
              color: AppTheme.textTertiary,
            ),
          ),
        ],
      ),
    );
    const panelKey = ValueKey('goal-studio-panel');
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth < 720) {
          return SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                intro,
                const SizedBox(height: 10),
                const GoalChatPanel(key: panelKey, studio: true),
              ],
            ),
          );
        }
        return Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(flex: 5, child: SingleChildScrollView(child: intro)),
              const SizedBox(width: 12),
              const Expanded(
                flex: 6,
                child: GoalChatPanel(
                  key: panelKey,
                  studio: true,
                  fillHeight: true,
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  /// 速览模式的活跃目标阶段（只读）：无选中、无操作、无 AI 入口。
  Widget _buildActiveSection(AsyncValue<List<Goal>> activeAsync) {
    return activeAsync.when(
      loading: () => const Padding(
        padding: EdgeInsets.symmetric(vertical: 12),
        child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
      ),
      error: (e, _) => Text(
        context.l10n.goalLoadFailed(e.toString()),
        style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
      ),
      data: (goals) {
        if (goals.isEmpty) {
          return Padding(
            padding: const EdgeInsets.symmetric(vertical: 12),
            child: Text(
              context.l10n.noActiveGoals,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          );
        }
        return Column(
          children: [
            for (final goal in goals)
              _GoalListTile(goal: goal, selected: false, showBlocker: true),
          ],
        );
      },
    );
  }
}

/// 目标域左栏工作台（Roadmap G3 侧栏化）：目标 master 清单。
///
/// 原主区中列整体搬入左栏：标题「目标」+ 计数徽章 + 右上常驻「新建」
/// 按钮（始终可点，表单弹出底部面板，见 [showGoalCreateSheet]）；
/// 状态过滤多选（一行三枚，缺省活跃）；「和 AI 拆解 / 立新目标」入口
/// 与按状态并集渲染的目标清单。选中经 [selectedGoalIdProvider] 与
/// 主区详情/讨论共享；移动端抽屉形态下点选后自动收起抽屉。
class GoalWorkbenchPanel extends ConsumerWidget {
  const GoalWorkbenchPanel({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final activeAsync = ref.watch(activeGoalsProvider);
    final pausedAsync = ref.watch(pausedGoalsProvider);
    final completedAsync = ref.watch(completedGoalsProvider);
    final filter = ref.watch(goalStatusFilterProvider);
    final selectedId = ref.watch(selectedGoalIdProvider);
    final activeCount = activeAsync.value?.length ?? 0;
    final pausedCount = pausedAsync.value?.length ?? 0;
    final completedCount = completedAsync.value?.length ?? 0;

    void select(String id) {
      ref.read(selectedGoalIdProvider.notifier).set(id);
      // 移动端抽屉形态：选完目标即收抽屉，让主区的详情与讨论可见。
      final scaffold = Scaffold.maybeOf(context);
      if (scaffold?.isDrawerOpen ?? false) {
        Navigator.of(context).pop();
      }
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.track_changes_outlined,
                size: 19,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 8),
              Text(
                context.l10n.goals,
                style: TextStyle(
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(width: 6),
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 6,
                  vertical: 1.5,
                ),
                decoration: BoxDecoration(
                  color: AppTheme.surface3,
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  '$activeCount / $maxActiveGoals',
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: activeCount >= maxActiveGoals
                        ? AppTheme.accentPrimary
                        : AppTheme.textSecondary,
                  ),
                ),
              ),
              const Spacer(),
              // 新建入口：始终可点——活跃已满时后端会以「暂停搁置」状态创建。
              FilledButton.tonalIcon(
                onPressed: () => showGoalCreateSheet(context),
                icon: const Icon(Icons.add, size: 14),
                label: Text(context.l10n.newItem),
                style: FilledButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  minimumSize: const Size(0, 30),
                  textStyle: const TextStyle(fontSize: 11.5),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          // 状态过滤：多选（一行三枚），决定清单内容（缺省活跃）。
          Row(
            children: [
              for (final (statusFilter, count) in [
                (GoalStatusFilter.active, activeCount),
                (GoalStatusFilter.paused, pausedCount),
                (GoalStatusFilter.completed, completedCount),
              ]) ...[
                Expanded(
                  child: _StatusFilterChip(
                    label: switch (statusFilter) {
                      GoalStatusFilter.active => context.l10n.activeGoals,
                      GoalStatusFilter.paused => context.l10n.pausedGoals,
                      GoalStatusFilter.completed => context.l10n.achievedGoals,
                    },
                    count: count,
                    selected: filter.contains(statusFilter),
                    onTap: () => toggleGoalStatusFilter(ref, statusFilter),
                  ),
                ),
                if (statusFilter != GoalStatusFilter.completed)
                  const SizedBox(width: 6),
              ],
            ],
          ),
          const SizedBox(height: 6),
          // AI 共同立项入口：预填对话草稿并切回对话 tab，实际落库仍走
          // create_goal / update_goal_facets 的 WriteConfirm 提议卡。
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: () => _startGoalCoAuthor(ref, context, activeCount),
              icon: const Icon(Icons.auto_awesome, size: 13),
              label: Text(
                activeCount == 0
                    ? context.l10n.planWithAi
                    : context.l10n.planNewGoal,
                style: const TextStyle(fontSize: 11),
              ),
              style: TextButton.styleFrom(
                visualDensity: VisualDensity.compact,
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              ),
            ),
          ),
          const SizedBox(height: 4),
          _listSection(
            context: context,
            ref: ref,
            filters: filter,
            activeAsync: activeAsync,
            pausedAsync: pausedAsync,
            completedAsync: completedAsync,
            selectedId: selectedId,
            onSelect: select,
          ),
        ],
      ),
    );
  }

  /// 按状态多选渲染清单（并集，缺省活跃）。空态文案带下一步指引。
  Widget _listSection({
    required BuildContext context,
    required WidgetRef ref,
    required Set<GoalStatusFilter> filters,
    required AsyncValue<List<Goal>> activeAsync,
    required AsyncValue<List<Goal>> pausedAsync,
    required AsyncValue<List<Goal>> completedAsync,
    required String? selectedId,
    required void Function(String id) onSelect,
  }) {
    Widget loading() => const Padding(
      padding: EdgeInsets.symmetric(vertical: 12),
      child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
    );
    Widget failed(Object e) => Text(
      context.l10n.goalLoadFailed(e.toString()),
      style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
    );
    Widget empty(String message) => Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Text(
        message,
        style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
      ),
    );

    final sections = <(String?, List<Goal>)>[];
    for (final (statusFilter, asyncValue, badge) in [
      (GoalStatusFilter.active, activeAsync, null),
      (GoalStatusFilter.paused, pausedAsync, context.l10n.pausedShort),
      (GoalStatusFilter.completed, completedAsync, context.l10n.achievedGoals),
    ]) {
      if (!filters.contains(statusFilter)) continue;
      if (asyncValue.hasError) return failed(asyncValue.error!);
      final goals = asyncValue.value;
      if (goals == null) return loading();
      sections.add((badge, goals));
    }
    final total = sections.fold<int>(
      0,
      (sum, section) => sum + section.$2.length,
    );
    if (total == 0) {
      final activeOnly =
          filters.length == 1 && filters.contains(GoalStatusFilter.active);
      return empty(
        activeOnly
            ? context.l10n.noActiveGoalsCreate
            : context.l10n.noGoalsInStatus,
      );
    }
    final board = ref.watch(planBoardProvider);
    if (board.hasError) return failed(board.error!);
    final plans = board.value ?? const <PlanBoardEntry>[];
    final visible = sections.expand((section) => section.$2).toList();
    final grouped = <String>{};
    final nodes = <Widget>[];
    for (final entry in plans) {
      final steps = visible.where((g) => g.planId == entry.plan.id).toList()
        ..sort((a, b) => (a.stepIndex ?? 0).compareTo(b.stepIndex ?? 0));
      if (steps.isEmpty) continue;
      grouped.addAll(steps.map((g) => g.id));
      nodes.add(
        _PlanNavigationNode(
          key: ValueKey('plan-navigation-${entry.plan.id}'),
          entry: entry,
          steps: steps,
          selectedId: selectedId,
          onSelect: onSelect,
        ),
      );
    }
    for (final (badge, goals) in sections) {
      for (final goal in goals) {
        if (grouped.contains(goal.id)) continue;
        nodes.add(
          _GoalListTile(
            goal: goal,
            selected: selectedId == goal.id,
            statusBadge: badge,
            onTap: () => onSelect(goal.id),
          ),
        );
      }
    }
    return Column(children: nodes);
  }
}

/// 打开「新建目标」底部表单（AdaptivePanelSheet 统一壳）。
Future<void> showGoalCreateSheet(BuildContext context) {
  return showPanelSheet<void>(
    context,
    sheet: (_) => AdaptivePanelSheet(
      title: context.l10n.newGoal,
      builder: (_) => const _GoalCreateForm(),
    ),
  );
}

/// 新建目标底部表单内容：愿景必填 + 阶段 + 可选目标背景。
///
/// 原主区中列的常驻表单收进面板：成功落库后关闭面板并刷新目标三态
/// 清单。活跃已满 3 条时不再禁用表单——后端会把新目标以「暂停搁置」
/// 状态创建，表单与 Toast 同步说明该语义。
class _GoalCreateForm extends ConsumerStatefulWidget {
  const _GoalCreateForm();

  @override
  ConsumerState<_GoalCreateForm> createState() => _GoalCreateFormState();
}

class _GoalCreateFormState extends ConsumerState<_GoalCreateForm> {
  final _contentController = TextEditingController();
  final _whyController = TextEditingController();
  final _focusController = TextEditingController();
  final _blockerController = TextEditingController();
  final _doneController = TextEditingController();

  GoalPhase _newPhase = GoalPhase.near;
  bool _saving = false;
  bool _expandFacets = false;

  @override
  void dispose() {
    _contentController.dispose();
    _whyController.dispose();
    _focusController.dispose();
    _blockerController.dispose();
    _doneController.dispose();
    super.dispose();
  }

  Future<void> _add() async {
    final content = _contentController.text.trim();
    if (content.isEmpty || _saving) return;
    setState(() => _saving = true);
    final messenger = ScaffoldMessenger.of(context);
    final navigator = Navigator.of(context);
    final atCap =
        (ref.read(activeGoalsProvider).value?.length ?? 0) >= maxActiveGoals;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final why = _whyController.text.trim();
      final focus = _focusController.text.trim();
      final blocker = _blockerController.text.trim();
      final done = _doneController.text.trim();
      await repo.createGoal(
        content: content,
        phase: _newPhase,
        why: why.isEmpty ? null : why,
        currentFocus: focus.isEmpty ? null : focus,
        blocker: blocker.isEmpty ? null : blocker,
        doneCriteria: done.isEmpty ? null : done,
      );
      if (!mounted) return;
      final savedMessage = atCap
          ? context.l10n.newGoalPaused(content)
          : context.l10n.newGoalCreated(content);
      refreshGoalProviders(ref);
      navigator.pop();
      messenger.showSnackBar(
        SnackBar(
          content: Text(savedMessage),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _saving = false);
      showAppError(context, context.l10n.save, e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final activeCount = ref.watch(activeGoalsProvider).value?.length ?? 0;
    final atCap = activeCount >= maxActiveGoals;
    return Padding(
      // 移动端键盘弹起时整体上移（底部面板 isScrollControlled: true）。
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              key: const ValueKey('goal-create-content'),
              controller: _contentController,
              enabled: !_saving,
              autofocus: true,
              onSubmitted: (_) => _add(),
              decoration: InputDecoration(
                hintText: atCap
                    ? context.l10n.goalCapHint
                    : context.l10n.newGoalHint,
                hintStyle: TextStyle(
                  fontSize: 12.5,
                  color: AppTheme.textTertiary,
                ),
                prefixIcon: const Icon(Icons.add_task, size: 16),
                isDense: true,
              ),
            ),
            const SizedBox(height: 10),
            // 阶段选择与阶段展开：Wrap 布局，窄容器下自动换行不溢出
            Wrap(
              spacing: 6,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                for (final phase in GoalPhase.values)
                  ChoiceChip(
                    label: Text(_goalPhaseLabel(context, phase)),
                    selected: _newPhase == phase,
                    onSelected: (_) => setState(() => _newPhase = phase),
                    visualDensity: VisualDensity.compact,
                    labelStyle: const TextStyle(fontSize: 11.5),
                  ),
                TextButton.icon(
                  onPressed: () =>
                      setState(() => _expandFacets = !_expandFacets),
                  icon: Icon(
                    _expandFacets ? Icons.expand_less : Icons.expand_more,
                    size: 16,
                  ),
                  label: Text(
                    _expandFacets
                        ? context.l10n.collapseGoalContext
                        : context.l10n.addGoalContext,
                  ),
                  style: TextButton.styleFrom(
                    visualDensity: VisualDensity.compact,
                    textStyle: const TextStyle(fontSize: 11.5),
                  ),
                ),
              ],
            ),
            if (_expandFacets) ...[
              const SizedBox(height: 8),
              TextField(
                controller: _whyController,
                decoration: InputDecoration(
                  labelText: context.l10n.goalWhyOptional,
                  isDense: true,
                  hintText: context.l10n.goalWhyHint,
                ),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _focusController,
                decoration: InputDecoration(
                  labelText: context.l10n.goalActionOptional,
                  isDense: true,
                  hintText: context.l10n.goalActionHint,
                ),
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _blockerController,
                      decoration: InputDecoration(
                        labelText: context.l10n.goalObstacleOptional,
                        isDense: true,
                        hintText: context.l10n.goalObstacleHint,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: TextField(
                      controller: _doneController,
                      decoration: InputDecoration(
                        labelText: context.l10n.goalDoneOptional,
                        isDense: true,
                        hintText: context.l10n.goalDoneHint,
                      ),
                    ),
                  ),
                ],
              ),
            ],
            const SizedBox(height: 14),
            SizedBox(
              width: double.infinity,
              child: FilledButton.tonal(
                onPressed: _saving ? null : _add,
                child: _saving
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : Text(context.l10n.add),
              ),
            ),
            if (atCap)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(
                  context.l10n.newGoalCapacityHint(maxActiveGoals),
                  style: TextStyle(
                    fontSize: 11.5,
                    color: AppTheme.accentPrimary,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// 「和 AI 拆解 / 立新目标」：预填目标页立项工作室草稿（绝不自动发送），
/// 不跳对话页。实际落库走 create_plan / update_goal_facets 的 WriteConfirm
/// 提议卡，这里不做任何直接写库。
void _startGoalCoAuthor(WidgetRef ref, BuildContext context, int activeCount) {
  final goals = ref.read(activeGoalsProvider).value ?? const <Goal>[];
  final goalBrief = goals.isEmpty
      ? context.l10n.noGoalsBrief
      : context.l10n.goalBrief(
          goals
              .map(
                (g) => context.l10n.goalBriefItem(
                  g.content,
                  g.currentFocus ?? context.l10n.notSet,
                ),
              )
              .join(context.l10n.listSeparator),
        );
  final seed = activeCount == 0
      ? context.l10n.goalNewSeed(goalBrief)
      : context.l10n.goalReviewSeed(goalBrief);
  startGoalCoAuthor(ref, seed);
  showGoalCoAuthorToast(context);
}

/// 目标清单项（左栏工作台）：阶段徽章 + 标题 + 当前行动阶段，
/// 点击选中，主区详情与 AI 讨论跟随。
///
/// 紧凑导航形态（非管理卡片）：管理动作（调整当前行动/暂停/达成/唤醒）
/// 全部收敛到主区详情卡；这里只负责「看有哪些目标 + 选中一个」。
/// [statusBadge] 非空时展示状态后缀（暂停/已达成，活跃态不展示）。
class _GoalListTile extends StatelessWidget {
  const _GoalListTile({
    required this.goal,
    required this.selected,
    this.onTap,
    this.statusBadge,
    this.showBlocker = false,
  });

  final Goal goal;
  final bool selected;

  /// 点击选中该目标。速览模式传 null：只读呈现，无选中动作。
  final VoidCallback? onTap;
  final String? statusBadge;

  /// 速览模式（quickGlance）下额外展示当前困难：对话中瞄一眼时
  /// 「行动 + 困难」是最有用的两个阶段；侧栏清单保持紧凑不展示。
  final bool showBlocker;

  @override
  Widget build(BuildContext context) {
    final focus = goal.currentFocus?.trim() ?? '';
    final blocker = goal.blocker?.trim() ?? '';
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        child: Container(
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 7),
          decoration: BoxDecoration(
            color: selected ? AppTheme.surface2 : Colors.transparent,
            borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
            border: Border.all(
              color: selected
                  ? AppTheme.accentPrimary.withValues(alpha: 0.45)
                  : AppTheme.surface3.withValues(alpha: 0.6),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 5,
                      vertical: 1.5,
                    ),
                    decoration: BoxDecoration(
                      color: AppTheme.accentPrimary.withValues(alpha: 0.14),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      _goalPhaseLabel(context, goal.phase),
                      style: TextStyle(
                        fontSize: 10,
                        fontWeight: FontWeight.w600,
                        letterSpacing: 0.4,
                        color: AppTheme.accentPrimary,
                      ),
                    ),
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      goal.content,
                      softWrap: true,
                      style: TextStyle(
                        fontSize: 12.5,
                        fontWeight: FontWeight.w600,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                  ),
                  if (statusBadge != null) ...[
                    const SizedBox(width: 4),
                    Text(
                      statusBadge!,
                      style: TextStyle(
                        fontSize: 10,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                  ],
                ],
              ),
              if (focus.isNotEmpty) ...[
                const SizedBox(height: 3),
                Text(
                  '📍 $focus',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
                ),
              ],
              if (showBlocker && blocker.isNotEmpty) ...[
                const SizedBox(height: 3),
                Text(
                  '🚧 $blocker',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// 状态过滤胶囊（活跃/暂停搁置/已达成）：多选切换 + 计数徽标。
///
/// 一行三枚（外层 Row + Expanded 定宽），故不带图标、收紧内边距，
/// 保证「暂停搁置」这类长标签也单行放下不换行。
class _StatusFilterChip extends StatelessWidget {
  const _StatusFilterChip({
    required this.label,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final int count;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final fg = selected ? AppTheme.accentPrimary : AppTheme.textTertiary;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(999),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
        decoration: BoxDecoration(
          color: selected
              ? AppTheme.accentPrimary.withValues(alpha: 0.12)
              : Colors.transparent,
          borderRadius: BorderRadius.circular(999),
          border: Border.all(
            color: selected
                ? AppTheme.accentPrimary.withValues(alpha: 0.45)
                : AppTheme.surface3,
          ),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Flexible(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                  color: fg,
                ),
              ),
            ),
            const SizedBox(width: 4),
            Text(
              '$count',
              style: TextStyle(
                fontSize: 10.5,
                fontWeight: FontWeight.w600,
                color: selected ? fg : AppTheme.textTertiary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 目标详情卡（高亮呈现当前行动与当前困难）。AI 讨论面板
/// [GoalChatPanel] 由主区与卡片并列渲染，卡片本身不含对话入口。
/// P3 计划看板：推进中的计划各一卡（意图 / 当前步 / 进度 / 下一步预告）。
/// 选中联动：点击卡片把主区切到该计划的当前步（无当前步则下一步）。
class _PlanNavigationNode extends StatefulWidget {
  const _PlanNavigationNode({
    super.key,
    required this.entry,
    required this.steps,
    required this.selectedId,
    required this.onSelect,
  });
  final PlanBoardEntry entry;
  final List<Goal> steps;
  final String? selectedId;
  final void Function(String) onSelect;
  @override
  State<_PlanNavigationNode> createState() => _PlanNavigationNodeState();
}

class _PlanNavigationNodeState extends State<_PlanNavigationNode> {
  bool expanded = true;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            IconButton(
              tooltip: expanded
                  ? context.l10n.collapsePhase
                  : context.l10n.expandPhase,
              visualDensity: VisualDensity.compact,
              onPressed: () => setState(() => expanded = !expanded),
              icon: Icon(
                expanded ? Icons.expand_more : Icons.chevron_right,
                size: 18,
              ),
            ),
            Expanded(
              child: InkWell(
                onTap: () => widget.onSelect(
                  widget.steps.any((g) => g.id == widget.entry.focusGoalId)
                      ? widget.entry.focusGoalId!
                      : widget.steps.first.id,
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Text(
                    widget.entry.plan.intentText,
                    softWrap: true,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
        if (expanded)
          Padding(
            padding: const EdgeInsets.only(left: 20),
            child: Column(
              children: [
                for (final step in widget.steps)
                  _GoalListTile(
                    goal: step,
                    selected: widget.selectedId == step.id,
                    statusBadge: step.status == GoalStatus.active
                        ? context.l10n.currentPhase
                        : _goalStatusLabel(context, step.status),
                    onTap: () => widget.onSelect(step.id),
                  ),
              ],
            ),
          ),
      ],
    ),
  );
}

class _ActiveGoalCard extends StatefulWidget {
  const _ActiveGoalCard({
    required this.goal,
    required this.onRollFocus,
    required this.onEditFull,
    required this.onPause,
    required this.onComplete,
    this.onReactivate,
  });

  final Goal goal;
  final VoidCallback onRollFocus;
  final VoidCallback onEditFull;
  final VoidCallback onPause;
  final VoidCallback onComplete;

  /// 暂停/已达成目标在详情中的「重新激活」回调（活跃目标为 null
  /// 不展示）。弹读档唤醒对话框，需有活跃名额。
  final VoidCallback? onReactivate;

  @override
  State<_ActiveGoalCard> createState() => _ActiveGoalCardState();
}

class _ActiveGoalCardState extends State<_ActiveGoalCard> {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final goal = widget.goal;
    final focus = goal.currentFocus?.trim();
    final blocker = goal.blocker?.trim();
    final why = goal.why?.trim();
    final done = goal.doneCriteria?.trim();

    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // 头部：阶段徽章 + 愿景 + 操作菜单
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 10, 8, 4),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface3,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    _goalPhaseLabel(context, goal.phase),
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.accentPrimary,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    goal.content,
                    style: TextStyle(
                      fontSize: 13.5,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
                PopupMenuButton<String>(
                  tooltip: context.l10n.moreActions,
                  icon: const Icon(Icons.more_horiz, size: 16),
                  color: AppTheme.surface2,
                  onSelected: (val) {
                    switch (val) {
                      case 'roll':
                        widget.onRollFocus();
                        break;
                      case 'complete':
                        widget.onComplete();
                        break;
                      case 'pause':
                        widget.onPause();
                        break;
                      case 'edit':
                        widget.onEditFull();
                        break;
                      case 'reactivate':
                        widget.onReactivate?.call();
                        break;
                    }
                  },
                  itemBuilder: (_) => [
                    if (widget.onReactivate != null)
                      PopupMenuItem(
                        value: 'reactivate',
                        child: Row(
                          children: [
                            Icon(Icons.play_arrow_outlined, size: 16),
                            SizedBox(width: 8),
                            Text(context.l10n.reactivateGoal),
                          ],
                        ),
                      ),
                    PopupMenuItem(
                      value: 'roll',
                      child: Row(
                        children: [
                          Icon(Icons.tune_outlined, size: 16),
                          SizedBox(width: 8),
                          Text(context.l10n.adjustGoalAction),
                        ],
                      ),
                    ),
                    PopupMenuItem(
                      value: 'complete',
                      child: Row(
                        children: [
                          Icon(Icons.check_circle_outline, size: 16),
                          SizedBox(width: 8),
                          Text(context.l10n.completeGoal),
                        ],
                      ),
                    ),
                    PopupMenuItem(
                      value: 'pause',
                      child: Row(
                        children: [
                          Icon(Icons.pause_circle_outline, size: 16),
                          SizedBox(width: 8),
                          Text(context.l10n.pauseGoal),
                        ],
                      ),
                    ),
                    const PopupMenuDivider(),
                    PopupMenuItem(
                      value: 'edit',
                      child: Row(
                        children: [
                          Icon(Icons.edit_note, size: 16),
                          SizedBox(width: 8),
                          Text(context.l10n.editGoalDetails),
                        ],
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),

          // 阶段高亮区
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 4, 12, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 行动条
                InkWell(
                  onTap: widget.onRollFocus,
                  borderRadius: BorderRadius.circular(6),
                  child: Container(
                    width: double.infinity,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 10,
                      vertical: 7,
                    ),
                    decoration: BoxDecoration(
                      color: AppTheme.surface2,
                      borderRadius: BorderRadius.circular(6),
                      border: Border.all(
                        color: focus != null && focus.isNotEmpty
                            ? AppTheme.accentPrimary.withValues(alpha: 0.3)
                            : AppTheme.surface3,
                      ),
                    ),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text('📍 ', style: TextStyle(fontSize: 12)),
                        Expanded(
                          child: Text.rich(
                            TextSpan(
                              style: const TextStyle(fontSize: 12.5),
                              children: [
                                TextSpan(
                                  text: context.l10n.currentActionLabel,
                                  style: TextStyle(
                                    fontWeight: FontWeight.w600,
                                    color: AppTheme.textPrimary,
                                  ),
                                ),
                                TextSpan(
                                  text: (focus != null && focus.isNotEmpty)
                                      ? focus
                                      : context.l10n.goalActionNotSet,
                                  style: TextStyle(
                                    color: (focus != null && focus.isNotEmpty)
                                        ? AppTheme.textPrimary
                                        : AppTheme.textTertiary,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),

                // 困难条
                if (blocker != null && blocker.isNotEmpty) ...[
                  const SizedBox(height: 6),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 10,
                      vertical: 6,
                    ),
                    decoration: BoxDecoration(
                      color: AppTheme.surface2,
                      borderRadius: BorderRadius.circular(6),
                      border: Border.all(
                        color: theme.colorScheme.error.withValues(alpha: 0.3),
                      ),
                    ),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text('🚧 ', style: TextStyle(fontSize: 12)),
                        Expanded(
                          child: Text.rich(
                            TextSpan(
                              style: const TextStyle(fontSize: 12),
                              children: [
                                TextSpan(
                                  text: context.l10n.currentObstacleLabel,
                                  style: TextStyle(
                                    fontWeight: FontWeight.w600,
                                    color: theme.colorScheme.error,
                                  ),
                                ),
                                TextSpan(
                                  text: blocker,
                                  style: TextStyle(
                                    color: AppTheme.textSecondary,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],

                // 目标背景完整呈现
                if ((why != null && why.isNotEmpty) ||
                    (done != null && done.isNotEmpty)) ...[
                  const SizedBox(height: 6),
                  Wrap(
                    spacing: 12,
                    runSpacing: 4,
                    children: [
                      if (why != null && why.isNotEmpty)
                        Text(
                          context.l10n.whyGoal(why),
                          style: TextStyle(
                            fontSize: 13,
                            height: 1.6,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      if (done != null && done.isNotEmpty)
                        Text(
                          context.l10n.doneGoal(done),
                          style: TextStyle(
                            fontSize: 13,
                            height: 1.6,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                    ],
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// 调整当前行动与困难对话框（最常用的轻量认知更新）
class GoalRollFocusDialog extends StatefulWidget {
  const GoalRollFocusDialog({
    super.key,
    required this.goalId,
    required this.goalContent,
    this.initialFocus,
    this.initialBlocker,
  });

  factory GoalRollFocusDialog.forGoal(Goal goal) {
    return GoalRollFocusDialog(
      goalId: goal.id,
      goalContent: goal.content,
      initialFocus: goal.currentFocus,
      initialBlocker: goal.blocker,
    );
  }

  final String goalId;
  final String goalContent;
  final String? initialFocus;
  final String? initialBlocker;

  @override
  State<GoalRollFocusDialog> createState() => _GoalRollFocusDialogState();
}

class _GoalRollFocusDialogState extends State<GoalRollFocusDialog> {
  late final TextEditingController _focusController = TextEditingController(
    text: widget.initialFocus ?? '',
  );
  late final TextEditingController _blockerController = TextEditingController(
    text: widget.initialBlocker ?? '',
  );

  @override
  void dispose() {
    _focusController.dispose();
    _blockerController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      scrollable: true,
      title: Text(context.l10n.adjustActionTitle(widget.goalContent)),
      content: SizedBox(
        width: 520,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.l10n.nextPhaseHint,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _focusController,
              autofocus: true,
              minLines: 2,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: context.l10n.currentActionField,
                hintText: context.l10n.nextActionHint,
                alignLabelWithHint: true,
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _blockerController,
              minLines: 2,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: context.l10n.clearObstacleField,
                hintText: context.l10n.nextObstacleHint,
                alignLabelWithHint: true,
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: () {
            final focus = _focusController.text.trim();
            final blocker = _blockerController.text.trim();
            Navigator.of(context).pop((
              focus: focus.isEmpty ? null : focus,
              blocker: blocker.isEmpty ? null : blocker,
            ));
          },
          child: Text(context.l10n.updateAction),
        ),
      ],
    );
  }
}

/// 唤醒读档对话框（回显上下文，允许确认或修改新行动）
class _GoalReactivateDialog extends StatefulWidget {
  const _GoalReactivateDialog({required this.goal});
  final Goal goal;

  @override
  State<_GoalReactivateDialog> createState() => _GoalReactivateDialogState();
}

class _GoalReactivateDialogState extends State<_GoalReactivateDialog> {
  late final TextEditingController _focusController = TextEditingController(
    text: widget.goal.currentFocus ?? '',
  );
  late final TextEditingController _blockerController = TextEditingController(
    text: widget.goal.blocker ?? '',
  );

  @override
  void dispose() {
    _focusController.dispose();
    _blockerController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final why = widget.goal.why?.trim();
    return AlertDialog(
      scrollable: true,
      title: Text(context.l10n.reactivateGoalTitle(widget.goal.content)),
      content: SizedBox(
        width: 520,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (why != null && why.isNotEmpty) ...[
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: AppTheme.surface2,
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Text(
                  context.l10n.motivationReview(why),
                  style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                ),
              ),
              const SizedBox(height: 12),
            ],
            Text(
              context.l10n.restartGoalHint,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _focusController,
              autofocus: true,
              minLines: 2,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: context.l10n.restartActionField,
                hintText: context.l10n.restartActionHint,
                alignLabelWithHint: true,
              ),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _blockerController,
              minLines: 2,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: context.l10n.restartObstacleField,
                hintText: context.l10n.restartObstacleHint,
                alignLabelWithHint: true,
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: () {
            final focus = _focusController.text.trim();
            final blocker = _blockerController.text.trim();
            Navigator.of(context).pop((
              focus: focus.isEmpty ? null : focus,
              blocker: blocker.isEmpty ? null : blocker,
            ));
          },
          child: Text(context.l10n.confirmReactivate),
        ),
      ],
    );
  }
}

/// 编辑目标全貌对话框
class _GoalEditFullDialog extends StatefulWidget {
  const _GoalEditFullDialog({required this.goal});
  final Goal goal;

  @override
  State<_GoalEditFullDialog> createState() => _GoalEditFullDialogState();
}

class _GoalEditFullDialogState extends State<_GoalEditFullDialog> {
  late final TextEditingController _contentController = TextEditingController(
    text: widget.goal.content,
  );
  late final TextEditingController _whyController = TextEditingController(
    text: widget.goal.why ?? '',
  );
  late final TextEditingController _focusController = TextEditingController(
    text: widget.goal.currentFocus ?? '',
  );
  late final TextEditingController _blockerController = TextEditingController(
    text: widget.goal.blocker ?? '',
  );
  late final TextEditingController _doneController = TextEditingController(
    text: widget.goal.doneCriteria ?? '',
  );

  late GoalPhase _phase = widget.goal.phase;

  @override
  void dispose() {
    _contentController.dispose();
    _whyController.dispose();
    _focusController.dispose();
    _blockerController.dispose();
    _doneController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(context.l10n.editFullGoal),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              TextField(
                controller: _contentController,
                autofocus: true,
                minLines: 1,
                maxLines: 2,
                decoration: InputDecoration(labelText: context.l10n.goalVision),
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 6,
                children: [
                  for (final phase in GoalPhase.values)
                    ChoiceChip(
                      label: Text(_goalPhaseLabel(context, phase)),
                      selected: _phase == phase,
                      onSelected: (_) => setState(() => _phase = phase),
                      visualDensity: VisualDensity.compact,
                      labelStyle: const TextStyle(fontSize: 11.5),
                    ),
                ],
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _whyController,
                minLines: 2,
                maxLines: 6,
                decoration: InputDecoration(labelText: context.l10n.goalWhy),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _focusController,
                minLines: 2,
                maxLines: 6,
                decoration: InputDecoration(
                  labelText: context.l10n.goalNextAction,
                ),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _blockerController,
                minLines: 2,
                maxLines: 6,
                decoration: InputDecoration(
                  labelText: context.l10n.goalObstacle,
                ),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _doneController,
                minLines: 2,
                maxLines: 6,
                decoration: InputDecoration(labelText: context.l10n.goalDone),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: () {
            final content = _contentController.text.trim();
            if (content.isEmpty) return;
            final why = _whyController.text.trim();
            final focus = _focusController.text.trim();
            final blocker = _blockerController.text.trim();
            final done = _doneController.text.trim();

            Navigator.of(context).pop((
              content: content,
              phase: _phase,
              why: why.isEmpty ? null : why,
              focus: focus.isEmpty ? null : focus,
              blocker: blocker.isEmpty ? null : blocker,
              done: done.isEmpty ? null : done,
            ));
          },
          child: Text(context.l10n.saveChanges),
        ),
      ],
    );
  }
}

String _goalPhaseLabel(BuildContext context, GoalPhase phase) =>
    switch (phase) {
      GoalPhase.near => context.l10n.nearTerm,
      GoalPhase.mid => context.l10n.midTerm,
      GoalPhase.long => context.l10n.longTerm,
    };
String _goalStatusLabel(BuildContext context, GoalStatus status) =>
    switch (status) {
      GoalStatus.active => context.l10n.inProgressGoal,
      GoalStatus.paused => context.l10n.pausedGoalState,
      GoalStatus.completed => context.l10n.achievedGoals,
    };
