import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../models/conversation.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';

/// 触发一次 AI 生成（发送后自动触发，或失败气泡上的「重新生成」点击）。
/// 首次发送与重试走完全相同的路径：成功刷新消息/会话列表、失败追加
/// 「会话内错误气泡」（带重新生成入口），结束统一清除「生成中」状态。
/// [isMounted]：组件可能已卸载（如生成期间切走 tab），回调返回 false 时
/// 跳过依赖 ref 的界面刷新（成功入库后的 invalidate 由下次加载兜底）。
Future<void> runAiGeneration(
  WidgetRef ref,
  String conversationId, {
  bool Function()? isMounted,
  VoidCallback? onError,
}) async {
  final generatingNotifier = ref.read(aiGeneratingProvider.notifier);
  try {
    setAiGenerating(ref, conversationId, true);
    final repo = ref.read(conversationRepositoryProvider);
    await repo.generateReply(conversationId);
    if (isMounted == null || isMounted()) {
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);
      ref.invalidate(pendingActionsProvider);
    }
  } catch (e) {
    addConversationNotice(ref, conversationId, aiFailureNotice(e));
    ref.read(scrollRequestProvider.notifier).state++;
    if (isMounted == null || isMounted()) onError?.call();
  } finally {
    final next = {...generatingNotifier.state}..remove(conversationId);
    generatingNotifier.state = next;
  }
}

