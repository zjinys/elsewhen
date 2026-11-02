import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/goal.dart';
import '../providers/conversation_provider.dart';
import '../providers/goal_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'goal_compass_banner.dart';
// goalCompassChatDraftProvider / MessageArea 输入预填机制定义在此文件。
import 'message_area.dart';

/// 活跃目标上限，与后端 `MAX_ACTIVE_GOALS` 及数据库触发器一致。
const int maxActiveGoals = 3;

/// 目标域「AI 共同立项 / 共同分解」统一入口：把指令预填到对话输入框并
/// 切回对话 tab，**绝不自动发送**——用户可改可发（与 compass「聚焦讨论」、
/// 习惯问候预填同一纪律）。实际分解由 AI 走 create_goal /
/// update_goal_facets 的 WriteConfirm 提议卡完成，抓手永远由用户确认才落库。
///
/// 返回 false 表示当前没有任何可用会话（含会话列表尚未加载），调用方
/// 应提示用户先去对话页；true 表示已预填并切换成功。
bool startGoalCoAuthor(
  WidgetRef ref,
  String seed, {
  void Function()? onSwitched,
}) {
  String? convId = ref.read(selectedConversationIdProvider);
  if (convId == null) {
    final convs = ref.read(conversationsProvider).value;
    final main = convs?.firstOrNull;
    if (main != null) {
      ref.read(selectedConversationIdProvider.notifier).set(main.id);
      convId = main.id;
    }
  }
  if (convId == null) return false;
  ref.read(goalCompassChatDraftProvider(convId).notifier).set(seed);
  ref.read(sidebarTabProvider.notifier).set(SidebarTab.conversation);
  onSwitched?.call();
  return true;
}

/// 目标域「AI 共同立项 / 共同分解」的 Toast 文案。
void showGoalCoAuthorToast(BuildContext context) {
  ScaffoldMessenger.of(context).showSnackBar(
    const SnackBar(
      content: Text('已带目标上下文到对话输入框，确认或修改后发送即可共同拆解'),
      behavior: SnackBarBehavior.floating,
      duration: Duration(seconds: 3),
    ),
  );
}

/// 历史归档视图子分类
enum _StatusFilter {
  active,
  paused,
  completed,
}

/// 目标管理面板（FR-PES-005-01 / Roadmap Phase G3）：
/// 认知型目标工作台（切片化卡片、滚动抓手、三态流转与无缝读档复活）。
///
/// 两个挂载点内容刻意不同：
/// - 左侧导航「目标」tab（默认，可编辑）：完整工作台，按状态过滤浏览
///   （缺省「活跃」，可切「暂停搁置」「已达成」），写入口与 AI 共同
///   立项/拆解都收敛在这里。
/// - 对话界面「目标」弹窗（[quickGlance] = true）：只读速览——仅活跃
///   目标切片，零写操作、零 AI 入口、无历史归档。对话中瞄一眼目标
///   现状用；所有操作引流到左 tab 工作台。
///
/// 单一写入口纪律：目标写操作只经对话流（WriteConfirm 提议卡）与
/// 左 tab 工作台，避免同一功能多入口语义漂移。
class GoalListView extends ConsumerStatefulWidget {
  const GoalListView({super.key, this.quickGlance = false});

  /// 只读速览模式：供对话界面目标弹窗使用，只呈现活跃目标切片。
  final bool quickGlance;

  @override
  ConsumerState<GoalListView> createState() => _GoalListViewState();
}

class _GoalListViewState extends ConsumerState<GoalListView> {
  final _contentController = TextEditingController();
  final _whyController = TextEditingController();
  final _focusController = TextEditingController();
  final _blockerController = TextEditingController();
  final _doneController = TextEditingController();

  GoalPhase _newPhase = GoalPhase.near;
  bool _saving = false;
  bool _expandFacets = false;

  /// 状态过滤视图（可编辑工作台）：缺省看活跃。
  _StatusFilter _statusFilter = _StatusFilter.active;

