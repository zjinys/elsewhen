part of '../message_area.dart';

/// 主对话里的知识洞见浮现卡（提案 2026-11-05-surfacing-digest-insights §4）：
/// 后台提炼的高价值洞察/新习惯/重复固定成本在主对话流轻量浮现，卡片底部
/// 提供四重闭环出口——采纳为规则 / 开启习惯问候 / 设为待办 / 知道了。
///
/// 数据两条线：
/// - 卡片消息 `kind='digest_insight'`、`ref='surfacing:<id>'`：随消息同步，
///   正文自含卡片文本，副设备没有本机台账行时仍可完整展示并动作
///   （动作用消息 id 发起，后端按正文解析重建台账；规则/待办作为一等
///   公民各自同步）；
/// - 本机台账 `digestInsightSurfacingsProvider`：状态（已采纳/已了解/已搁置）
///   与来源页 kind 判断（「开启习惯问候」仅对活跃 habit 页开放）。
class DigestInsightBubble extends ConsumerStatefulWidget {
  final Message message;

  const DigestInsightBubble({super.key, required this.message});

  @override
  ConsumerState<DigestInsightBubble> createState() =>
      _DigestInsightBubbleState();
}

class _DigestInsightBubbleState extends ConsumerState<DigestInsightBubble> {
  bool _busy = false;

  static const _prefix = '💡 认知洞察｜';
  static const _actionMark = '\n\n建议行动：';
  static const _sourceMark = '\n\n来源：';

  String get _surfacingId {
    final reference = widget.message.messageRef ?? '';
    return reference.startsWith('surfacing:') ? reference.substring(10) : '';
  }

  /// 从卡片正文解析（副设备/台账未及时刷新的兜底渲染与来源链接）。
  ({String title, String body, String? action, String slug}) _parsedCard() {
    final content = widget.message.content.trim();
    final rest = content.startsWith(_prefix)
        ? content.substring(_prefix.length)
        : content;
    final doubleBreak = rest.indexOf('\n\n');
    if (doubleBreak < 0) {
      return (title: rest, body: '', action: null, slug: '');
    }
    final title = rest.substring(0, doubleBreak).trim();
    var body = rest.substring(doubleBreak + 2);
    var slug = '';
    final sourceIndex = body.lastIndexOf(_sourceMark);
    if (sourceIndex >= 0) {
      final source = body.substring(sourceIndex + _sourceMark.length).trim();
      if (source.startsWith('[[') && source.endsWith(']]')) {
        slug = source.substring(2, source.length - 2).trim();
      }
      body = body.substring(0, sourceIndex);
    }
    String? action;
    final actionIndex = body.lastIndexOf(_actionMark);
    if (actionIndex >= 0) {
      final value = body.substring(actionIndex + _actionMark.length).trim();
      if (value.isNotEmpty) action = value;
      body = body.substring(0, actionIndex);
    }
    return (title: title, body: body.trim(), action: action, slug: slug);
  }