/// 把 AI 生成失败整理成**一行可读文案**，不展示 Anyhow 的 Caused by 调用链。
/// - 未配置 provider：直接给引导文案
/// - 其余：取调用链上的根因（最后一级 cause）；没有链则取首行
String aiFailureNotice(Object e) {
  final raw = e.toString();
  if (raw.contains('No active AI provider')) {
    return '尚未配置 AI Provider，请到设置页填写后重试';
  }

  var detail = raw;
  final causedBy = 'Caused by:';
  if (raw.contains(causedBy)) {
    final causes = raw
        .substring(raw.indexOf(causedBy) + causedBy.length)
        .split(RegExp(r'\n\s*\d+:\s*'))
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();
    if (causes.isNotEmpty) detail = causes.last;
  } else {
    detail = raw.split('\n').first.trim();
  }
  // 去掉 AnyhowException(...) 外壳与其多出的收尾括号（wrapper 恰好多一个）
  const prefix = 'AnyhowException(';
  if (detail.startsWith(prefix)) detail = detail.substring(prefix.length);
  if (detail.endsWith(')')) {
    detail = detail.substring(0, detail.length - 1).trimRight();
  }
  return 'AI 回复失败：$detail';
}

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
    // 会话临时提示（AI 回复失败等，仅内存，不写库）
    final notices =
        ref.watch(conversationNoticeProvider)[selectedId] ?? const <String>[];
    // 正在生成 AI 回复的会话（发送后反馈「AI 生成中」占位气泡）
    final generatingIds = ref.watch(aiGeneratingProvider);

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
    // 当前会话进入「AI 生成中」→ 滚到底部展示生成中占位气泡
    ref.listen(aiGeneratingProvider, (prev, next) {
      final id = ref.read(selectedConversationIdProvider);
      if (id == null) return;
      final started = next.contains(id) && !(prev?.contains(id) ?? false);
      if (started) _scheduleScrollToBottom();
    });
    // 新消息（用户发送或 AI 回复）：若停在底部则跟随滚动。
    // 「会话切换/初次加载的滚到最新」不在这里消费 —— loading 阶段触发会把
    // _pendingScrollToBottom 消耗掉却滚不动（列表未挂载）；改由数据分支在
    // 渲染完成后兜底滚到底部。
    ref.listen(messagesProvider, (prev, next) {
      final nextLen = next.value?.length ?? 0;
      final prevLen = prev?.value?.length ?? 0;
      if (nextLen > prevLen && _nearBottom) {
        _scheduleScrollToBottom();
      }
    });
    // 会话临时提示追加（错误气泡等）→ 滚到底部展示
    ref.listen(scrollRequestProvider, (prev, next) {
      if (next != prev) _scheduleScrollToBottom();
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
        _PendingRelationsBanner(),
        // Messages list
        Expanded(
          child: messagesAsync.when(
            data: (messages) {
              // 当前会话是否正在生成 AI 回复（渲染尾部占位气泡）
              final isAiTyping = generatingIds.contains(selectedId);

              if (messages.isEmpty && notices.isEmpty && !isAiTyping) {
                return _buildNoMessages();
              }

              // 会话刚切换 / 初次加载：数据渲染完成后滚到最新消息。
              // 放在渲染分支内保证 postFrame 时列表已挂载（hasClients 为真），
              // 无论上游是 loading 空跑还是数据缓存的路径，都能稳定滚到底。
              if (_pendingScrollToBottom) {
                _pendingScrollToBottom = false;
                _scheduleScrollToBottom();
              }

              // 默认只看最近 N 条，点「显示更早」逐页展开
              final total = messages.length;
              final shownSince = _expanded
                  ? _shownSince
                  : (total > _windowSize ? total - _windowSize : 0);
              final hasMore = shownSince > 0;
              final visibleCount = total - shownSince;
              final tailCount = notices.length + (isAiTyping ? 1 : 0);

              return ListView.builder(
                controller: _scrollController,
                padding: const EdgeInsets.all(AppTheme.space4),
                itemCount: visibleCount + (hasMore ? 1 : 0) + tailCount,
                itemBuilder: (context, index) {
                  if (hasMore && index == 0) {
                    return _buildLoadMoreButton(total);
                  }
                  final libIndex = index - (hasMore ? 1 : 0);
                  if (libIndex < visibleCount) {
                    final message = messages[shownSince + libIndex];
                    // 「重新生成」只出现在最后一条消息上：它必须是用户消息
                    // （即后面没有 AI 回复 —— 生成失败、或还没生成），
                    // 且当前不在生成中（生成期间以「AI 正在思考…」占位反馈）。
                    final isLast = shownSince + libIndex == total - 1;
                    final needsReply = isLast && message.isUser && !isAiTyping;
                    return _MessageBubble(
                      message: message,
                      showRetry: needsReply,
                      onRetry: () => _retryAiGeneration(selectedId),
                    );
                  }
                  final tailIndex = libIndex - visibleCount;
                  if (tailIndex < notices.length) {
                    return _NoticeBubble(text: notices[tailIndex]);
                  }
                  return const _AiTypingBubble();
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

  /// 消息列表滚到底部（新消息 / 临时提示 / 会话切换时）。
  /// 懒加载 ListView 滚动中会 build 出新条目、extent 变大，单次滚动目标
  /// 会落后；因此跳到当前底部后在下一帧复查，extent 还有增长就继续跳，
  /// 迭代至固定点 —— 保证真正停在最新消息位置。
  void _scheduleScrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) => _jumpToBottom());
  }

  void _jumpToBottom() {
    if (!_scrollController.hasClients) return;
    _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scrollController.hasClients) return;
      final pos = _scrollController.position;
      if (pos.pixels < pos.maxScrollExtent - 1) {
        _jumpToBottom();
      }
    });
  }

  /// 错误气泡上的「重新生成」：清掉本次失败提示后，
  /// 按与首次发送完全相同的路径重启 AI 生成。
  /// 重试期间以「AI 正在思考…」占位反馈；若再次失败，
  /// runAiGeneration 会追加新的错误气泡（仍可继续重试）。
  void _retryAiGeneration(String conversationId) {
    if (ref.read(aiGeneratingProvider).contains(conversationId)) return;
    clearConversationNotices(ref, conversationId);
    if (mounted) setState(() {});
    runAiGeneration(ref, conversationId, isMounted: () => mounted);
  }

  /// 加载更早消息：窗口向前扩展一页，然后回到顶部看旧内容
  void _loadMore(int total) {
    // 当前有效窗口：未展开时为「只看最近 N 条」的起点
    final currentShown = _expanded
        ? _shownSince
        : (total > _windowSize ? total - _windowSize : 0);
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
          Icon(Icons.forum_outlined, size: 64, color: AppTheme.textTertiary),
          const SizedBox(height: AppTheme.space4),
          Text(
            '选择一个对话开始聊天',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 16),
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
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 14),
          ),
          const SizedBox(height: AppTheme.space2),
          Text(
            '在下方输入框开始对话',
            style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

class _PendingRelationsBanner extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).valueOrNull ?? const [];
    final relation = actions.cast<dynamic>().where((a) => a.action == 'propose_people_relations').toList();
    if (relation.isEmpty) return const SizedBox.shrink();
    final payload = jsonDecode(relation.first.argsJson) as Map<String, dynamic>;
    final people = (payload['people'] as List? ?? const [])
        .map((item) => (item as Map)['name']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .join('、');
    final targets = (payload['relations'] as List? ?? const [])
        .map((item) => (item as Map)['target']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .toSet()
        .join('、');
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(color: AppTheme.accentPrimary.withValues(alpha: .1), borderRadius: BorderRadius.circular(10)),
      child: Row(children: [
        Icon(Icons.auto_awesome_outlined, size: 18, color: AppTheme.accentPrimary),
        const SizedBox(width: 8),
        Expanded(child: Text('发现人物：$people\n关联事项：$targets\n回复“好”确认保存，回复“不要”忽略。', style: const TextStyle(fontSize: 12, height: 1.5))),
      ]),
    );
  }
}