  @override
  void dispose() {
    _contentController.dispose();
    _whyController.dispose();
    _focusController.dispose();
    _blockerController.dispose();
    _doneController.dispose();
    super.dispose();
  }

  void _toast(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), behavior: SnackBarBehavior.floating),
    );
  }

  void _refreshAll() {
    ref.invalidate(activeGoalsProvider);
    ref.invalidate(archivedGoalsProvider);
    ref.invalidate(pausedGoalsProvider);
    ref.invalidate(completedGoalsProvider);
    ref.invalidate(activeCompassNudgeProvider);
  }

  Future<void> _add() async {
    final content = _contentController.text.trim();
    if (content.isEmpty || _saving) return;
    setState(() => _saving = true);
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
      _contentController.clear();
      _whyController.clear();
      _focusController.clear();
      _blockerController.clear();
      _doneController.clear();
      setState(() => _expandFacets = false);
      _refreshAll();
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  /// 「和 AI 拆解 / 立新目标」：预填对话草稿并切回对话 tab。
  /// 提及当前活跃目标作为上下文，AI 会看到 system_context 里的
  /// 完整切片并走提议卡确认，这里不做任何直接写库。
  void _startCoAuthor(int activeCount) {
    final activeAsync = ref.read(activeGoalsProvider);
    final goals = activeAsync.value ?? const <Goal>[];
    final goalBrief = goals.isEmpty
        ? '目前还没有活跃目标。'
        : '当前活跃目标：${goals.map((g) => '「${g.content}」（抓手：${g.currentFocus ?? "未设定"}）').join('、')}。';
    final seed = activeCount == 0
        ? '我想立一个新的长程目标。请用几个设问帮我明确：为什么做、什么算完成、近 1~2 周的第一启动抓手是什么、可能卡在哪。$goalBrief'
        : '帮我看看这些目标的推进情况：哪个抓手该滚动到下一个切片了？或者帮我共同规划一个新的目标。$goalBrief';
    _launchCoAuthor(seed);
  }

  /// 卡片/tab 级 AI 辅助：种子只带这一条目标的完整切片上下文，
  /// AI 会看到 system_context 并走提议卡确认，这里不做任何直接写库。
  void _startGoalCoAuthor(Goal goal) {
    final focus = goal.currentFocus?.trim();
    final blocker = goal.blocker?.trim();
    final why = goal.why?.trim();
    final buf = StringBuffer('关于目标「${goal.content}」');
    if (why != null && why.isNotEmpty) buf.write('（为什么做：$why）');
    if (focus != null && focus.isNotEmpty) {
      buf.write('，当前抓手是「$focus」');
    } else {
      buf.write('，还没设定当前抓手');
    }
    if (blocker != null && blocker.isNotEmpty) {
      buf.write('，卡在「$blocker」');
    }
    buf.write('。帮我拆解下一步：先聊聊近 1~2 周该做什么、怎么验证推进了。');

    _launchCoAuthor(buf.toString());
  }

  /// 共同立项/拆解的实际触发：预填草稿并切回对话 tab。
  /// 只在左 tab 可编辑工作台可用（速览弹窗无 AI 入口）。
  void _launchCoAuthor(String seed) {
    final ok = startGoalCoAuthor(ref, seed);
    if (!ok) {
      _toast('暂无可用会话，请先在对话页新建会话后再试');
      return;
    }
    if (mounted) showGoalCoAuthorToast(context);
  }

  Future<void> _pause(Goal goal) async {
    if (widget.quickGlance) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.pauseGoal(goal.id);
      _refreshAll();
      _toast('已将「${goal.content}」暂时搁置');
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _complete(Goal goal) async {
    if (widget.quickGlance) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.completeGoal(goal.id);
      _refreshAll();
      _toast('🎉 达成目标「${goal.content}」！');
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
    if (result == null) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateGoalFocus(
        id: goal.id,
        currentFocus: result.focus,
        blocker: result.blocker,
      );
      _refreshAll();
      _toast('已更新抓手与卡点');
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _editFull(Goal goal) async {
    if (widget.quickGlance) return;
    final result = await showDialog<
        ({
          String content,
          GoalPhase phase,
          String? why,
          String? focus,
          String? blocker,
          String? done,
        })>(
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
      _refreshAll();
      _toast('已保存目标全貌');
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _reactivate(Goal goal, int activeCount) async {
    if (widget.quickGlance) return;
    if (activeCount >= maxActiveGoals) {
      _toast('当前已有 $maxActiveGoals 个活跃目标（精力上限）。请先暂停或达成一个，再腾出精力唤醒此目标。');
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
      _refreshAll();
      _toast('已唤醒「${goal.content}」并设为活跃目标');
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  @override
  Widget build(BuildContext context) {
    final activeAsync = ref.watch(activeGoalsProvider);
    final pausedAsync = ref.watch(pausedGoalsProvider);
    final completedAsync = ref.watch(completedGoalsProvider);
    final nudge = ref.watch(activeCompassNudgeProvider);
    final activeCount = activeAsync.value?.length ?? 0;

    // 速览模式：只看活跃目标切片，零操作零 AI 入口
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
                  '活跃目标 $activeCount / $maxActiveGoals',
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
                const Spacer(),
                Text(
                  '管理与 AI 讨论在「目标」页',
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
                '长程目标与航向',
                style: TextStyle(
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(width: 6),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1.5),
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
            ],
          ),
          const SizedBox(height: 3),
          Text(
            '活跃目标上限严格控制在 $maxActiveGoals 个以内，守恒注意力。通过滚动抓手持续向前推进。',
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
          const SizedBox(height: 8),
          // AI 共同立项入口：预填对话草稿并切回对话 tab，实际落库仍走
          // create_goal / update_goal_facets 的 WriteConfirm 提议卡。
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: activeCount >= maxActiveGoals
                  ? null
                  : () => _startCoAuthor(activeCount),
              icon: const Icon(Icons.auto_awesome, size: 14),
              label: Text(
                activeCount == 0
                    ? '和 AI 聊聊，共同立一个目标'
                    : '和 AI 拆解 / 立新目标',
                style: const TextStyle(fontSize: 11.5),
              ),
              style: TextButton.styleFrom(
                visualDensity: VisualDensity.compact,
                padding: const EdgeInsets.symmetric(
                  horizontal: 8,
                  vertical: 2,
                ),
              ),
            ),
          ),
          const SizedBox(height: 6),
          if (nudge != null) GoalCompassNudgeCard(nudge: nudge),
          const SizedBox(height: 8),
          // 状态过滤：缺省看活跃，可切暂停搁置/已达成
          SegmentedButton<_StatusFilter>(
            segments: [
              ButtonSegment(
                value: _StatusFilter.active,
                label: Text('活跃 ($activeCount)'),
                icon: const Icon(Icons.play_arrow_outlined, size: 14),
              ),
              ButtonSegment(
                value: _StatusFilter.paused,
                label: Text('暂停搁置 (${pausedAsync.value?.length ?? 0})'),
                icon: const Icon(Icons.pause_circle_outline, size: 14),
              ),
              ButtonSegment(
                value: _StatusFilter.completed,
                label: Text('已达成 (${completedAsync.value?.length ?? 0})'),
                icon: const Icon(Icons.emoji_events_outlined, size: 14),
              ),
            ],
            selected: {_statusFilter},
            onSelectionChanged: (set) {
              setState(() => _statusFilter = set.first);
            },
            style: const ButtonStyle(
              visualDensity: VisualDensity.compact,
              textStyle: WidgetStatePropertyAll(TextStyle(fontSize: 11)),
            ),
          ),
          const SizedBox(height: 12),
          switch (_statusFilter) {
            _StatusFilter.active => _buildActiveSection(activeAsync),
            _StatusFilter.paused => _buildPausedList(
                pausedAsync.value ?? [],
                activeCount,
              ),
            _StatusFilter.completed => _buildCompletedList(
                completedAsync.value ?? [],
                activeCount,
              ),
          },
          const SizedBox(height: 16),
          _buildAddForm(activeCount),
        ],
      ),
    );
  }

  Widget _buildActiveSection(AsyncValue<List<Goal>> activeAsync) {
    return activeAsync.when(
      loading: () => const Padding(
        padding: EdgeInsets.symmetric(vertical: 16),
        child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
      ),
      error: (e, _) => Text(
        '目标加载失败：$e',
        style: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
      ),
      data: (goals) {
        if (goals.isEmpty) {
          return Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 24, horizontal: 16),
            decoration: BoxDecoration(
              color: AppTheme.surface1,
              border: Border.all(color: AppTheme.surface3),
              borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
            ),
            child: Column(
              children: [
                Icon(
                  Icons.flag_outlined,
                  size: 26,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(height: 8),
                Text(
                  '还没设定目标',
                  style: TextStyle(
                    fontSize: 13.5,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  '设立 1~3 个近期最关心的长程愿景，明确近 1~2 周的首发抓手',
                  textAlign: TextAlign.center,
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              ],
            ),
          );
        }
        return Column(
          children: [
            for (final goal in goals) _buildActiveCard(goal),
          ],
        );
      },
    );
  }

  Widget _buildActiveCard(Goal goal) {
    return _ActiveGoalCard(
      goal: goal,
      quickGlance: widget.quickGlance,
      onRollFocus: () => _rollFocus(goal),
      onEditFull: () => _editFull(goal),
      onPause: () => _pause(goal),
      onComplete: () => _complete(goal),
      onCoAuthor: widget.quickGlance ? null : () => _startGoalCoAuthor(goal),
    );
  }

  Widget _buildAddForm(int activeCount) {
    final atCap = activeCount >= maxActiveGoals;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _contentController,
                  enabled: !_saving && !atCap,
                  onSubmitted: (_) => _add(),
                  decoration: InputDecoration(
                    hintText: atCap
                        ? '已达 $maxActiveGoals 条上限，请先归档一条'
                        : '设立新目标愿景…（如：发布 Elsewhen 1.0）',
                    hintStyle: TextStyle(
                      fontSize: 12.5,
                      color: AppTheme.textTertiary,
                    ),
                    prefixIcon: const Icon(Icons.add_task, size: 16),
                    isDense: true,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton.tonal(
                onPressed: (_saving || atCap) ? null : _add,
                child: _saving
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('添加'),
              ),
            ],
          ),
          const SizedBox(height: 8),
          // 阶段选择与切片展开：Wrap 布局，窄容器下自动换行不溢出
          Wrap(
            spacing: 6,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              for (final phase in GoalPhase.values)
                ChoiceChip(
                  label: Text(phase.label),
                  selected: _newPhase == phase,
                  onSelected: atCap
                      ? null
                      : (_) => setState(() => _newPhase = phase),
                  visualDensity: VisualDensity.compact,
                  labelStyle: const TextStyle(fontSize: 11.5),
                ),
              TextButton.icon(
                onPressed: atCap
                    ? null
                    : () => setState(() => _expandFacets = !_expandFacets),
                icon: Icon(
                  _expandFacets ? Icons.expand_less : Icons.expand_more,
                  size: 16,
                ),
                label: Text(_expandFacets ? '收起认知切片' : '补充动力/抓手/卡点'),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  textStyle: const TextStyle(fontSize: 11.5),
                ),
              ),
            ],
          ),
          if (_expandFacets && !atCap) ...[
            const SizedBox(height: 8),
            TextField(
              controller: _whyController,
              decoration: const InputDecoration(
                labelText: '💡 为什么做 / 核心动力（可选）',
                isDense: true,
                hintText: '如：验证个人认知系统的完整闭环',
              ),
            ),
            const SizedBox(height: 8),
            TextField(
              controller: _focusController,
              decoration: const InputDecoration(
                labelText: '📍 当前 1~2 周启动抓手（可选）',
                isDense: true,
                hintText: '如：攻克多端同步快照收敛',
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _blockerController,
                    decoration: const InputDecoration(
                      labelText: '🚧 已知卡点 / 困难（可选）',
                      isDense: true,
                      hintText: '如：真机证书配置较繁琐',
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: TextField(
                    controller: _doneController,
                    decoration: const InputDecoration(
                      labelText: '🎯 验收标准（可选）',
                      isDense: true,
                      hintText: '如：多端测试全绿通过',
                    ),
                  ),
                ),
              ],
            ),
          ],
          if (atCap)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                '活跃目标守恒上限为 $maxActiveGoals 个。暂停或完成一条后即可设立新愿景。',
                style: TextStyle(fontSize: 11.5, color: AppTheme.accentPrimary),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildPausedList(List<Goal> goals, int activeCount) {
    if (goals.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 12),
        child: Center(
          child: Text(
            '暂无暂停搁置的目标',
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
        ),
      );
    }
    return Column(
      children: [
        for (final goal in goals)
          _PausedGoalTile(
            goal: goal,
            readOnly: widget.quickGlance,
            onReactivate: () => _reactivate(goal, activeCount),
          ),
      ],
    );
  }

  Widget _buildCompletedList(List<Goal> goals, int activeCount) {
    if (goals.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 12),
        child: Center(
          child: Text(
            '暂无已达成的成就勋章',
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
        ),
      );
    }
    return Column(
      children: [
        for (final goal in goals)
          _CompletedGoalTile(
            goal: goal,
            readOnly: widget.quickGlance,
            onReopen: () => _reactivate(goal, activeCount),
          ),
      ],
    );
  }
}

