part of '../message_area.dart';

/// 主对话里的习惯问候卡：问题 + 按习惯定制的完成/未完成按钮。
///
/// 数据三条线：
/// - 消息本身带 `kind='habit_nudge'` 与 `ref='<nudge_id>:<date>'`；
/// - 计划来自 `habitNudgePlanProvider`（按钮文案/记录句）；
/// - 本机状态来自 `habitNudgeStatesProvider`（已回答/已推迟/已补问）；
/// - 已同步的回答来自同一 ref 的 `habit_checkin` 消息（主设备回答后，
///   其他设备本地没有 fire 行，只能靠这条消息判断已回答）。
class HabitNudgeBubble extends ConsumerStatefulWidget {
  final Message message;
  final Message? checkin;
  final bool supersededByNewer;

  const HabitNudgeBubble({
    super.key,
    required this.message,
    this.checkin,
    this.supersededByNewer = false,
  });

  @override
  ConsumerState<HabitNudgeBubble> createState() => _HabitNudgeBubbleState();
}

class _HabitNudgeBubbleState extends ConsumerState<HabitNudgeBubble> {
  bool _busy = false;

  /// 建议卡被处理后的提示（「改时间」已保存 / 「暂停问候」已落库）。
  /// 本地状态即可：建议卡每天只出一张，处理后当天不会再触发。
  String? _acted;

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
    // 主设备回答后，回答消息会同步到其他设备；那些设备没有本机 fire 行，
    // 因此 checkin 消息是跨设备判断「已回答」的权威来源。
    final checkinText = widget.checkin?.content;
    final answered =
        checkinText != null || response == 'done' || response == 'missed';
    final dismissed = checkinText == null && response == 'dismissed';
    final snoozed =
        checkinText == null &&
        !answered &&
        !dismissed &&
        state?.snoozeUntil != null;
    final superseded =
        checkinText == null &&
        !answered &&
        !dismissed &&
        !snoozed &&
        (widget.supersededByNewer ||
            (state?.messageId != null && state!.messageId != message.id));
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
    // 连续忽略后的确定性建议卡（kind 由后端写入，文案不含回答按钮）。
    final isSuggestion = message.kind == 'habit_nudge_suggest';

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
                    if (isSuggestion) ...[
                      // 建议卡：用户确认才落库，不自动改计划。暂停后 plan.enabled
                      // 变 false（revision 触发重读），卡片直接展示已暂停。
                      if (_acted == null &&
                          !stale &&
                          (plan == null || plan.enabled))
                        Wrap(
                          spacing: 8,
                          runSpacing: 8,
                          crossAxisAlignment: WrapCrossAlignment.center,
                          children: [
                            FilledButton(
                              onPressed: _busy ? null : _editTime,
                              style: FilledButton.styleFrom(
                                backgroundColor: AppTheme.accentPrimary,
                                foregroundColor: AppTheme.surface0,
                                visualDensity: VisualDensity.compact,
                              ),
                              child: const Text('改时间'),
                            ),
                            OutlinedButton(
                              onPressed: _busy ? null : _pause,
                              style: OutlinedButton.styleFrom(
                                foregroundColor: AppTheme.textSecondary,
                                side: BorderSide(color: AppTheme.surface3),
                                visualDensity: VisualDensity.compact,
                              ),
                              child: const Text('暂停问候'),
                            ),
                          ],
                        )
                      else
                        Text(
                          _acted ?? '已暂停，可在设置页恢复',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                    ] else if (active) ...[
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
                          // 只有写过本机 fire 行（主设备）才能推迟；加入设备
                          // 只同步到问候消息，没有本机 fire，点击会失败。
                          if (state != null)
                            TextButton(
                              onPressed: _busy ? null : _snooze,
                              style: TextButton.styleFrom(
                                foregroundColor: AppTheme.textTertiary,
                                visualDensity: VisualDensity.compact,
                              ),
                              child: const Text('晚点再问'),
                            ),
                          // 聊聊：预填主对话输入框，不自动发送。
                          TextButton(
                            onPressed: _busy
                                ? null
                                : () => _chatAbout('$missedText，因为'),
                            style: TextButton.styleFrom(
                              foregroundColor: AppTheme.textTertiary,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: const Text('聊聊'),
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
                    // 记了「没做」之后，提供「聊聊原因」入口（同样是预填不发送）。
                    if (response == 'missed' && !stale)
                      Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            '已记录：${checkinText ?? missedText}',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textTertiary,
                            ),
                          ),
                          TextButton(
                            onPressed: () => _chatAbout('$missedText，因为'),
                            style: TextButton.styleFrom(
                              foregroundColor: AppTheme.accentPrimary,
                              visualDensity: VisualDensity.compact,
                              padding: EdgeInsets.zero,
                            ),
                            child: const Text('聊聊原因'),
                          ),
                        ],
                      )
                    else
                      Text(
                        checkinText != null
                            ? '已记录：$checkinText'
                            : _statusText(
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

  /// 「聊聊」：把记录句预填进主对话输入框（追加「，因为」留待用户补全），
  /// 由 _MessageInput 监听填入，不自动发送。
  void _chatAbout(String seed) {
    ref
        .read(habitChatDraftProvider(widget.message.conversationId).notifier)
        .set(seed);
  }

  /// 建议卡「改时间」：按 slug 找到习惯页，复用「定期问我」设置对话框；
  /// 用户在对话框里确认保存后才落库（对话框自身负责写库与广播）。
  Future<void> _editTime() async {
    if (_busy) return;
    final nudgeId = widget.message.messageRef?.split(':').first ?? '';
    if (nudgeId.isEmpty) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final plan = await repo.getHabitNudge(nudgeId);
      if (!mounted || plan == null) return;
      final page = await ref.read(wikiPageProvider(plan.pageSlug).future);
      if (!mounted || page == null) return;
      final changed = await showHabitNudgeDialog(context, page: page);
      if (!mounted) return;
      if (changed == true) {
        setState(() => _acted = '已按新的时间继续提醒');
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('定期问候已更新'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 建议卡「暂停问候」：保留全部配置只把 enabled 置 false，用户可随时
  /// 在设置页恢复；这是确定性操作，不经 AI。
  Future<void> _pause() async {
    if (_busy) return;
    final nudgeId = widget.message.messageRef?.split(':').first ?? '';
    if (nudgeId.isEmpty) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final plan = await repo.getHabitNudge(nudgeId);
      if (!mounted || plan == null) return;
      await repo.saveHabitNudge(
        HabitNudgeInputDto(
          pageSlug: plan.pageSlug,
          question: plan.question,
          doneLabel: plan.doneLabel,
          doneText: plan.doneText,
          missedLabel: plan.missedLabel,
          missedText: plan.missedText,
          weekdays: plan.weekdays,
          localTime: plan.localTime,
          enabled: false,
        ),
      );
      if (!mounted) return;
      setState(() => _acted = '已暂停，可在设置页恢复');
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
