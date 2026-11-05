import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/goal.dart';
import '../providers/goal_provider.dart';
import '../theme/app_theme.dart';
import 'goal_view.dart';

/// 航向偏离校准横幅（挂在 MessageArea 状态栏下方，与 ReminderBanner 并列）。
/// 当 3~5 天滑动窗口检测到活跃目标抓手停滞且精力分散时呈现。
class GoalCompassNudgeBanner extends ConsumerWidget {
  const GoalCompassNudgeBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final nudge = ref.watch(activeCompassNudgeProvider);
    if (nudge == null) return const SizedBox.shrink();

    // 尝试从缓存的活跃目标中提取当前卡点
    final activeGoals = ref.watch(activeGoalsProvider).value ?? const [];
    final goal = activeGoals.cast<Goal?>().firstWhere(
          (g) => g?.id == nudge.goalId,
          orElse: () => null,
        );
    final blocker = goal?.blocker;

    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: 0.08),
        border: Border(
          bottom: BorderSide(
            color: AppTheme.accentPrimary.withValues(alpha: 0.2),
          ),
        ),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Icon(
                Icons.explore_outlined,
                size: 17,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  nudge.planIntent != null
                      ? '航向校准 · 计划「${nudge.planIntent}」'
                      : '航向校准 · 目标「${nudge.goalContent}」',
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              IconButton(
                tooltip: '处理急事，7 天内不打扰',
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.close, size: 16),
                color: AppTheme.textTertiary,
                onPressed: () => _dismiss(context, ref, nudge),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text.rich(
            TextSpan(
              style: TextStyle(fontSize: 12, color: AppTheme.textSecondary, height: 1.4),
              children: [
                const TextSpan(text: '当前抓手「'),
                TextSpan(
                  text: nudge.currentFocus,
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
                TextSpan(text: '」已停滞 ${nudge.stalledDays} 天'),
                if (blocker != null && blocker.trim().isNotEmpty) ...[
                  const TextSpan(text: '，卡点：'),
                  TextSpan(
                    text: blocker,
                    style: TextStyle(
                      color: AppTheme.accentPrimary,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ],
                TextSpan(text: '。近期注意力多在「${nudge.recentTopics}」。'),
              ],
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              FilledButton.tonal(
                style: FilledButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                ),
                onPressed: () => _focusAndDiscuss(
                  context,
                  ref,
                  nudge,
                  blocker: blocker,
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.chat_bubble_outline, size: 13),
                    const SizedBox(width: 4),
                    Text(
                      blocker != null && blocker.trim().isNotEmpty
                          ? '聚焦攻克此卡点'
                          : '聊聊推进破局',
                      style: const TextStyle(fontSize: 12),
                    ),
                  ],
                ),
              ),
              OutlinedButton(
                style: OutlinedButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                  side: BorderSide(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.3),
                  ),
                ),
                onPressed: () => _rollFocus(
                  context,
                  ref,
                  nudge,
                  initialBlocker: blocker,
                ),
                child: const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.update_outlined, size: 13),
                    SizedBox(width: 4),
                    Text('更新当前抓手', style: TextStyle(fontSize: 12)),
                  ],
                ),
              ),
              TextButton(
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  foregroundColor: AppTheme.textTertiary,
                ),
                onPressed: () => _dismiss(context, ref, nudge),
                child: const Text('暂不打扰（退避7天）', style: TextStyle(fontSize: 11.5)),
              ),
            ],
          ),
        ],
      ),
    );
  }

  static void _focusAndDiscuss(
    BuildContext context,
    WidgetRef ref,
    GoalCompassNudge nudge, {
    String? blocker,
    VoidCallback? onBeforeDiscuss,
  }) {
    final hasBlocker = blocker != null && blocker.trim().isNotEmpty;
    final seed = hasBlocker
        ? '关于目标「${nudge.goalContent}」卡在「$blocker」，当前抓手是「${nudge.currentFocus}」，我们聊聊怎么破局？'
        : '关于目标「${nudge.goalContent}」，当前抓手是「${nudge.currentFocus}」，近几天有些停滞，我们聊聊怎么推进？';

    onBeforeDiscuss?.call();

    // 讨论就在目标页完成：选中该目标的专属讨论面板并预填草稿，不跳对话页。
    startGoalCoAuthor(ref, seed, goalId: nudge.goalId);
    showGoalCoAuthorToast(context);
  }

  static Future<void> _rollFocus(
    BuildContext context,
    WidgetRef ref,
    GoalCompassNudge nudge, {
    String? initialBlocker,
  }) async {
    final result = await showDialog<({String? focus, String? blocker})>(
      context: context,
      builder: (_) => GoalRollFocusDialog(
        goalId: nudge.goalId,
        goalContent: nudge.goalContent,
        initialFocus: nudge.currentFocus,
        initialBlocker: initialBlocker,
      ),
    );
    if (result == null) return;

    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.updateGoalFocus(
        id: nudge.goalId,
        currentFocus: result.focus,
        blocker: result.blocker,
      );
      await repo.realignCompassNudge(nudge.id);
      ref.invalidate(activeGoalsProvider);
      ref.invalidate(activeCompassNudgeProvider);
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('已更新目标抓手，航向已校准'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('更新抓手失败: $e'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    }
  }

  static Future<void> _dismiss(
    BuildContext context,
    WidgetRef ref,
    GoalCompassNudge nudge,
  ) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.dismissCompassNudge(nudge.id);
      ref.invalidate(activeCompassNudgeProvider);
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('已暂不打扰，7 天内静默退避'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('操作失败: $e'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    }
  }
}

