import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/goal.dart';
import '../providers/goal_provider.dart';
import '../theme/app_theme.dart';

/// 活跃目标上限，与后端 `MAX_ACTIVE_GOALS` 及数据库触发器一致。
///
/// 达上限时禁用新增并提示先归档；后端仍是最终防线，这里只是提前告知，
/// 不能当作唯一约束。
const int maxActiveGoals = 3;

/// 目标管理面板（FR-PES-005-01）：活跃目标 + 新增/编辑/归档 + 历史归档。
///
/// 挂载点是对话区顶部「今天」状态栏的目标入口（不新增一级导航），
/// 所以本面板自带关闭按钮。
class GoalListView extends ConsumerStatefulWidget {
  const GoalListView({super.key});

  @override
  ConsumerState<GoalListView> createState() => _GoalListViewState();
}

class _GoalListViewState extends ConsumerState<GoalListView> {
  final _inputController = TextEditingController();
  GoalPhase _newPhase = GoalPhase.near;
  bool _saving = false;

  @override
  void dispose() {
    _inputController.dispose();
    super.dispose();
  }

  void _toast(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), behavior: SnackBarBehavior.floating),
    );
  }

  Future<void> _add() async {
    final content = _inputController.text.trim();
    if (content.isEmpty || _saving) return;
    setState(() => _saving = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.createGoal(content: content, phase: _newPhase);
      if (!mounted) return;
      _inputController.clear();
      ref.invalidate(activeGoalsProvider);
    } catch (e) {
      // 后端拒绝时给出的是可展示的中文（已在上游把触发器标识翻过一遍）。
      _toast(e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _archive(Goal goal) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.archiveGoal(goal.id);
      ref.invalidate(activeGoalsProvider);
      ref.invalidate(archivedGoalsProvider);
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _reactivate(Goal goal) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.reactivateGoal(goal.id);
      ref.invalidate(activeGoalsProvider);
      ref.invalidate(archivedGoalsProvider);
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _edit(Goal goal) async {
    final result = await showDialog<({String content, GoalPhase phase})>(
      context: context,
      builder: (_) => _GoalEditDialog(goal: goal),
    );
    if (result == null) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateGoal(
        id: goal.id,
        content: result.content,
        phase: result.phase,
      );
      ref.invalidate(activeGoalsProvider);
    } catch (e) {
      _toast(e.toString().replaceFirst('Exception: ', ''));
    }
  }

  @override
  Widget build(BuildContext context) {
    final activeAsync = ref.watch(activeGoalsProvider);
    final archivedAsync = ref.watch(archivedGoalsProvider);

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.flag_outlined,
                size: 18,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 8),
              Text(
                '我的目标',
                style: TextStyle(
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const Spacer(),
              IconButton(
                tooltip: '关闭',
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.close, size: 18),
                color: AppTheme.textSecondary,
                onPressed: () => Navigator.of(context).maybePop(),
              ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            '最多 $maxActiveGoals 条，阶段不必齐全。认知系统靠它们判断你的行为与目标有多大偏差。',
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
          const SizedBox(height: 12),
          _buildActive(activeAsync),
          const SizedBox(height: 12),
          _buildAddForm(activeAsync),
          const SizedBox(height: 16),
          _buildHistory(archivedAsync),
        ],
      ),
    );
  }

  Widget _buildActive(AsyncValue<List<Goal>> activeAsync) {
    return activeAsync.when(
      loading: () => const Padding(
        padding: EdgeInsets.symmetric(vertical: 12),
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
            padding: const EdgeInsets.symmetric(vertical: 18),
            decoration: BoxDecoration(
              border: Border.all(color: AppTheme.surface3),
              borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
            ),
            child: Column(
              children: [
                Icon(
                  Icons.flag_outlined,
                  size: 22,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(height: 6),
                Text(
                  '还没设定目标',
                  style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
                ),
                const SizedBox(height: 2),
                Text(
                  '写下你最近真正想推进的事',
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              ],
            ),
          );
        }
        return Column(
          children: [
            for (final goal in goals)
              _GoalTile(
                goal: goal,
                onEdit: () => _edit(goal),
                onArchive: () => _archive(goal),
              ),
          ],
        );
      },
    );
  }

  Widget _buildAddForm(AsyncValue<List<Goal>> activeAsync) {
    final count = activeAsync.value?.length ?? 0;
    // 达到上限时预先禁用。这里只是提前告知，后端触发器才是最终防线。
    final atCap = count >= maxActiveGoals;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _inputController,
                enabled: !_saving && !atCap,
                onSubmitted: (_) => _add(),
                decoration: InputDecoration(
                  hintText: atCap
                      ? '已达 $maxActiveGoals 条上限，请先归档一条'
                      : '写一个你近期真正想推进的事…（回车添加）',
                  hintStyle: TextStyle(
                    fontSize: 12.5,
                    color: AppTheme.textTertiary,
                  ),
                  prefixIcon: const Icon(Icons.flag_outlined, size: 16),
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
        Wrap(
          spacing: 6,
          children: [
            for (final phase in GoalPhase.values)
              ChoiceChip(
                label: Text(phase.label),
                selected: _newPhase == phase,
                onSelected: (_) => setState(() => _newPhase = phase),
                visualDensity: VisualDensity.compact,
                labelStyle: const TextStyle(fontSize: 12),
              ),
          ],
        ),
        if (atCap)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(
              '已达到 $maxActiveGoals 条上限。归档一条后才能新增——系统不会自动顶掉你写好的目标。',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          ),
      ],
    );
  }

  Widget _buildHistory(AsyncValue<List<Goal>> archivedAsync) {
    return archivedAsync.when(
      loading: () => const SizedBox.shrink(),
      error: (_, _) => const SizedBox.shrink(),
      data: (goals) {
        if (goals.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '历史归档',
              style: TextStyle(
                fontSize: 12.5,
                fontWeight: FontWeight.w600,
                color: AppTheme.textSecondary,
              ),
            ),
            const SizedBox(height: 6),
            for (final goal in goals)
              _ArchivedTile(goal: goal, onReactivate: () => _reactivate(goal)),
          ],
        );
      },
    );
  }
}

