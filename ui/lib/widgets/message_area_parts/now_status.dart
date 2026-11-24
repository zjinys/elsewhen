part of '../message_area.dart';

//! 「今天」状态图标按钮、状态浮层与临时提示气泡。

class NowStatusIconButton extends StatelessWidget {
  final String tooltip;
  final IconData icon;
  final String label;
  final Color? color;
  final VoidCallback? onPressed;

  const NowStatusIconButton({
    super.key,
    required this.tooltip,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.color,
  });

  @override
  Widget build(BuildContext context) {
    final effective = color ?? AppTheme.textSecondary;
    return Tooltip(
      message: tooltip,
      child: InkWell(
        onTap: onPressed,
        borderRadius: BorderRadius.circular(6),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 15, color: effective),
              const SizedBox(width: 4),
              Text(
                label,
                style: TextStyle(
                  fontSize: TypographyTokens.secondary,
                  color: effective,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// 会话顶部状态条。统计项（待办 / 话题 / provider / token / 待入库草稿）
/// 一律在**这里**订阅，而不是在消息列表所在的 State 里 watch：它们与消息
/// 内容无关，放在父级 build 里会让每次 todo/wiki/token 变化都重建整份消息
/// 列表（P13）。
///
/// 移动端不显示：入口已全部移到标题栏「今天」行，这里整栏隐藏。
class _NowStatus extends ConsumerWidget {
  final String? conversationId;
  final List<Message> messages;
  final bool showSearch;
  final VoidCallback? onOpenTodos;
  final VoidCallback? onOpenGoals;
  final VoidCallback? onCreateTopic;
  final VoidCallback? onOpenPageMarks;
  final VoidCallback? onToggleSearch;
  final VoidCallback? onCopyAll;
  const _NowStatus({
    this.conversationId,
    required this.messages,
    this.showSearch = false,
    this.onOpenPageMarks,
    this.onOpenTodos,
    this.onOpenGoals,
    this.onCreateTopic,
    this.onToggleSearch,
    this.onCopyAll,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // 移动端：入口已上移到标题栏「今天」行，整栏不再显示。
    final isMobilePlatform = !kIsWeb && (Platform.isAndroid || Platform.isIOS);
    if (isMobilePlatform) return const SizedBox.shrink();

    final openTodos = ref
        .watch(todosProvider)
        .value
        ?.where((todo) => !todo.isDone)
        .length;
    final activeGoals = ref.watch(activeGoalsProvider).value ?? const [];
    // 目标为空时入口仍必须显示：目标功能没有独立一级 tab，这里是唯一入口，
    // 显示为「目标 0」会让人以为无路可走。
    final goalLabel = context.l10n.goalSummaryCount(activeGoals.length);
    final dark = AppTheme.brightness == Brightness.dark;
    return GlassPanel(
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 2),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
      borderRadius: 12,
      elevation: 1,
      fillColor: dark ? AppTheme.surface1 : AppTheme.surface1,
      showHighlight: true,
      child: LayoutBuilder(
        builder: (context, constraints) {
          // 宽形态一行的自然宽度实测约 500px（今天+目标+待办+审阅+创建主题+右侧操作按钮）；
          // 断点取 640 留出安全余量。
          final compact = constraints.maxWidth < 640;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.today_outlined,
                    size: DesignTokens.iconMicro,
                    color: AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: 8),
                  Text(
                    context.l10n.today,
                    style: TextStyle(
                      fontSize: TypographyTokens.secondary,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                  const SizedBox(width: 6),
                  if (!compact) ...[
                    // 目标入口在宽屏才与草稿、待办并排：窄屏第一行放不下三个带文案的
                    // 按钮（实测 360px 下会溢出 51px），窄屏改由第二行承载。
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenGoals,
                      icon: Icon(
                        Icons.flag_outlined,
                        size: DesignTokens.iconMicro,
                        color: activeGoals.isEmpty
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                      ),
                      label: Text(
                        goalLabel,
                        style: TextStyle(
                          color: activeGoals.isEmpty
                              ? AppTheme.accentPrimary
                              : AppTheme.textSecondary,
                        ),
                      ),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                        textStyle: const TextStyle(
                          fontSize: TypographyTokens.secondary,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenTodos,
                      icon: const Icon(Icons.check_circle_outlined, size: 16),
                      label: Text(context.l10n.todoCountItems(openTodos ?? 0)),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                        textStyle: const TextStyle(
                          fontSize: TypographyTokens.secondary,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                    if (conversationId != null) ...[
                      const SizedBox(width: 6),
                      ChatProposalReviewButton(conversationId: conversationId),
                    ],
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onCreateTopic,
                      icon: const Icon(Icons.note_add_outlined, size: 16),
                      label: Text(context.l10n.createTopic),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                        textStyle: const TextStyle(
                          fontSize: TypographyTokens.secondary,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                    const Spacer(),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (onCopyAll != null)
                          IconButton(
                            tooltip: context.l10n.copyAll,
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.copy_all_rounded, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onCopyAll,
                          ),
                        if (onOpenPageMarks != null)
                          IconButton(
                            tooltip: context.l10n.flipPast,
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.auto_stories, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onOpenPageMarks,
                          ),
                        IconButton(
                          tooltip: showSearch
                              ? context.l10n.hideSearch
                              : context.l10n.searchAndDate,
                          visualDensity: VisualDensity.compact,
                          icon: Icon(
                            showSearch ? Icons.search_off : Icons.search,
                            size: DesignTokens.icon,
                          ),
                          color: showSearch
                              ? AppTheme.accentPrimary
                              : AppTheme.textSecondary,
                          onPressed: onToggleSearch,
                        ),
                      ],
                    ),
                  ] else ...[
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenTodos,
                      icon: const Icon(Icons.check_circle_outlined, size: 16),
                      label: Text(context.l10n.todoCountItems(openTodos ?? 0)),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                        textStyle: const TextStyle(
                          fontSize: TypographyTokens.secondary,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                  ],
                ],
              ),
              if (compact)
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        NowStatusIconButton(
                          tooltip: context.l10n.goals,
                          icon: Icons.flag_outlined,
                          label: goalLabel,
                          color: activeGoals.isEmpty
                              ? AppTheme.accentPrimary
                              : AppTheme.textTertiary,
                          onPressed: onOpenGoals,
                        ),
                        const SizedBox(width: 4),
                        NowStatusIconButton(
                          tooltip: context.l10n.createTopic,
                          icon: Icons.note_add_outlined,
                          label: context.l10n.topics,
                          color: AppTheme.textTertiary,
                          onPressed: onCreateTopic,
                        ),
                        if (conversationId != null) ...[
                          const SizedBox(width: 4),
                          ChatProposalReviewButton(
                            conversationId: conversationId,
                            compact: true,
                          ),
                        ],
                        if (onCopyAll != null) ...[
                          const SizedBox(width: 4),
                          IconButton(
                            tooltip: context.l10n.copyAll,
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.copy_all_rounded, size: 16),
                            color: AppTheme.textSecondary,
                            onPressed: onCopyAll,
                          ),
                        ],
                        if (onOpenPageMarks != null)
                          IconButton(
                            tooltip: context.l10n.flipPast,
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.auto_stories, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onOpenPageMarks,
                          ),
                        IconButton(
                          tooltip: showSearch
                              ? context.l10n.hideSearch
                              : context.l10n.searchAndDate,
                          visualDensity: VisualDensity.compact,
                          icon: Icon(
                            showSearch ? Icons.search_off : Icons.search,
                            size: DesignTokens.iconMicro,
                          ),
                          color: showSearch
                              ? AppTheme.accentPrimary
                              : AppTheme.textSecondary,
                          onPressed: onToggleSearch,
                        ),
                      ],
                    ),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}

class _NoticeBubble extends StatelessWidget {
  final String text;

  const _NoticeBubble({required this.text});

  @override
  Widget build(BuildContext context) {
    const danger = Color(0xFFB91C1C);
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4, right: 48),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          _buildAvatar(isUser: false),
          const SizedBox(width: AppTheme.space3),
          Flexible(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Padding(
                  padding: const EdgeInsets.only(
                    left: AppTheme.space2,
                    right: AppTheme.space2,
                    bottom: AppTheme.space1,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.error_outline_rounded,
                        size: DesignTokens.iconMicro,
                        color: danger.withValues(alpha: 0.85),
                      ),
                      const SizedBox(width: 4),
                      Text(
                        context.l10n.systemNotice,
                        style: TextStyle(
                          color: danger.withValues(alpha: 0.85),
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ),
                ),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space3,
                    vertical: AppTheme.space3,
                  ),
                  decoration: BoxDecoration(
                    color: danger.withValues(alpha: 0.08),
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    border: Border.all(
                      color: danger.withValues(alpha: 0.4),
                      width: 1,
                    ),
                  ),
                  child: resolveNotice(text, context.l10n) != null
                      ? AppErrorMessage(
                          action: context.l10n.aiReply,
                          error: text,
                        )
                      : SelectableText(
                          text,
                          style: TextStyle(
                            color: AppTheme.textSecondary,
                            fontSize: 13,
                            height: 1.5,
                          ),
                        ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildAvatar({required bool isUser}) {
    return Container(
      width: 32,
      height: 32,
      decoration: BoxDecoration(
        color: const Color(0xFFB91C1C).withValues(alpha: 0.15),
        shape: BoxShape.circle,
      ),
      child: Icon(
        Icons.error_outline_rounded,
        size: DesignTokens.iconMicro,
        color: const Color(0xFFB91C1C).withValues(alpha: 0.85),
      ),
    );
  }
}
