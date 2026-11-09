import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/goal.dart';
import '../providers/goal_provider.dart';
import '../theme/app_theme.dart';
import 'goal_view.dart';
import '../l10n/localization.dart';

/// 航向偏离校准横幅（挂在 MessageArea 状态栏下方，与 ReminderBanner 并列）。
/// 当 3~5 天滑动窗口检测到活跃目标行动停滞且精力分散时呈现。
class GoalCompassNudgeBanner extends ConsumerWidget {
  const GoalCompassNudgeBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final nudge = ref.watch(activeCompassNudgeProvider);
    if (nudge == null) return const SizedBox.shrink();
    if (nudge.assessment != null) return _SemanticCompassCard(nudge: nudge);

    // 尝试从缓存的活跃目标中提取当前困难
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
                      ? context.l10n.compassPlanTitle(nudge.planIntent!)
                      : context.l10n.compassGoalTitle(nudge.goalContent),
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              IconButton(
                tooltip: context.l10n.compassDismissHint,
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
              style: TextStyle(
                fontSize: 12,
                color: AppTheme.textSecondary,
                height: 1.4,
              ),
              children: [
                TextSpan(
                  text: context.l10n.compassActionStalled(
                    nudge.currentFocus,
                    nudge.stalledDays,
                  ),
                ),
                if (blocker != null && blocker.trim().isNotEmpty) ...[
                  TextSpan(text: context.l10n.compassObstacle(blocker)),
                ],
                TextSpan(
                  text: context.l10n.compassRecentTopics(nudge.recentTopics),
                ),
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
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 4,
                  ),
                ),
                onPressed: () =>
                    _focusAndDiscuss(context, ref, nudge, blocker: blocker),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.chat_bubble_outline, size: 13),
                    const SizedBox(width: 4),
                    Text(
                      blocker != null && blocker.trim().isNotEmpty
                          ? context.l10n.compassFocusObstacle
                          : context.l10n.compassDiscussProgress,
                      style: const TextStyle(fontSize: 12),
                    ),
                  ],
                ),
              ),
              OutlinedButton(
                style: OutlinedButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 4,
                  ),
                  side: BorderSide(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.3),
                  ),
                ),
                onPressed: () =>
                    _rollFocus(context, ref, nudge, initialBlocker: blocker),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.update_outlined, size: 13),
                    SizedBox(width: 4),
                    Text(
                      context.l10n.compassUpdateAction,
                      style: TextStyle(fontSize: 12),
                    ),
                  ],
                ),
              ),
              TextButton(
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  foregroundColor: AppTheme.textTertiary,
                ),
                onPressed: () => _dismiss(context, ref, nudge),
                child: Text(
                  context.l10n.compassSnooze,
                  style: TextStyle(fontSize: 11.5),
                ),
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
        ? context.l10n.compassSeedObstacle(
            nudge.goalContent,
            blocker,
            nudge.currentFocus,
          )
        : context.l10n.compassSeedProgress(
            nudge.goalContent,
            nudge.currentFocus,
          );

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
          SnackBar(
            content: Text(context.l10n.compassRealigned),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.errorMessage(context.l10n.edit, e)),
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
          SnackBar(
            content: Text(context.l10n.compassDismissed),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.errorMessage(context.l10n.edit, e)),
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
    if (nudge.assessment != null) return _SemanticCompassCard(nudge: nudge);
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
              Expanded(
                child: Text(
                  context.l10n.compassTitle,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.accentPrimary,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 6,
                  vertical: 1.5,
                ),
                decoration: BoxDecoration(
                  color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  context.l10n.compassStalled(nudge.stalledDays),
                  style: TextStyle(
                    fontSize: 10.5,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.accentPrimary,
                  ),
                ),
              ),
              IconButton(
                tooltip: context.l10n.compassSnoozeHint,
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
            context.l10n.compassCardDetail(
              nudge.planIntent != null
                  ? context.l10n.compassPlanStep(nudge.planIntent!)
                  : context.l10n.goals,
              nudge.goalContent,
              nudge.currentFocus,
              nudge.stalledDays,
              nudge.recentTopics,
            ),
            style: TextStyle(
              fontSize: 12,
              color: AppTheme.textSecondary,
              height: 1.4,
            ),
          ),
          if (blocker != null && blocker.trim().isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              context.l10n.compassObstacleLine(blocker),
              style: TextStyle(
                fontSize: 11.5,
                color: AppTheme.accentPrimary,
                fontWeight: FontWeight.w500,
              ),
            ),
          ],
          const SizedBox(height: 10),
          Wrap(
            spacing: 8,
            runSpacing: 6,
            children: [
              FilledButton.tonal(
                style: FilledButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 4,
                  ),
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
                          ? context.l10n.compassFocusObstacle
                          : context.l10n.compassDiscussProgress,
                      style: const TextStyle(fontSize: 11.5),
                    ),
                  ],
                ),
              ),
              OutlinedButton(
                style: OutlinedButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 4,
                  ),
                ),
                onPressed: () => GoalCompassNudgeBanner._rollFocus(
                  context,
                  ref,
                  nudge,
                  initialBlocker: blocker,
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.update_outlined, size: 13),
                    SizedBox(width: 4),
                    Text(
                      context.l10n.compassRollAction,
                      style: TextStyle(fontSize: 11.5),
                    ),
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

