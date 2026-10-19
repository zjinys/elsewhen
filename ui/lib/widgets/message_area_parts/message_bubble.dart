part of '../message_area.dart';

//! 消息气泡、存为主题按钮与 AI 打字指示。

class _MessageBubble extends ConsumerStatefulWidget {
  final Message message;

  /// 是否显示「重新生成」：仅最后一条用户消息（且无 AI 回复）时为真
  final bool showRetry;
  final VoidCallback? onRetry;

  /// 「翻篇到此」只在边界后还有原文的消息上出现（最后一条被
  /// 「不吞最新一轮」硬约束拒绝，直接不亮按钮）。
  final bool isLastMessage;

  const _MessageBubble({
    required this.message,
    this.isLastMessage = false,
    this.showRetry = false,
    this.onRetry,
  });

  @override
  ConsumerState<_MessageBubble> createState() => _MessageBubbleState();
}

class _MessageBubbleState extends ConsumerState<_MessageBubble> {
  @override
  Widget build(BuildContext context) {
    final message = widget.message;
    final isUser = message.isUser;
    final canFlipPast = !widget.isLastMessage;
    final displayContent = isUser
        ? message.content
        // 模型常把普通换行输出成空行分隔，聊天阅读中会显得过于松散；
        // 保留真实换行，但把连续空行收敛成单行间距。
        : message.content.replaceAll(RegExp(r'\n{2,}'), '\n');

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisAlignment: isUser
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(isUser: false),
            const SizedBox(width: AppTheme.space3),
          ],

          Flexible(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 820),
              child: Column(
                crossAxisAlignment: isUser
                    ? CrossAxisAlignment.end
                    : CrossAxisAlignment.start,
                children: [
                  // Role and time + 复制按钮
                  Padding(
                    padding: const EdgeInsets.only(
                      left: AppTheme.space2,
                      right: AppTheme.space2,
                      bottom: AppTheme.space1,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          '${isUser ? "我" : "AI"} • ${_formatTime(message.createdAt)}',
                          style: TextStyle(
                            color: AppTheme.textTertiary,
                            fontSize: 11,
                          ),
                        ),
                        const SizedBox(width: 2),
                        // 整条消息复制
                        IconButton(
                          icon: Icon(
                            Icons.copy_rounded,
                            size: 13,
                            color: AppTheme.textTertiary,
                          ),
                          tooltip: '复制这条消息',
                          visualDensity: VisualDensity.compact,
                          padding: EdgeInsets.zero,
                          constraints: const BoxConstraints(
                            minWidth: 24,
                            minHeight: 20,
                          ),
                          onPressed: () =>
                              _copyToClipboard(context, message.content, '消息'),
                        ),
                        // 「存为主题」：常驻显示在工具条（与「复制」并排），一眼可见。
                        // 移动端另支持长按气泡弹底部菜单（见下方 GestureDetector），两者同一逻辑。
                        // 点击不直接调工具——把明确指令预填进输入框，用户可改可发，
                        // 草稿仍走确认门（比按钮直写库透明，也不绕开用户把关）。
                        if (!isUser) _SaveTopicButton(message: message),
                        // 「翻篇到此」入口暂隐藏：功能在 Rust 侧已就绪，
                        // 待装机观测/冲突裁决后放量（canFlipPast 判定保留）。
                        // if (canFlipPast)
                        //   PageMarkToolButton(
                        //     conversationId: message.conversationId,
                        //     boundaryMessageId: message.id,
                        //   ),
                        if (isUser) _buildRecordability(context, ref),
                      ],
                    ),
                  ),