/// 会话内临时提示气泡（AI 回复失败等）。仅内存态——不写库，
/// 不进入对话历史 / 记忆注入 / AI 上下文；发送下一条消息后即清除。
/// 只承载失败原因文案；「重新生成」入口在最后一条用户消息上。
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

class _MessageBubble extends StatelessWidget {
  final Message message;

  /// 是否显示「重新生成」：仅最后一条用户消息（且无 AI 回复）时为真
  final bool showRetry;
  final VoidCallback? onRetry;

  const _MessageBubble({
    required this.message,
    this.showRetry = false,
    this.onRetry,
  });

  @override
  Widget build(BuildContext context) {
    final isUser = message.isUser;

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
                    color: isUser
                        ? AppTheme.accentPrimary.withValues(alpha: 0.15)
                        : AppTheme.surface2,
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

                // 生成失败/未生成：最后一条用户消息上提供「重新生成」入口
                if (showRetry && onRetry != null)
                  Padding(
                    padding: const EdgeInsets.only(top: AppTheme.space1),
                    child: TextButton.icon(
                      onPressed: onRetry,
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

/// AI 回复生成中的占位气泡：用户发送后、回复入库前，让界面明确告知
/// 「后台正在执行」，避免发送后长时间无反馈的错觉。
class _AiTypingBubble extends StatelessWidget {
  const _AiTypingBubble();

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
              padding: const EdgeInsets.symmetric(
                horizontal: AppTheme.space3,
                vertical: AppTheme.space3,
              ),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.surface3, width: 1),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      color: AppTheme.accentPrimary,
                    ),
                  ),
                  const SizedBox(width: AppTheme.space2),
                  Text(
                    'AI 正在思考…',
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 13,
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

class _MessageInput extends ConsumerStatefulWidget {
  @override
  ConsumerState<_MessageInput> createState() => _MessageInputState();
}

class _MessageInputState extends ConsumerState<_MessageInput> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  bool _isSubmitting = false;
  String? _pendingSubmissionText;
  String? _pendingSubmissionKey;

  String _submissionKeyFor(String text) {
    if (_pendingSubmissionText == text && _pendingSubmissionKey != null) {
      return _pendingSubmissionKey!;
    }
    final random = Random.secure();
    final key =
        'ui-${DateTime.now().microsecondsSinceEpoch}-'
        '${random.nextInt(1 << 32).toRadixString(16)}';
    _pendingSubmissionText = text;
    _pendingSubmissionKey = key;
    return key;
  }

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
    // AI 正在生成本会话回复：暂不提交新消息，
    // 避免并发生成互相覆盖上下文（用户可继续打字，生成完后再发送）。
    if (ref.read(aiGeneratingProvider).contains(conversationId)) return;

    setState(() => _isSubmitting = true);
    final submissionKey = _submissionKeyFor(text);

    try {
      final uri = Uri.tryParse(text);
      final isUrl =
          uri != null &&
          (uri.scheme == 'http' || uri.scheme == 'https') &&
          uri.host.isNotEmpty &&
          !text.contains(RegExp(r'\s'));
      if (isUrl) {
        await _routeUrlToImportPreview(text, submissionKey);
        _controller.clear();
        _pendingSubmissionText = null;
        _pendingSubmissionKey = null;
        return;
      }
      final repo = ref.read(conversationRepositoryProvider);
      // 先把用户消息写入库（不依赖 AI）
      await repo.sendMessage(
        conversationId,
        text,
        idempotencyKey: submissionKey,
      );

      // 新消息发出，清掉该会话之前的临时错误提示
      clearConversationNotices(ref, conversationId);
      if (mounted) setState(() {});

      _controller.clear();
      _pendingSubmissionText = null;
      _pendingSubmissionKey = null;
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);
    } catch (e) {
      // 写库失败：明确反馈（文字仍保留在输入框，可手动重发）
      _showError('发送失败：$e');
      return;
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
      _focusNode.requestFocus();
    }

    if (!mounted) return;
    // 接着触发 AI 分析生成回复（不阻塞输入，出错单独反馈）。
    // 「生成中」状态由 _generateAiReply 内部设置，确保 invalidate
    // 时列表仍在 data 分支（可渲染 notice），而非 AsyncLoading（空跑）。
    await _generateAiReply(conversationId);
  }

