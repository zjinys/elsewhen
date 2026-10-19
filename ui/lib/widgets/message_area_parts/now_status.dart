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
              Text(label, style: TextStyle(fontSize: 12, color: effective)),
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
  final VoidCallback? onOpenDrafts;
  final VoidCallback? onOpenTodos;
  final VoidCallback? onOpenGoals;
  final VoidCallback? onOpenNotepad;
  final VoidCallback? onOpenPageMarks;
  final VoidCallback? onToggleSearch;
  final VoidCallback? onCopyAll;
  const _NowStatus({
    this.conversationId,
    required this.messages,
    this.showSearch = false,
    this.onOpenDrafts,
    this.onOpenPageMarks,
    this.onOpenTodos,
    this.onOpenGoals,
    this.onOpenNotepad,
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
    final activeProvider = ref.watch(activeAiProviderProvider).value;
    final tokenUsage = ref.watch(todayTokenUsageProvider).value;
    final activeGoals = ref.watch(activeGoalsProvider).value ?? const [];
    // 目标为空时入口仍必须显示：目标功能没有独立一级 tab，这里是唯一入口，
    // 显示为「目标 0」会让人以为无路可走。
    final goalLabel = activeGoals.isEmpty ? '目标' : '目标${activeGoals.length}';
    final draftCount = (ref.watch(pendingActionsProvider).value ?? const [])
        .where((action) => action.action == 'save_knowledge_draft')
        .length;

    final today = DateTime.now();
    final count = messages
        .where(
          (m) =>
              m.isUser &&
              m.createdAt.year == today.year &&
              m.createdAt.month == today.month &&
              m.createdAt.day == today.day,
        )
        .length;
    final summary = '对话$count次';
    return Container(
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // 宽形态一行的自然宽度实测约 950–1000px（今天+草稿+目标+待办+审阅
          // +对话统计+Token+provider+2 图标按钮）；断点取 1020 留出余量。
          // 旧值 780 在图标轨（700–1024 窗口）区间会溢出 RenderFlex。
          final compact = constraints.maxWidth < 1020;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.today_outlined,
                    size: 16,
                    color: AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: 8),
                  Text(
                    '今天',
                    style: TextStyle(
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                  const SizedBox(width: 6),
                  TextButton.icon(
                    onPressed: onOpenDrafts,
                    icon: const Icon(Icons.drafts_outlined, size: 16),
                    label: Text('草稿$draftCount份'),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                    ),
                  ),
                  if (!compact) ...[
                    // 目标入口在宽屏才与草稿、待办并排：窄屏第一行放不下三个带文案的
                    // 按钮（实测 360px 下会溢出 51px），窄屏改由第二行承载。
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenGoals,
                      icon: Icon(
                        Icons.flag_outlined,
                        size: 16,
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
                      ),
                    ),
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenTodos,
                      icon: const Icon(Icons.check_circle_outlined, size: 16),
                      label: Text('待办${openTodos ?? 0}项'),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                      ),
                    ),
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenNotepad,
                      icon: const Icon(Icons.note_alt_outlined, size: 16),
                      label: const Text('记事'),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                      ),
                    ),
                    if (conversationId != null) ...[
                      const SizedBox(width: 6),
                      ChatProposalReviewButton(conversationId: conversationId),
                    ],
                    const Spacer(),
                    // 防跳动：会出现/消失的 widget 全部排在左边，常驻按钮钉死
                    // 最右。provider 名加载异步、最长、最易有/无 → 挪到最左，
                    // 它的闪烁只影响自己左边（没人），不再推挤右侧所有图标。
                    if (activeProvider != null) ...[
                      Icon(
                        Icons.smart_toy_outlined,
                        size: 14,
                        color: AppTheme.textTertiary,
                      ),
                      const SizedBox(width: 5),
                      ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 120),
                        child: Text(
                          activeProvider,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ),
                      const SizedBox(width: 12),
                    ],
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.forum_outlined,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 5),
                        Text(
                          summary,
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ],
                    ),
                    // Token 永远占位：有数据显数值，没数据显「Token–」，不再
                    // 整个消失导致右侧图标来回跳。
                    const SizedBox(width: 12),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.data_usage_outlined,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 5),
                        Text(
                          tokenUsage != null
                              ? 'Token${_formatTokens(tokenUsage.totalTokens)}'
                              : 'Token–',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ],
                    ),
                    // 常驻按钮区：复制全部（可能无）也在搜索左边，搜索钉死最右。
                    const SizedBox(width: 12),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (onCopyAll != null)
                          IconButton(
                            tooltip: '复制全部对话',
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.copy_all_rounded, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onCopyAll,
                          ),
                        if (onOpenPageMarks != null)
                          IconButton(
                            tooltip: '翻篇记录',
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.auto_stories, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onOpenPageMarks,
                          ),
                        IconButton(
                          tooltip: showSearch ? '隐藏搜索' : '搜索与日期定位',
                          visualDensity: VisualDensity.compact,
                          icon: Icon(
                            showSearch ? Icons.search_off : Icons.search,
                            size: 17,
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
                      label: Text('待办${openTodos ?? 0}'),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
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
                        // 窄屏时目标入口落在第二行：它没有独立一级 tab，这里是唯一入口，
                        // 空态也必须可见（故空态只显示「目标」，且用 accent 着色）。
                        NowStatusIconButton(
                          tooltip: '目标',
                          icon: Icons.flag_outlined,
                          label: goalLabel,
                          color: activeGoals.isEmpty
                              ? AppTheme.accentPrimary
                              : AppTheme.textTertiary,
                          onPressed: onOpenGoals,
                        ),
                        if (conversationId != null) ...[
                          const SizedBox(width: 4),
                          ChatProposalReviewButton(
                            conversationId: conversationId,
                            compact: true,
                          ),
                        ],
                        if (activeProvider != null) ...[
                          const SizedBox(width: 6),
                          Icon(
                            Icons.smart_toy_outlined,
                            size: 14,
                            color: AppTheme.textTertiary,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            activeProvider,
                            maxLines: 1,
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                        ],
                        const SizedBox(width: 6),
                        Icon(
                          Icons.forum_outlined,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 4),
                        Text(
                          summary,
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                        if (tokenUsage != null) ...[
                          const SizedBox(width: 8),
                          Icon(
                            Icons.data_usage_outlined,
                            size: 14,
                            color: AppTheme.textTertiary,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            'Token ${_formatTokens(tokenUsage.totalTokens)}',
                            maxLines: 1,
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                        ],
                        if (onCopyAll != null) ...[
                          const SizedBox(width: 4),
                          IconButton(
                            tooltip: '复制全部对话',
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.copy_all_rounded, size: 16),
                            color: AppTheme.textSecondary,
                            onPressed: onCopyAll,
                          ),
                        ],
                        if (onOpenPageMarks != null)
                          IconButton(
                            tooltip: '翻篇记录',
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.auto_stories, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onOpenPageMarks,
                          ),
                        IconButton(
                          tooltip: showSearch ? '隐藏搜索' : '搜索与日期定位',
                          visualDensity: VisualDensity.compact,
                          icon: Icon(
                            showSearch ? Icons.search_off : Icons.search,
                            size: 16,
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

  String _formatTokens(int value) {
    if (value >= 1000000) return '${(value / 1000000).toStringAsFixed(1)}M';
    if (value >= 1000) return '${(value / 1000).toStringAsFixed(1)}K';
    return value.toString();
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
                        size: 13,
                        color: danger.withValues(alpha: 0.85),
                      ),
                      const SizedBox(width: 4),
                      Text(
                        '系统提示',
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
                  child: SelectableText(
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
        size: 16,
        color: const Color(0xFFB91C1C).withValues(alpha: 0.85),
      ),
    );
  }
}