/// Semantic cards use the model's verified question and actual source records.
/// Historical keyword cards retain their read-only presentation above.
class _SemanticCompassCard extends ConsumerStatefulWidget {
  const _SemanticCompassCard({required this.nudge});
  final GoalCompassNudge nudge;
  @override
  ConsumerState<_SemanticCompassCard> createState() =>
      _SemanticCompassCardState();
}

class _SemanticCompassCardState extends ConsumerState<_SemanticCompassCard> {
  bool _saving = false;

  Future<void> _feedback(String kind) async {
    final strings = context.l10n;
    String note = '';
    if (kind == 'still_progressing' || kind == 'changed_direction') {
      final value = await showDialog<String>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(
            kind == 'still_progressing'
                ? strings.compassStillProgressing
                : strings.compassDirectionChanged,
          ),
          content: TextField(
            onChanged: (value) => note = value,
            maxLength: 1000,
            maxLines: 3,
            decoration: InputDecoration(
              hintText: strings.compassCorrectionNote,
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(strings.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, note),
              child: Text(strings.save),
            ),
          ],
        ),
      );
      if (!mounted || value == null) return;
      note = value;
    }
    if (!mounted || _saving) return;
    setState(() => _saving = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.submitCompassFeedback(widget.nudge.id, kind, note: note);
      if (!mounted) return;
      if (kind == 'discuss') {
        final assessment = widget.nudge.assessment!;
        final seed =
            '${strings.compassReviewSeed} ${assessment.reason}\n${assessment.question}\n${assessment.recommendations.join('\n')}';
        startGoalCoAuthor(ref, seed, goalId: widget.nudge.goalId);
        showGoalCoAuthorToast(context);
      } else {
        ref.invalidate(activeCompassNudgeProvider);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              kind == 'snooze'
                  ? strings.compassDismissed
                  : strings.compassFeedbackSaved,
            ),
          ),
        );
      }
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.errorMessage(strings.edit, error))),
        );
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  void _evidence() {
    final strings = context.l10n;
    final assessment = widget.nudge.assessment!;
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(strings.compassEvidence),
        content: SizedBox(
          width: 560,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(strings.compassEvidenceNotice),
                for (final evidence in assessment.evidence) ...[
                  const SizedBox(height: 16),
                  Text(
                    MaterialLocalizations.of(context).formatFullDate(
                      DateTime.parse(evidence.occurredAt).toLocal(),
                    ),
                    style: Theme.of(context).textTheme.labelMedium,
                  ),
                  const SizedBox(height: 4),
                  SelectableText(
                    evidence.quote,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 6),
                  SelectableText(evidence.excerpt),
                ],
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: Text(strings.close),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final nudge = widget.nudge;
    final assessment = nudge.assessment!;
    final strings = context.l10n;
    return Material(
      color: AppTheme.accentPrimary.withValues(alpha: 0.06),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              strings.compassGoalTitle(nudge.goalContent),
              style: Theme.of(context).textTheme.labelLarge,
            ),
            const SizedBox(height: 6),
            Text(
              assessment.question.isEmpty
                  ? strings.compassGentleCheck
                  : assessment.question,
            ),
            const SizedBox(height: 4),
            Text(
              assessment.reason,
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 6),
            Wrap(
              spacing: 6,
              runSpacing: 2,
              children: [
                FilledButton.tonal(
                  onPressed: _saving ? null : () => _feedback('discuss'),
                  child: Text(strings.compassReview),
                ),
                TextButton(
                  onPressed: _evidence,
                  child: Text(strings.compassEvidence),
                ),
                TextButton(
                  onPressed: _saving
                      ? null
                      : () => _feedback('still_progressing'),
                  child: Text(strings.compassStillProgressing),
                ),
                TextButton(
                  onPressed: _saving
                      ? null
                      : () => _feedback('changed_direction'),
                  child: Text(strings.compassDirectionChanged),
                ),
                TextButton(
                  onPressed: _saving ? null : () => _feedback('snooze'),
                  child: Text(strings.compassSnooze),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