  Future<void> _routeUrlToImportPreview(
    String url,
    String idempotencyKey,
  ) async {
    final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final input = await bridge.beginUrlInput(
      url,
      idempotencyKey: idempotencyKey,
    );
    try {
      final kind = await bridge.guessImportKind(url);
      if (kind == 'tweet') {
        final existing = await bridge.findTweetSourcePage(url);
        if (existing != null) {
          await bridge.finishUrlInput(input.id, wikiPageSlug: existing.slug);
          ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
          openWikiPageTab(ref, existing);
          return;
        }
        final fetch = (await bridge.fetchTweet(url)).withInputRecord(input.id);
        ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
        openWikiTweetTab(ref, fetch);
      } else {
        final fetch = (await bridge.fetchImportUrl(url))
            .withInputRecord(input.id);
        ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
        openWikiImportFetchTab(ref, fetch);
      }
    } catch (_) {
      await bridge.finishUrlInput(input.id, failed: true);
      rethrow;
    }
  }

  Future<void> _generateAiReply(String conversationId) async {
    // 生成逻辑集中在顶部 runAiGeneration：重试（错误气泡按钮）与首次发送
    // 走同一路径，保证成功/失败/收尾行为一致。
    await runAiGeneration(
      ref,
      conversationId,
      isMounted: () => mounted,
      onError: () {
        if (mounted) setState(() {});
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final selectedId = ref.watch(selectedConversationIdProvider);
    final isGenerating =
        selectedId != null &&
        ref.watch(aiGeneratingProvider).contains(selectedId);
    // 三态：写库中（_isSubmitting）或 AI 生成中 → 忙碌
    final busy = _isSubmitting || isGenerating;

    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Expanded(
            child: TextField(
              controller: _controller,
              focusNode: _focusNode,
              maxLines: 4,
              minLines: 1,
              enabled: !_isSubmitting,
              style: TextStyle(color: AppTheme.textPrimary, fontSize: 14),
              decoration: InputDecoration(
                hintText: isGenerating ? 'AI 正在思考，您可以先输入下一条消息…' : '输入消息...',
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
                  borderSide: BorderSide(
                    color: AppTheme.accentPrimary,
                    width: 2,
                  ),
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
            onPressed: busy ? null : _handleSubmit,
            icon: busy
                ? const SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.send),
            color: AppTheme.accentPrimary,
            iconSize: 24,
            tooltip: isGenerating ? 'AI 正在思考…' : '发送',
          ),
        ],
      ),
    );
  }
}

/// 写入系统剪贴板并给出轻量反馈
Future<void> _copyToClipboard(
  BuildContext context,
  String text,
  String label,
) async {
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