/// 目标工作台顶部专用的航向偏离卡片（GoalListView 内展示）
class GoalCompassNudgeCard extends ConsumerWidget {
  const GoalCompassNudgeCard({super.key, required this.nudge});

  final GoalCompassNudge nudge;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final activeGoals = ref.watch(activeGoalsProvider).value ?? const [];
    final goal = activeGoals.cast<Goal?>().firstWhere(
          (g) => g?.id == nudge.goalId,
          orElse: () => null,
        );
    final blocker = goal?.blocker;

    return Container(
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 14),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: 0.08),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(
          color: AppTheme.accentPrimary.withValues(alpha: 0.25),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.explore_outlined,
                size: 16,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 6),
              Text(
                '航向偏离提醒',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.accentPrimary,
                ),
              ),
              const SizedBox(width: 8),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1.5),
                decoration: BoxDecoration(
                  color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  '停滞 ${nudge.stalledDays} 天',
                  style: TextStyle(
                    fontSize: 10.5,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.accentPrimary,
                  ),
                ),
              ),
              const Spacer(),
              IconButton(
                tooltip: '暂不打扰（7天）',
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.close, size: 15),
                color: AppTheme.textTertiary,
                onPressed: () =>
                    GoalCompassNudgeBanner._dismiss(context, ref, nudge),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '${nudge.planIntent != null ? '计划「${nudge.planIntent}」· 步骤' : '目标'}「${nudge.goalContent}」当前抓手「${nudge.currentFocus}」已连续 ${nudge.stalledDays} 天未推进，近期精力分散在「${nudge.recentTopics}」。',
            style: TextStyle(
              fontSize: 12,
              color: AppTheme.textSecondary,
              height: 1.4,
            ),
          ),
          if (blocker != null && blocker.trim().isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              '已知卡点：$blocker',
              style: TextStyle(
                fontSize: 11.5,
                color: AppTheme.accentPrimary,
                fontWeight: FontWeight.w500,
              ),
            ),
          ],
          const SizedBox(height: 10),
          Row(
            children: [
              FilledButton.tonal(
                style: FilledButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                ),
                onPressed: () => GoalCompassNudgeBanner._focusAndDiscuss(
                  context,
                  ref,
                  nudge,
                  blocker: blocker,
                  onBeforeDiscuss: () => Navigator.of(context).maybePop(),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.chat_bubble_outline, size: 13),
                    const SizedBox(width: 4),
                    Text(
                      blocker != null && blocker.trim().isNotEmpty
                          ? '聚焦攻克此卡点'
                          : '聊聊推进破局',
                      style: const TextStyle(fontSize: 11.5),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              OutlinedButton(
                style: OutlinedButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                ),
                onPressed: () => GoalCompassNudgeBanner._rollFocus(
                  context,
                  ref,
                  nudge,
                  initialBlocker: blocker,
                ),
                child: const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.update_outlined, size: 13),
                    SizedBox(width: 4),
                    Text('滚动更新抓手', style: TextStyle(fontSize: 11.5)),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