/// 活跃目标卡片（高亮呈现当前抓手与已知卡点）
class _ActiveGoalCard extends StatelessWidget {
  const _ActiveGoalCard({
    required this.goal,
    required this.onRollFocus,
    required this.onEditFull,
    required this.onPause,
    required this.onComplete,
    this.onCoAuthor,
    this.quickGlance = false,
  });

  final Goal goal;
  final VoidCallback onRollFocus;
  final VoidCallback onEditFull;
  final VoidCallback onPause;
  final VoidCallback onComplete;

  /// 卡片级 AI 辅助入口（带本目标上下文预填对话草稿）；
  /// 速览模式下隐藏。实际落库仍走 WriteConfirm 提议卡。
  final VoidCallback? onCoAuthor;

  /// 只读速览模式：隐藏滚动抓手按钮与操作菜单。
  final bool quickGlance;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
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
                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                  decoration: BoxDecoration(
                    color: AppTheme.surface3,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    goal.phase.label,
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
                if (!quickGlance)
                  IconButton(
                    tooltip: '滚动抓手',
                    visualDensity: VisualDensity.compact,
                    icon: const Icon(Icons.tune_outlined, size: 16),
                    color: AppTheme.accentPrimary,
                    onPressed: onRollFocus,
                  ),
                if (!quickGlance)
                  PopupMenuButton<String>(
                  tooltip: '更多操作',
                  icon: const Icon(Icons.more_horiz, size: 16),
                  color: AppTheme.surface2,
                  onSelected: (val) {
                    switch (val) {
                      case 'roll':
                        onRollFocus();
                        break;
                      case 'complete':
                        onComplete();
                        break;
                      case 'pause':
                        onPause();
                        break;
                      case 'edit':
                        onEditFull();
                        break;
                    }
                  },
                  itemBuilder: (_) => [
                    const PopupMenuItem(
                      value: 'roll',
                      child: Row(
                        children: [
                          Icon(Icons.tune_outlined, size: 16),
                          SizedBox(width: 8),
                          Text('滚动抓手 / 卡点'),
                        ],
                      ),
                    ),
                    const PopupMenuItem(
                      value: 'complete',
                      child: Row(
                        children: [
                          Icon(Icons.check_circle_outline, size: 16),
                          SizedBox(width: 8),
                          Text('达成目标（归档为成就）'),
                        ],
                      ),
                    ),
                    const PopupMenuItem(
                      value: 'pause',
                      child: Row(
                        children: [
                          Icon(Icons.pause_circle_outline, size: 16),
                          SizedBox(width: 8),
                          Text('暂时搁置（随时读档复活）'),
                        ],
                      ),
                    ),
                    const PopupMenuDivider(),
                    const PopupMenuItem(
                      value: 'edit',
                      child: Row(
                        children: [
                          Icon(Icons.edit_note, size: 16),
                          SizedBox(width: 8),
                          Text('编辑全貌'),
                        ],
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),

          // 切片高亮区
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 4, 12, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 抓手条
                InkWell(
                  onTap: quickGlance ? null : onRollFocus,
                  borderRadius: BorderRadius.circular(6),
                  child: Container(
                    width: double.infinity,
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
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
                                  text: '当前抓手：',
                                  style: TextStyle(
                                    fontWeight: FontWeight.w600,
                                    color: AppTheme.textPrimary,
                                  ),
                                ),
                                TextSpan(
                                  text: (focus != null && focus.isNotEmpty)
                                      ? focus
                                      : '暂未设定（点击快速设定当前 1~2 周切片）',
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
                        Icon(
                          Icons.edit_outlined,
                          size: 13,
                          color: quickGlance
                              ? Colors.transparent
                              : AppTheme.textTertiary,
                        ),
                      ],
                    ),
                  ),
                ),

                // 卡点条
                if (blocker != null && blocker.isNotEmpty) ...[
                  const SizedBox(height: 6),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
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
                                  text: '已知卡点：',
                                  style: TextStyle(
                                    fontWeight: FontWeight.w600,
                                    color: theme.colorScheme.error,
                                  ),
                                ),
                                TextSpan(
                                  text: blocker,
                                  style: TextStyle(color: AppTheme.textSecondary),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],

                // 动机与判据（紧凑展示）
                if ((why != null && why.isNotEmpty) ||
                    (done != null && done.isNotEmpty)) ...[
                  const SizedBox(height: 6),
                  Wrap(
                    spacing: 12,
                    runSpacing: 4,
                    children: [
                      if (why != null && why.isNotEmpty)
                        Text(
                          '💡 动力：$why',
                          style: TextStyle(
                            fontSize: 11.5,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      if (done != null && done.isNotEmpty)
                        Text(
                          '🎯 判据：$done',
                          style: TextStyle(
                            fontSize: 11.5,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                    ],
                  ),
                ],

                // 卡片级 AI 辅助：只预填对话草稿，落库仍走提议卡确认
                if (onCoAuthor != null) ...[
                  const SizedBox(height: 8),
                  TextButton.icon(
                    onPressed: onCoAuthor,
                    icon: const Icon(Icons.auto_awesome, size: 13),
                    label: const Text('AI 辅助拆解', style: TextStyle(fontSize: 11.5)),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 8,
                        vertical: 2,
                      ),
                    ),
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

/// 暂停搁置的目标项（支持无缝读档唤醒）
class _PausedGoalTile extends StatelessWidget {
  const _PausedGoalTile({
    required this.goal,
    required this.onReactivate,
    this.readOnly = false,
  });

  final Goal goal;
  final VoidCallback onReactivate;

  /// 只读总览模式：隐藏重新激活按钮。
  final bool readOnly;

  @override
  Widget build(BuildContext context) {
    final focus = goal.currentFocus?.trim();
    final blocker = goal.blocker?.trim();
    final why = goal.why?.trim();

    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                goal.phase.label,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textTertiary,
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  goal.content,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
              ),
              if (!readOnly)
                Tooltip(
                  message: '复活（需有名额）',
                  child: FilledButton.tonalIcon(
                    onPressed: onReactivate,
                    icon: const Icon(Icons.play_arrow_outlined, size: 15),
                    label: const Text('重新激活'),
                    style: FilledButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      textStyle: const TextStyle(fontSize: 11.5),
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 0),
                    ),
                  ),
                ),
            ],
          ),
          if (focus != null && focus.isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              '📍 上次停顿抓手：$focus',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ],
          if (blocker != null && blocker.isNotEmpty) ...[
            const SizedBox(height: 2),
            Text(
              '🚧 留存卡点：$blocker',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ],
          if (why != null && why.isNotEmpty) ...[
            const SizedBox(height: 2),
            Text(
              '💡 核心动力：$why',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ],
        ],
      ),
    );
  }
}

/// 已达成的成就勋章项
class _CompletedGoalTile extends StatelessWidget {
  const _CompletedGoalTile({
    required this.goal,
    required this.onReopen,
    this.readOnly = false,
  });

