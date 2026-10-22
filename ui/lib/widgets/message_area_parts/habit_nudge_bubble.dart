part of '../message_area.dart';

/// 主对话里的习惯问候卡：问题 + 按习惯定制的完成/未完成按钮。
///
/// 数据三条线：
/// - 消息本身带 `kind='habit_nudge'` 与 `ref='<nudge_id>:<date>'`；
/// - 计划来自 `habitNudgePlanProvider`（按钮文案/记录句）；
/// - 本机状态来自 `habitNudgeStatesProvider`（已回答/已推迟/已补问）。
class HabitNudgeBubble extends ConsumerStatefulWidget {
  final Message message;

  const HabitNudgeBubble({super.key, required this.message});

  @override
  ConsumerState<HabitNudgeBubble> createState() => _HabitNudgeBubbleState();
}

class _HabitNudgeBubbleState extends ConsumerState<HabitNudgeBubble> {
  bool _busy = false;

  @override
  Widget build(BuildContext context) {
    final message = widget.message;
    final referenceParts = message.messageRef?.split(':') ?? const [];
    final nudgeId = referenceParts.isEmpty ? '' : referenceParts.first;
    final referenceDate = referenceParts.length > 1 ? referenceParts[1] : null;
    final planAsync = nudgeId.isEmpty
        ? null
        : ref.watch(habitNudgePlanProvider(nudgeId));
    final plan = planAsync?.value;
    // 计划已经关闭（不是「还在加载」）时，历史卡片不再提供回答入口。
    final planMissing =
        planAsync != null &&
        !planAsync.isLoading &&
        planAsync.hasValue &&
        plan == null;
    final states = ref.watch(habitNudgeStatesProvider).value ?? const [];
    HabitNudgeStateDto? state;
    for (final candidate in states) {
      if (candidate.nudgeId == nudgeId &&
          (referenceDate == null || candidate.localDate == referenceDate)) {
        state = candidate;
        break;
      }
    }
    final response = state?.response;
    final answered = response == 'done' || response == 'missed';
    final dismissed = response == 'dismissed';
    final snoozed = !answered && !dismissed && state?.snoozeUntil != null;
    final superseded =
        !answered &&
        !dismissed &&
        !snoozed &&
        state?.messageId != null &&
        state!.messageId != message.id;
    // 昨天的问候只作为历史记录，不能今天补点（补点会写错日期的事件）。
    final today = DateFormat('yyyy-MM-dd').format(DateTime.now());
    final stale = referenceDate != null && referenceDate != today;
    final active =
        !stale &&
        !planMissing &&
        !answered &&
        !dismissed &&
        !snoozed &&
        !superseded;

    final doneLabel = plan?.doneLabel ?? '完成了';
    final missedLabel = plan?.missedLabel ?? '还没';
    final doneText = plan?.doneText ?? '今天完成了';
    final missedText = plan?.missedText ?? '今天还没完成';

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 30,
            height: 30,
            decoration: BoxDecoration(
              color: AppTheme.accentPrimary.withValues(alpha: 0.12),
              shape: BoxShape.circle,
              border: Border.all(
                color: AppTheme.accentPrimary.withValues(alpha: 0.25),
              ),
            ),
            child: Icon(
              Icons.notifications_active_outlined,
              size: 15,
              color: AppTheme.accentPrimary,
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          Flexible(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 820),
              child: Container(
                padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
                decoration: BoxDecoration(
                  color: AppTheme.surface1,
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  border: Border.all(
                    color: active
                        ? AppTheme.accentPrimary.withValues(alpha: 0.32)
                        : AppTheme.surface3.withValues(alpha: 0.7),
                  ),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Text(
                          '习惯问候',
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            color: AppTheme.accentPrimary,
                          ),
                        ),
                        Text(
                          ' • ${DateFormat('HH:mm').format(message.createdAt)}',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(
                      message.content,
                      style: TextStyle(
                        fontSize: 15.5,
                        height: 1.55,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                    const SizedBox(height: 12),
                    if (active) ...[
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        children: [
                          FilledButton(
                            onPressed: _busy
                                ? null
                                : () => _answer(done: true, text: doneText),
                            style: FilledButton.styleFrom(
                              backgroundColor: AppTheme.accentPrimary,
                              foregroundColor: AppTheme.surface0,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: Text(doneLabel),
                          ),
                          OutlinedButton(
                            onPressed: _busy
                                ? null
                                : () => _answer(done: false, text: missedText),
                            style: OutlinedButton.styleFrom(
                              foregroundColor: AppTheme.textSecondary,
                              side: BorderSide(color: AppTheme.surface3),
                              visualDensity: VisualDensity.compact,
                            ),
                            child: Text(missedLabel),
                          ),
                          TextButton(
                            onPressed: _busy ? null : _snooze,
                            style: TextButton.styleFrom(
                              foregroundColor: AppTheme.textTertiary,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: const Text('晚点再问'),
                          ),
                          TextButton(
                            onPressed: _busy ? null : _dismiss,
                            style: TextButton.styleFrom(
                              foregroundColor: AppTheme.textTertiary,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: const Text('今天不问'),
                          ),
                        ],
                      ),
                      if (_busy) ...[
                        const SizedBox(height: 8),
                        LinearProgressIndicator(
                          minHeight: 2,
                          color: AppTheme.accentPrimary,
                          backgroundColor: AppTheme.surface3,
                        ),
                      ],
                    ] else
                      Text(
                        _statusText(
                          response: response,
                          snoozed: snoozed,
                          superseded: superseded,
                          stale: stale,
                          planMissing: planMissing,
                          doneText: doneText,
                          missedText: missedText,
                        ),
                        style: TextStyle(
                          fontSize: 12,
                          color: answered
                              ? AppTheme.success
                              : AppTheme.textTertiary,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  String _statusText({
    required String? response,
    required bool snoozed,
    required bool superseded,
    required bool stale,
    required bool planMissing,
    required String doneText,
    required String missedText,
  }) {
    if (response == 'done') return '已记录：$doneText';
    if (response == 'missed') return '已记录：$missedText';
    if (response == 'dismissed') return '今天先不问了';
    if (snoozed) return '已推迟，稍后再问一次';
    if (superseded) return '已再次提醒';
    if (stale) return '这条问候已过期';
    if (planMissing) return '这条问候的计划已关闭';
    return '';
  }

  Future<void> _answer({required bool done, required String text}) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final result = await repo.answerHabitNudge(widget.message.id, done);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            done && result.eventCreated ? '已记录，并生成一条事件' : '已记录：$text',
          ),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('记录失败：${e.toString().replaceFirst('Exception: ', '')}'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _snooze() async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.snoozeHabitNudge(widget.message.id);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('好，1 小时后再问一次'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('推迟失败：${e.toString().replaceFirst('Exception: ', '')}'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _dismiss() async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.dismissHabitNudge(widget.message.id);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('操作失败：${e.toString().replaceFirst('Exception: ', '')}'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }
}
