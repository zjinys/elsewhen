part of '../message_area.dart';

//! 消息气泡、存为主题按钮与 AI 打字指示。

/// Internal generation state is persisted in message content for auditability,
/// but it is presented as a quiet status affordance rather than chat prose.
({String note, String content})? _splitSystemAnnotation(String content) {
  final match = RegExp(r'^〔系统标注：([\s\S]*?)〕\s*').firstMatch(content);
  if (match == null) return null;
  return (note: match.group(1)!.trim(), content: content.substring(match.end));
}

String _messageDisplayContent(String content) =>
    _splitSystemAnnotation(content)?.content ?? content;

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
    final bubbleStyle = ChatBubbleStyle.of(context);
    final inlineMetadata = !isUser && bubbleStyle.isLight;
    final annotation = isUser ? null : _splitSystemAnnotation(message.content);
    final canFlipPast = !widget.isLastMessage;
    // 模型常把普通换行输出成空行分隔，聊天阅读中会显得过于松散；
    // 保留真实换行，但把连续空行收敛成单行间距。
    final displayContent =
        (isUser ? message.content : (annotation?.content ?? message.content))
            .replaceAll(RegExp(r'\n{2,}'), '\n');
    // 对话气泡跟随「外观 → 正文字号」：全对话流字号从同一基准推算
    // （ChatTypeScale），AI 消息为基准，用户消息小一号维持层次差（原 15/14）。
    final scale = ref.watch(settingsProvider.select(chatTypeScaleOf));
    final aiFontSize = scale.ai;
    final userFontSize = scale.user;
    final metaColor = bubbleStyle.metadataColor;

    final metadata = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: Text.rich(
            TextSpan(
              children: [
                TextSpan(
                  text: isUser ? context.l10n.me : 'AI',
                  style: inlineMetadata
                      ? TextStyle(
                          color: bubbleStyle.palette.textPrimary,
                          fontWeight: FontWeight.w600,
                        )
                      : null,
                ),
                TextSpan(text: ' • ${_formatTime(message.createdAt)}'),
              ],
            ),
            style: TextStyle(color: metaColor, fontSize: scale.meta),
          ),
        ),
        SizedBox(width: inlineMetadata ? AppTheme.space2 : 2),
        IconButton(
          icon: Icon(Icons.copy_rounded, size: 13, color: metaColor),
          tooltip: context.l10n.copyThisMessage,
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          constraints: const BoxConstraints(minWidth: 24, minHeight: 20),
          style: inlineMetadata && !_isMobilePlatform
              ? IconButton.styleFrom(
                  minimumSize: const Size(28, 28),
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                )
              : null,
          onPressed: () =>
              _copyToClipboard(context, displayContent, context.l10n.message),
        ),
        if (annotation != null)
          Tooltip(
            message: annotation.note.isEmpty
                ? context.l10n.systemNotice
                : '${context.l10n.systemNotice}: ${annotation.note}',
            child: Semantics(
              label: context.l10n.systemNotice,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 5),
                child: Icon(Icons.info_outline, size: 13, color: metaColor),
              ),
            ),
          ),
        if (!isUser)
          _SaveTopicButton(
            message: message,
            content: displayContent,
            iconColor: inlineMetadata ? metaColor : bubbleStyle.accent,
            compact: inlineMetadata && !_isMobilePlatform,
          ),
        if (isUser) _buildRecordability(context, ref),
      ],
    );

    final bodyStyle = TextStyle(
      color: bubbleStyle.palette.textPrimary,
      fontSize: isUser ? userFontSize : aiFontSize,
      height: isUser ? 1.55 : 1.65,
    );
    final Widget body;
    if (isUser) {
      body = ContentFontScope(
        child: _isMobilePlatform
            ? Text(displayContent, style: bodyStyle)
            : SelectableText(displayContent, style: bodyStyle),
      );
    } else {
      final markdown = MarkdownView(
        markdown: displayContent,
        baseStyle: bodyStyle,
      );
      body = _isMobilePlatform ? markdown : SelectionArea(child: markdown);
    }

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisAlignment: isUser
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        children: [
          if (!isUser) ...[
            Padding(
              padding: EdgeInsets.only(
                top: inlineMetadata ? AppTheme.space1 : 0,
              ),
              child: _buildAvatar(isUser: false),
            ),
            const SizedBox(width: AppTheme.space3),
          ],

          Flexible(
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxWidth: isUser ? 820 : bubbleStyle.assistantMaxWidth,
              ),
              child: Column(
                crossAxisAlignment: isUser
                    ? CrossAxisAlignment.end
                    : CrossAxisAlignment.start,
                children: [
                  // 元信息行始终在卡片外面（上方）：浅色紧凑模式下也一样，
                  // 不把「AI · 时间 + 操作」塞进卡片内部。
                  Padding(
                    padding: const EdgeInsets.only(
                      left: AppTheme.space2,
                      right: AppTheme.space2,
                      bottom: AppTheme.space1,
                    ),
                    child: metadata,
                  ),

                  // Message bubble
                  // 移动端：长按气泡弹出操作菜单（复制 / 存为主题），是唯一的长按行为，
                  // 故移动端不用 SelectionArea/SelectableText（避免长按选字与菜单手势竞争）。
                  // 桌面端保持原有可选中（SelectionArea/SelectableText）+ hover 悬浮按钮。
                  GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onLongPress: _isMobilePlatform
                        ? () => _showMessageActionsSheet(
                            context,
                            isUser,
                            canFlipPast,
                            message,
                          )
                        : null,
                    child: Container(
                      padding: EdgeInsets.symmetric(
                        horizontal: inlineMetadata
                            ? AppTheme.space4
                            : AppTheme.space3,
                        vertical: inlineMetadata
                            ? AppTheme.space2
                            : AppTheme.space3,
                      ),
                      decoration: isUser
                          ? bubbleStyle.userBubble(outlined: true)
                          : bubbleStyle.assistantCard(
                              darkBorder: AppTheme.surface3.withValues(
                                alpha: .7,
                              ),
                            ),
                      child: body,
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
                        label: Text(context.l10n.regenerate),
                        style: TextButton.styleFrom(
                          foregroundColor: AppTheme.accentPrimary,
                          visualDensity: VisualDensity.compact,
                          padding: const EdgeInsets.symmetric(
                            horizontal: AppTheme.space2,
                            vertical: 2,
                          ),
                          textStyle: TextStyle(fontSize: scale.button),
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
              title: Text(context.l10n.copyMessage),
              onTap: () {
                Navigator.of(sheetContext).pop();
                _copyToClipboard(
                  context,
                  message.content,
                  context.l10n.message,
                );
              },
            ),
            if (!isUser)
              ListTile(
                leading: Icon(
                  Icons.bookmark_add_outlined,
                  color: AppTheme.accentPrimary,
                ),
                title: Text(context.l10n.saveAsTopic),
                subtitle: Text(context.l10n.saveAsTopicDescription),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  SaveTopicDialog.show(
                    context,
                    _messageDisplayContent(message.content),
                  );
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
    final label = analyzing
        ? context.l10n.aiAnalyzing
        : (value.recordable ? context.l10n.record : context.l10n.discussion);
    final icon = analyzing
        ? Icons.sync
        : (value.recordable ? Icons.bookmark_outline : Icons.forum_outlined);
    final color = value.recordable
        ? AppTheme.accentPrimary
        : AppTheme.textTertiary;
    return PopupMenuButton<String>(
      tooltip: context.l10n.recordCategory,
      onSelected: (action) =>
          _handleRecordabilityAction(context, ref, value.eventId, action),
      itemBuilder: (_) => [
        PopupMenuItem(
          value: 'record',
          child: ListTile(
            leading: Icon(Icons.bookmark_outline),
            title: Text(context.l10n.includeInRecords),
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
        ),
        PopupMenuItem(
          value: 'discussion',
          child: ListTile(
            leading: Icon(Icons.forum_outlined),
            title: Text(context.l10n.asDiscussion),
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
        ),
        PopupMenuDivider(),
        PopupMenuItem(
          value: 'reanalyze',
          child: ListTile(
            leading: Icon(Icons.refresh),
            title: Text(context.l10n.reanalyze),
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
            Text(label, style: TextStyle(fontSize: 11, color: color)),
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
        if (!queued) {
          if (context.mounted) {
            FeedbackToast.of(context).show(
              SnackBar(content: Text(context.l10n.eventAnalyzing)),
            );
          }
          return;
        }
      } else {
        await repo.setEventRecordability(eventId, action == 'record');
      }
      ref.invalidate(messageRecordabilityProvider(widget.message.id));
      if (context.mounted) {
        final text = action == 'record'
            ? context.l10n.savedAsRecord
            : action == 'discussion'
            ? context.l10n.savedAsDiscussion
            : context.l10n.reanalyzeQueued;
        FeedbackToast.of(context)
            .show(SnackBar(content: Text(text)));
      }
    } catch (error) {
      if (context.mounted) {
        FeedbackToast.of(context).show(
          SnackBar(
            content: Text(
              context.errorMessage(context.l10n.updateMessageStatus, error),
            ),
          ),
        );
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
        size: DesignTokens.icon,
        color: isUser ? AppTheme.accentPrimary : AppTheme.textSecondary,
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final messageDate = DateTime(time.year, time.month, time.day);

    if (messageDate == today) {
      return DateFormat.Hm(context.l10n.localeName).format(time);
    } else {
      return DateFormat.Md(context.l10n.localeName).add_Hm().format(time);
    }
  }
}

/// 「存为主题」按钮：常驻显示在 AI 气泡工具条（与「复制」并排），
/// 点击弹出「存为主题」对话框：从消息提取标题/正文预填，用户确认后存为 topic 页。
class _SaveTopicButton extends StatelessWidget {
  final Message message;
  final String? content;
  final Color iconColor;
  final bool compact;

  const _SaveTopicButton({
    required this.message,
    this.content,
    required this.iconColor,
    this.compact = false,
  });

  @override
  Widget build(BuildContext context) {
    return AppPressable(
      scaleOnPress: 0.88,
      duration: const Duration(milliseconds: 120),
      child: IconButton(
        icon: Icon(Icons.bookmark_add_outlined, size: 14, color: iconColor),
        tooltip: context.l10n.saveMessageAsTopic,
        visualDensity: VisualDensity.compact,
        padding: EdgeInsets.zero,
        constraints: const BoxConstraints(minWidth: 24, minHeight: 20),
        style: compact
            ? IconButton.styleFrom(
                minimumSize: const Size(28, 28),
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              )
            : null,
        onPressed: () => SaveTopicDialog.show(
          context,
          content ?? _messageDisplayContent(message.content),
        ),
      ),
    );
  }
}

/// AI 回复生成中的占位气泡：用户发送后、回复入库前，让界面明确告知
/// 「后台正在执行」，避免发送后长时间无反馈的错觉。
class _AiTypingBubble extends ConsumerStatefulWidget {
  const _AiTypingBubble();

  @override
  ConsumerState<_AiTypingBubble> createState() => _AiTypingBubbleState();
}

class _AiTypingBubbleState extends ConsumerState<_AiTypingBubble>
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
    // 「正在思考」文案与建议 chips 同档（原 13）。
    final typeScale = ref.watch(settingsProvider.select(chatTypeScaleOf));
    final bubbleStyle = ChatBubbleStyle.of(context);
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
              size: DesignTokens.icon,
              color: AppTheme.textSecondary,
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          Flexible(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              decoration: bubbleStyle.assistantCard(
                darkFill: AppTheme.surface2,
                darkBorder: AppTheme.surface3,
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
                    context.l10n.thinking,
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: typeScale.chip,
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