  final Goal goal;
  final VoidCallback onReopen;

  /// 只读总览模式：隐藏重新激活按钮。
  final bool readOnly;

  @override
  Widget build(BuildContext context) {
    final done = goal.doneCriteria?.trim();
    final why = goal.why?.trim();
    final date = goal.supersededAt ?? goal.updatedAt;
    final dateStr = '${date.year}/${date.month}/${date.day}';

    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(
                Icons.emoji_events_outlined,
                size: 16,
                color: Color(0xFFD4AF37), // 成就金
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  goal.content,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
              ),
              Text(
                '达成于 $dateStr',
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
              if (!readOnly)
                IconButton(
                  tooltip: '重新激活为此目标',
                  visualDensity: VisualDensity.compact,
                  icon: const Icon(Icons.refresh, size: 15),
                  color: AppTheme.textTertiary,
                  onPressed: onReopen,
                ),
            ],
          ),
          if (done != null && done.isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              '🎯 验收标准：$done',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ],
          if (why != null && why.isNotEmpty) ...[
            const SizedBox(height: 2),
            Text(
              '💡 最初动力：$why',
              style: TextStyle(fontSize: 11.5, color: AppTheme.textTertiary),
            ),
          ],
        ],
      ),
    );
  }
}

/// 滚动抓手与卡点对话框（最常用的轻量认知更新）
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
      title: Text('滚动抓手 · ${widget.goalContent}'),
      content: SizedBox(
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '攻克上一切片后，将注意力收敛到近 1~2 周最先启动的下一个切片。',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _focusController,
              autofocus: true,
              maxLines: 2,
              decoration: const InputDecoration(
                labelText: '📍 当前抓手',
                hintText: '如：完成首版同步快照算法并跑通测试',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _blockerController,
              maxLines: 2,
              decoration: const InputDecoration(
                labelText: '🚧 已知卡点（可留空清除）',
                hintText: '如：等待对方确认网络穿透端口',
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
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
          child: const Text('更新抓手'),
        ),
      ],
    );
  }
}