  @override
  Widget build(BuildContext context) {
    final surfacings =
        ref.watch(digestInsightSurfacingsProvider).value ?? const [];
    DigestInsightSurfacingDto? surfacing;
    for (final candidate in surfacings) {
      if (candidate.id == _surfacingId) {
        surfacing = candidate;
        break;
      }
    }
    final parsed = _parsedCard();
    final status = surfacing?.status;
    final actionable =
        status == null || status == 'pending' || status == 'surfaced';
    final title = surfacing?.title ?? parsed.title;
    final observation = surfacing?.observation ?? parsed.body;
    final action = surfacing?.actionSuggestion ?? parsed.action;
    final pageSlug = surfacing?.pageSlug ?? parsed.slug;
    final habitEligible =
        surfacing != null &&
        surfacing.pageKind == 'habit' &&
        surfacing.pageActive;

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
              Icons.lightbulb_outline,
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
                    color: actionable
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
                          context.l10n.cognitiveInsight,
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            color: AppTheme.accentPrimary,
                          ),
                        ),
                        Text(
                          ' • ${DateFormat.Hm(context.l10n.localeName).format(widget.message.createdAt)}',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(
                      title,
                      style: TextStyle(
                        fontSize: 15.5,
                        fontWeight: FontWeight.w600,
                        height: 1.45,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                    if (observation.isNotEmpty) ...[
                      const SizedBox(height: 6),
                      Text(
                        observation,
                        style: TextStyle(
                          fontSize: 13.5,
                          height: 1.55,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                    ],
                    if (action != null) ...[
                      const SizedBox(height: 6),
                      Text(
                        context.l10n.insightSuggestion(action),
                        style: TextStyle(
                          fontSize: 13.5,
                          height: 1.5,
                          color: AppTheme.textPrimary,
                        ),
                      ),
                    ],
                    if (pageSlug.isNotEmpty) ...[
                      const SizedBox(height: 6),
                      InkWell(
                        onTap: () =>
                            KnowledgePageDialog.show(context, slug: pageSlug),
                        borderRadius: BorderRadius.circular(4),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(vertical: 2),
                          child: Text(
                            context.l10n.insightSource(pageSlug),
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.accentPrimary,
                              decoration: TextDecoration.underline,
                              decorationColor: AppTheme.accentMuted,
                            ),
                          ),
                        ),
                      ),
                    ],
                    const SizedBox(height: 12),
                    if (_busy)
                      LinearProgressIndicator(
                        minHeight: 2,
                        color: AppTheme.accentPrimary,
                        backgroundColor: AppTheme.surface3,
                      )
                    else if (actionable)
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        children: [
                          FilledButton(
                            onPressed: () => _act('rule'),
                            style: FilledButton.styleFrom(
                              backgroundColor: AppTheme.accentPrimary,
                              foregroundColor: AppTheme.surface0,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: Text(context.l10n.adoptAsRule),
                          ),
                          if (habitEligible)
                            OutlinedButton(
                              onPressed: () => _enableHabitNudge(pageSlug),
                              style: OutlinedButton.styleFrom(
                                foregroundColor: AppTheme.textSecondary,
                                side: BorderSide(color: AppTheme.surface3),
                                visualDensity: VisualDensity.compact,
                              ),
                              child: Text(context.l10n.enableCheckins),
                            ),
                          OutlinedButton(
                            onPressed: () => _act('todo'),
                            style: OutlinedButton.styleFrom(
                              foregroundColor: AppTheme.textSecondary,
                              side: BorderSide(color: AppTheme.surface3),
                              visualDensity: VisualDensity.compact,
                            ),
                            child: Text(context.l10n.makeTodo),
                          ),
                          TextButton(
                            onPressed: () => _act('dismiss'),
                            style: TextButton.styleFrom(
                              foregroundColor: AppTheme.textTertiary,
                              visualDensity: VisualDensity.compact,
                            ),
                            child: Text(context.l10n.acknowledge),
                          ),
                        ],
                      )
                    else
                      Text(
                        _statusLabel(status, surfacing?.actionType),
                        style: TextStyle(
                          fontSize: 12,
                          color: AppTheme.textTertiary,
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

  String _statusLabel(String? status, String? actionType) {
    if (status == 'acted') {
      return switch (actionType) {
        'rule' => context.l10n.insightRuleApplied,
        'habit_nudge' => context.l10n.insightHabitEnabled,
        'todo' => context.l10n.insightTodoAdded,
        _ => context.l10n.insightHandled,
      };
    }
    if (status == 'dismissed') return context.l10n.insightDismissed;
    if (status == 'expired') return context.l10n.insightExpired;
    return '';
  }

  Future<void> _act(String action) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.actOnDigestInsight(widget.message.id, action);
      if (!mounted) return;
      final label = switch (action) {
        'rule' => context.l10n.personalRuleAdopted,
        'todo' => context.l10n.addedToTodoList,
        'dismiss' => context.l10n.insightDismissed,
        _ => context.l10n.recorded,
      };
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(label), behavior: SnackBarBehavior.floating),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.errorMessage(context.l10n.edit, e)),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 「开启习惯问候」：复用习惯页的推断/确认对话框（用户在对话框里确认
  /// 保存后才落库），返回 true 后再记录闭环状态。
  Future<void> _enableHabitNudge(String pageSlug) async {
    if (_busy || pageSlug.isEmpty) return;
    setState(() => _busy = true);
    try {
      final page = await ref.read(wikiPageProvider(pageSlug).future);
      if (!mounted) return;
      if (page == null) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.l10n.pageNotFoundDeleted),
            behavior: SnackBarBehavior.floating,
          ),
        );
        return;
      }
      final changed = await showHabitNudgeDialog(context, page: page);
      if (!mounted || changed != true) return;
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.actOnDigestInsight(widget.message.id, 'habit_nudge');
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.l10n.habitCheckinsEnabled),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.errorMessage(context.l10n.edit, e)),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }
}