                  // Message bubble
                  // 移动端：长按气泡弹出操作菜单（复制 / 存为主题），是唯一的长按行为，
                  // 故移动端不用 SelectionArea/SelectableText（避免长按选字与菜单手势竞争）。
                  // 桌面端保持原有可选中（SelectionArea/SelectableText）+ hover 悬浮按钮。
                  GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onLongPress: _isMobilePlatform
                        ? () => _showMessageActionsSheet(
                              context, isUser, canFlipPast, message)
                        : null,
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: AppTheme.space3,
                        vertical: AppTheme.space3,
                      ),
                      decoration: BoxDecoration(
                        color: isUser
                            ? AppTheme.accentPrimary.withValues(alpha: 0.15)
                            : AppTheme.surface1,
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusMedium,
                        ),
                        border: Border.all(
                          color: isUser
                              ? AppTheme.accentPrimary.withValues(alpha: 0.3)
                              : AppTheme.surface3.withValues(alpha: 0.7),
                          width: 1,
                        ),
                      ),
                      child: _isMobilePlatform
                          // 移动端：不可选字，纯展示，长按交给外层菜单。
                          ? (isUser
                                ? ContentFontScope(
                                    child: Text(
                                      displayContent,
                                      style: TextStyle(
                                        color: AppTheme.textPrimary,
                                        fontSize: 14,
                                        height: 1.55,
                                      ),
                                    ),
                                  )
                                : MarkdownView(
                                    markdown: displayContent,
                                    baseStyle: TextStyle(
                                      color: AppTheme.textPrimary,
                                      fontSize: 15,
                                      height: 1.65,
                                    ),
                                  ))
                          // 桌面端：保持可选中。
                          : (isUser
                                ? ContentFontScope(
                                    child: SelectableText(
                                      displayContent,
                                      style: TextStyle(
                                        color: AppTheme.textPrimary,
                                        fontSize: 14,
                                        height: 1.55,
                                      ),
                                    ),
                                  )
                                : SelectionArea(
                                    child: MarkdownView(
                                      markdown: displayContent,
                                      baseStyle: TextStyle(
                                        color: AppTheme.textPrimary,
                                        fontSize: 15,
                                        height: 1.65,
                                      ),
                                    ),
                                  )),
                    ),
                  ),

                  if (kSuggestionFeedbackEnabled && !isUser)
                    SuggestionFeedbackControl(
                      messageId: message.id,
                      content: message.content,
                    ),
                  // 生成失败/未生成：最后一条用户消息上提供「重新生成」入口
                  if (!isUser && message.content.contains('[['))
                    KnowledgeCitationsButton(messageId: message.id),
                  if (widget.showRetry && widget.onRetry != null)
                    Padding(
                      padding: const EdgeInsets.only(top: AppTheme.space1),
                      child: TextButton.icon(
                        onPressed: widget.onRetry,
                        icon: const Icon(Icons.refresh_rounded, size: 15),
                        label: const Text('重新生成'),
                        style: TextButton.styleFrom(
                          foregroundColor: AppTheme.accentPrimary,
                          visualDensity: VisualDensity.compact,
                          padding: const EdgeInsets.symmetric(
                            horizontal: AppTheme.space2,
                            vertical: 2,
                          ),
                          textStyle: const TextStyle(fontSize: 12),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),

          if (isUser) ...[
            const SizedBox(width: AppTheme.space3),
            _buildAvatar(isUser: true),
          ],
        ],
      ),
    );
  }

  /// 移动端长按气泡弹出的操作菜单：复制（所有消息）+ 存为主题（仅 AI 消息）。
  /// 「存为主题」与桌面 hover 按钮同一条路径：把保存指令预填进输入框（草稿走确认门），
  /// 不是直接写库，用户可改可发。
  void _showMessageActionsSheet(
    BuildContext context,
    bool isUser,
    bool canFlipPast,
    Message message,
  ) {
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: AppTheme.surface0,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (sheetContext) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: Icon(Icons.copy_rounded, color: AppTheme.accentPrimary),
              title: const Text('复制'),
              onTap: () {
                Navigator.of(sheetContext).pop();
                _copyToClipboard(context, message.content, '消息');
              },
            ),
            if (!isUser)
              ListTile(
                leading: Icon(
                  Icons.bookmark_add_outlined,
                  color: AppTheme.accentPrimary,
                ),
                title: const Text('存为主题'),
                subtitle: const Text('提取标题与正文，确认后存入主题库'),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  SaveTopicDialog.show(context, message.content);
                },
              ),
            // 「翻篇到此」入口暂隐藏（同工具条注释）。
            // if (canFlipPast)
            //   ListTile(...),
          ],
        ),
      ),
    );
  }

  Widget _buildRecordability(BuildContext context, WidgetRef ref) {
    final state = ref.watch(messageRecordabilityProvider(widget.message.id));
    final value = state.value;
    if (state.isLoading && value == null) {
      return const SizedBox(
        width: 20,
        height: 20,
        child: Padding(
          padding: EdgeInsets.all(6),
          child: CircularProgressIndicator(strokeWidth: 1.5),
        ),
      );
    }
    if (value == null) return const SizedBox.shrink();
    final analyzing =
        const ['pending', 'running', 'retry'].contains(value.jobStatus) &&
        value.source == 'default';
    // 分析结束前不露头：技术化的中间态（“分析中”）对用户无意义，
    // staleness 轮询完成后会自然变成 记录/讨论。
    if (analyzing) return const SizedBox.shrink();
    final label = analyzing ? '分析中' : (value.recordable ? '记录' : '讨论');
    final icon = analyzing
        ? Icons.sync
        : (value.recordable ? Icons.bookmark_outline : Icons.forum_outlined);
    final color = value.recordable
        ? AppTheme.accentPrimary
        : AppTheme.textTertiary;
    return PopupMenuButton<String>(
      tooltip: '记录分类',
      onSelected: (action) =>
          _handleRecordabilityAction(context, ref, value.eventId, action),
      itemBuilder: (_) => const [
        PopupMenuItem(
          value: 'record',
          child: ListTile(
            leading: Icon(Icons.bookmark_outline),
            title: Text('纳入记录'),
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
        ),
        PopupMenuItem(
          value: 'discussion',
          child: ListTile(
            leading: Icon(Icons.forum_outlined),
            title: Text('作为讨论'),
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
        ),
        PopupMenuDivider(),
        PopupMenuItem(
          value: 'reanalyze',
          child: ListTile(
            leading: Icon(Icons.refresh),
            title: Text('重新分析'),
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
        ),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 3),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 12, color: color),
            const SizedBox(width: 3),
            Text(label, style: TextStyle(fontSize: 10.5, color: color)),
          ],
        ),
      ),
    );
  }

  Future<void> _handleRecordabilityAction(
    BuildContext context,
    WidgetRef ref,
    String eventId,
    String action,
  ) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      if (action == 'reanalyze') {
        final queued = await repo.reanalyzeEvent(eventId);
        if (!queued) throw StateError('事件正在分析中，请稍后再试');
      } else {
        await repo.setEventRecordability(eventId, action == 'record');
      }
      ref.invalidate(messageRecordabilityProvider(widget.message.id));
      if (context.mounted) {
        final text = action == 'record'
            ? '已纳入记录'
            : action == 'discussion'
            ? '已作为讨论，不再进入日流和长期事实'
            : '已加入重新分析队列';
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(text)));
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(reportUiError('更新消息状态', error))));
      }
    }
  }

  Widget _buildAvatar({required bool isUser}) {
    return Container(
      width: 32,
      height: 32,
      decoration: BoxDecoration(
        color: isUser
            ? AppTheme.accentPrimary.withValues(alpha: 0.2)
            : AppTheme.surface3,
        shape: BoxShape.circle,
      ),
      child: Icon(
        isUser ? Icons.person : Icons.smart_toy,
        size: 18,
        color: isUser ? AppTheme.accentPrimary : AppTheme.textSecondary,
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final messageDate = DateTime(time.year, time.month, time.day);

    if (messageDate == today) {
      return DateFormat('HH:mm').format(time);
    } else {
      return DateFormat('MM-dd HH:mm').format(time);
    }
  }
}

/// 「存为主题」按钮：常驻显示在 AI 气泡工具条（与「复制」并排），
/// 点击弹出「存为主题」对话框：从消息提取标题/正文预填，用户确认后存为 topic 页。
class _SaveTopicButton extends StatelessWidget {
  final Message message;

  const _SaveTopicButton({required this.message});

  @override
  Widget build(BuildContext context) {
    return AppPressable(
      scaleOnPress: 0.88,
      duration: const Duration(milliseconds: 120),
      child: IconButton(
        icon: Icon(
          Icons.bookmark_add_outlined,
          size: 14,
          color: AppTheme.accentPrimary,
        ),
        tooltip: '把这条内容存为主题',
        visualDensity: VisualDensity.compact,
        padding: EdgeInsets.zero,
        constraints: const BoxConstraints(minWidth: 24, minHeight: 20),
        onPressed: () => SaveTopicDialog.show(context, message.content),
      ),
    );
  }
}

/// AI 回复生成中的占位气泡：用户发送后、回复入库前，让界面明确告知
/// 「后台正在执行」，避免发送后长时间无反馈的错觉。
class _AiTypingBubble extends StatefulWidget {
  const _AiTypingBubble();

  @override
  State<_AiTypingBubble> createState() => _AiTypingBubbleState();
}

class _AiTypingBubbleState extends State<_AiTypingBubble>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 32,
            height: 32,
            decoration: BoxDecoration(
              color: AppTheme.surface3,
              shape: BoxShape.circle,
            ),
            child: Icon(
              Icons.smart_toy,
              size: 18,
              color: AppTheme.textSecondary,
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          Flexible(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.surface3, width: 1),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  AnimatedBuilder(
                    animation: _controller,
                    builder: (context, child) {
                      return Row(
                        mainAxisSize: MainAxisSize.min,
                        children: List.generate(3, (index) {
                          final delay = index * 0.2;
                          final t = (_controller.value - delay) % 1.0;
                          // 柔和正弦起伏 (0.35 到 1.0 的透明度及微小比例缩放)
                          final bounce = (sin(t * 2 * pi) + 1) / 2;
                          final opacity = 0.35 + (0.65 * bounce);
                          final scale = 0.85 + (0.3 * bounce);
                          return Padding(
                            padding: EdgeInsets.only(
                              right: index < 2 ? 4.0 : 0.0,
                            ),
                            child: Transform.scale(
                              scale: scale,
                              child: Opacity(
                                opacity: opacity,
                                child: Container(
                                  width: 6,
                                  height: 6,
                                  decoration: BoxDecoration(
                                    color: AppTheme.accentPrimary,
                                    shape: BoxShape.circle,
                                  ),
                                ),
                              ),
                            ),
                          );
                        }),
                      );
                    },
                  ),
                  const SizedBox(width: 10),
                  Text(
                    'AI 正在思考…',
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 13,
                      letterSpacing: 0.2,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