/// 唤醒读档对话框（回显上下文，允许确认或修改新抓手）
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
      title: Text('读档唤醒 · ${widget.goal.content}'),
      content: SizedBox(
        width: 480,
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
                  '💡 核心动力回顾：$why',
                  style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                ),
              ),
              const SizedBox(height: 12),
            ],
            Text(
              '重新启动此目标，请确认或设定接下来的第一启动抓手：',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _focusController,
              autofocus: true,
              maxLines: 2,
              decoration: const InputDecoration(
                labelText: '📍 重新启动抓手',
                hintText: '如：理清代码依赖并设计重构方案',
              ),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _blockerController,
              maxLines: 2,
              decoration: const InputDecoration(
                labelText: '🚧 当前已知卡点（可选）',
                hintText: '如：缺少相关设备环境',
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
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
          child: const Text('确认唤醒'),
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
      title: const Text('编辑目标全貌'),
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
                decoration: const InputDecoration(labelText: '目标愿景内容'),
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 6,
                children: [
                  for (final phase in GoalPhase.values)
                    ChoiceChip(
                      label: Text(phase.label),
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
                decoration: const InputDecoration(labelText: '💡 为什么做 / 核心动力'),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _focusController,
                decoration: const InputDecoration(labelText: '📍 当前 1~2 周抓手'),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _blockerController,
                decoration: const InputDecoration(labelText: '🚧 已知卡点 / 困难'),
              ),
              const SizedBox(height: 10),
              TextField(
                controller: _doneController,
                decoration: const InputDecoration(labelText: '🎯 验收判据 / 完成标准'),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
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
          child: const Text('保存修改'),
        ),
      ],
    );
  }
}