class _GoalTile extends StatelessWidget {
  final Goal goal;
  final VoidCallback onEdit;
  final VoidCallback onArchive;

  const _GoalTile({
    required this.goal,
    required this.onEdit,
    required this.onArchive,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.fromLTRB(10, 8, 4, 8),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Text(
              goal.phase.label,
              style: TextStyle(
                fontSize: 11,
                color: AppTheme.accentPrimary,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              goal.content,
              style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
            ),
          ),
          IconButton(
            tooltip: '编辑',
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.edit_outlined, size: 15),
            color: AppTheme.textTertiary,
            onPressed: onEdit,
          ),
          IconButton(
            tooltip: '归档（让出名额，保留为历史）',
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.archive_outlined, size: 15),
            color: AppTheme.textTertiary,
            onPressed: onArchive,
          ),
        ],
      ),
    );
  }
}

class _ArchivedTile extends StatelessWidget {
  final Goal goal;
  final VoidCallback onReactivate;

  const _ArchivedTile({required this.goal, required this.onReactivate});

  @override
  Widget build(BuildContext context) {
    final archivedAt = goal.supersededAt;
    final when = archivedAt == null
        ? ''
        : '${archivedAt.month}/${archivedAt.day}';
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.fromLTRB(10, 7, 4, 7),
      decoration: BoxDecoration(
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Row(
        children: [
          Text(
            goal.phase.label,
            style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              goal.content,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
            ),
          ),
          if (when.isNotEmpty)
            Text(
              when,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
          IconButton(
            tooltip: '复活（需有名额）',
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.unarchive_outlined, size: 15),
            color: AppTheme.textTertiary,
            onPressed: onReactivate,
          ),
        ],
      ),
    );
  }
}

class _GoalEditDialog extends StatefulWidget {
  final Goal goal;

  const _GoalEditDialog({required this.goal});

  @override
  State<_GoalEditDialog> createState() => _GoalEditDialogState();
}

class _GoalEditDialogState extends State<_GoalEditDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.goal.content,
  );
  late GoalPhase _phase = widget.goal.phase;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('编辑目标'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _controller,
            autofocus: true,
            maxLines: 3,
            decoration: const InputDecoration(hintText: '目标内容'),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 6,
            children: [
              for (final phase in GoalPhase.values)
                ChoiceChip(
                  label: Text(phase.label),
                  selected: _phase == phase,
                  onSelected: (_) => setState(() => _phase = phase),
                  visualDensity: VisualDensity.compact,
                  labelStyle: const TextStyle(fontSize: 12),
                ),
            ],
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () {
            final content = _controller.text.trim();
            if (content.isEmpty) return;
            Navigator.of(context).pop((content: content, phase: _phase));
          },
          child: const Text('保存'),
        ),
      ],
    );
  }
}
