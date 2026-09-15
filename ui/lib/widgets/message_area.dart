import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';
import '../models/conversation.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';

class MessageArea extends ConsumerStatefulWidget {
  const MessageArea({super.key});

  @override
  ConsumerState<MessageArea> createState() => _MessageAreaState();
}

class _MessageAreaState extends ConsumerState<MessageArea> {
  /// 初始窗口与每页加载量：只显示最近 N 条，旧的通过「显示更早」逐步展开
  static const int _windowSize = 20;
  static const int _pageSize = 20;

  final ScrollController _scrollController = ScrollController();
  bool _expanded = false;
  int _shownSince = 0; // 展开后从消息开头跳过的条数
  bool _nearBottom = true;
  bool _pendingScrollToBottom = true; // 会话刚切换/初始加载后，数据到达时滚到最新

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(() {
      final pos = _scrollController.position;
      _nearBottom =
          !pos.hasContentDimensions || pos.pixels >= pos.maxScrollExtent - 120;
    });
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final messagesAsync = ref.watch(messagesProvider);
    final selectedId = ref.watch(selectedConversationIdProvider);

    // 侦听必须在 build 中注册（riverpod 2.x 约束）。
    // 切换会话：重置窗口，数据到达后回到最新消息
    ref.listen(selectedConversationIdProvider, (prev, next) {
      if (prev == next) return;
      _expanded = false;
      _shownSince = 0;
      _nearBottom = true;
      _pendingScrollToBottom = true;
      if (mounted) setState(() {});
    });
    // 新消息（用户发送或 AI 回复）：若停在底部则跟随滚动；
    // 或会话刚切换（pending），数据到达后滚到最新
    ref.listen(messagesProvider, (prev, next) {
      final nextLen = next.value?.length ?? 0;
      final prevLen = prev?.value?.length ?? 0;
      if (_pendingScrollToBottom || (nextLen > prevLen && _nearBottom)) {
        _pendingScrollToBottom = false;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (_scrollController.hasClients) {
            _scrollController.animateTo(
              _scrollController.position.maxScrollExtent,
              duration: const Duration(milliseconds: 200),
              curve: Curves.easeOut,
            );
          }
        });
      }
    });

    if (selectedId == null) {
      return _buildEmptyState();
    }

    return Column(
      children: [
        // 复制全部对话（有消息时显示）
        messagesAsync.when(
          data: (messages) => messages.isEmpty
              ? const SizedBox.shrink()
              : _buildCopyAllHeader(context, messages),
          loading: () => const SizedBox.shrink(),
          error: (_, __) => const SizedBox.shrink(),
        ),
        // Messages list
        Expanded(
          child: messagesAsync.when(
            data: (messages) {
              if (messages.isEmpty) {
                return _buildNoMessages();
              }

              // 默认只看最近 N 条，点「显示更早」逐页展开
              final total = messages.length;
              final shownSince = _expanded
                  ? _shownSince
                  : (total > _windowSize ? total - _windowSize : 0);
              final hasMore = shownSince > 0;
              final visibleCount = total - shownSince;

              return ListView.builder(
                controller: _scrollController,
                padding: const EdgeInsets.all(AppTheme.space4),
                itemCount: visibleCount + (hasMore ? 1 : 0),
                itemBuilder: (context, index) {
                  if (hasMore && index == 0) {
                    return _buildLoadMoreButton(total);
                  }
                  final message = messages[shownSince + index - (hasMore ? 1 : 0)];
                  return _MessageBubble(message: message);
                },
              );
            },
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (error, stack) => Center(
              child: Text(
                '加载消息失败',
                style: TextStyle(color: AppTheme.textSecondary),
              ),
            ),
          ),
        ),

        // Input area
        _MessageInput(),
      ],
    );
  }

  /// 加载更早消息：窗口向前扩展一页，然后回到顶部看旧内容
  void _loadMore(int total) {
    // 当前有效窗口：未展开时为「只看最近 N 条」的起点
    final currentShown =
        _expanded ? _shownSince : (total > _windowSize ? total - _windowSize : 0);
    setState(() {
      _expanded = true;
      _shownSince = (currentShown - _pageSize).clamp(0, total);
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          0,
          duration: const Duration(milliseconds: 250),
          curve: Curves.easeOut,
        );
      }
    });
  }

  Widget _buildLoadMoreButton(int total) {
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Center(
        child: TextButton.icon(
          onPressed: () => _loadMore(total),
          icon: const Icon(Icons.keyboard_arrow_up, size: 18),
          label: const Text('显示更早的消息'),
          style: TextButton.styleFrom(
            foregroundColor: AppTheme.textSecondary,
            visualDensity: VisualDensity.compact,
          ),
        ),
      ),
    );
  }

  /// 复制全部对话（含角色与时间，适合粘贴为记录）
  Widget _buildCopyAllHeader(BuildContext context, List<Message> messages) {
    return Padding(
      padding: const EdgeInsets.only(
        top: AppTheme.space2,
        right: AppTheme.space3,
        bottom: 0,
      ),
      child: Row(
        children: [
          const Spacer(),
          IconButton(
            icon: Icon(
              Icons.copy_all_rounded,
              size: 18,
              color: AppTheme.textSecondary,
            ),
            tooltip: '复制全部对话',
            visualDensity: VisualDensity.compact,
            onPressed: () => _copyToClipboard(
              context,
              _formatConversationForCopy(messages),
              '对话',
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.forum_outlined,
            size: 64,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: AppTheme.space4),
          Text(
            '选择一个对话开始聊天',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 16,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildNoMessages() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.chat_bubble_outline,
            size: 48,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: AppTheme.space3),
          Text(
            '还没有消息',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 14,
            ),
          ),
          const SizedBox(height: AppTheme.space2),
          Text(
            '在下方输入框开始对话',
            style: TextStyle(
              color: AppTheme.textTertiary,
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }
}

class _MessageBubble extends StatelessWidget {
  final Message message;

  const _MessageBubble({required this.message});

  @override
  Widget build(BuildContext context) {
    final isUser = message.isUser;

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisAlignment: isUser ? MainAxisAlignment.end : MainAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(isUser: false),
            const SizedBox(width: AppTheme.space3),
          ],

          Flexible(
            child: Column(
              crossAxisAlignment: isUser ? CrossAxisAlignment.end : CrossAxisAlignment.start,
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
                        onPressed: () => _copyToClipboard(
                          context,
                          message.content,
                          '消息',
                        ),
                      ),
                    ],
                  ),
                ),

                // Message bubble
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space3,
                    vertical: AppTheme.space3,
                  ),
                  decoration: BoxDecoration(
                    color: isUser ? AppTheme.accentPrimary.withValues(alpha: 0.15) : AppTheme.surface2,
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    border: Border.all(
                      color: isUser
                          ? AppTheme.accentPrimary.withValues(alpha: 0.3)
                          : AppTheme.surface3,
                      width: 1,
                    ),
                  ),
                  // SelectableText：支持鼠标选中局部复制
                  child: SelectableText(
                    message.content,
                    style: TextStyle(
                      color: AppTheme.textPrimary,
                      fontSize: 14,
                      height: 1.5,
                    ),
                  ),
                ),
              ],
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

  Widget _buildAvatar({required bool isUser}) {
    return Container(
      width: 32,
      height: 32,
      decoration: BoxDecoration(
        color: isUser ? AppTheme.accentPrimary.withValues(alpha: 0.2) : AppTheme.surface3,
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

class _MessageInput extends ConsumerStatefulWidget {
  @override
  ConsumerState<_MessageInput> createState() => _MessageInputState();
}

class _MessageInputState extends ConsumerState<_MessageInput> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  bool _isSubmitting = false;

  @override
  void initState() {
    super.initState();
    // 回车直接提交（多行输入下 onSubmitted 不触发，需在焦点节点拦 Enter）
    _focusNode.onKeyEvent = (node, event) {
      final isEnter = event.logicalKey == LogicalKeyboardKey.enter;
      final isShift = HardwareKeyboard.instance.isShiftPressed;
      if (event is KeyDownEvent && isEnter && !isShift) {
        _handleSubmit();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    };
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _showError(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(message),
        backgroundColor: const Color(0xFFB91C1C),
        behavior: SnackBarBehavior.floating,
        shape: const RoundedRectangleBorder(
          borderRadius: BorderRadius.all(Radius.circular(999)),
        ),
      ),
    );
  }

  Future<void> _handleSubmit() async {
    final text = _controller.text.trim();
    if (text.isEmpty || _isSubmitting) return;

    final conversationId = ref.read(selectedConversationIdProvider);
    if (conversationId == null) {
      _showError('请先选择一个会话');
      return;
    }

    setState(() => _isSubmitting = true);

    try {
      final repo = ref.read(conversationRepositoryProvider);
      // 先把用户消息写入库（不依赖 AI）
      await repo.sendMessage(conversationId, text);

      _controller.clear();
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);
    } catch (e) {
      // 写库失败：明确反馈
      _showError('发送失败：$e');
      return;
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
      _focusNode.requestFocus();
    }

    if (!mounted) return;
    // 接着触发 AI 分析生成回复（不阻塞输入，出错单独反馈）
    await _generateAiReply(conversationId);
  }

  Future<void> _generateAiReply(String conversationId) async {
    try {
      final repo = ref.read(conversationRepositoryProvider);
      await repo.generateReply(conversationId);
      if (!mounted) return;
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);
    } catch (e) {
      final message = e.toString();
      if (message.contains('No active AI provider')) {
        _showError('尚未配置 AI Provider，请到设置页填写后重试');
      } else {
        _showError('AI 回复失败：$e');
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          top: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: TextField(
              controller: _controller,
              focusNode: _focusNode,
              maxLines: 4,
              minLines: 1,
              enabled: !_isSubmitting,
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 14,
              ),
              decoration: InputDecoration(
                hintText: '输入消息...',
                hintStyle: TextStyle(color: AppTheme.textTertiary),
                filled: true,
                fillColor: AppTheme.surface2,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
                ),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: AppTheme.space3,
                  vertical: AppTheme.space3,
                ),
              ),
              onSubmitted: (_) => _handleSubmit(),
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          IconButton(
            onPressed: _isSubmitting ? null : _handleSubmit,
            icon: _isSubmitting
                ? const SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.send),
            color: AppTheme.accentPrimary,
            iconSize: 24,
            tooltip: '发送',
          ),
        ],
      ),
    );
  }
}

/// 写入系统剪贴板并给出轻量反馈
Future<void> _copyToClipboard(BuildContext context, String text, String label) async {
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text('已复制$label'),
      duration: const Duration(seconds: 1),
      behavior: SnackBarBehavior.floating,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(999)),
      ),
    ),
  );
}

/// 把整段对话格式化为可粘贴文本（角色 + 时间 + 内容）
String _formatConversationForCopy(List<Message> messages) {
  return messages
      .map((m) {
        final role = m.isUser ? '我' : 'AI';
        final time = DateFormat('MM-dd HH:mm').format(m.createdAt);
        return '$role ($time)\n${m.content}';
      })
      .join('\n\n');
}
