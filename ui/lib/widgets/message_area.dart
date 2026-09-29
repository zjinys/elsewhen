import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';
import 'package:intl/intl.dart';

import '../models/conversation.dart';
import '../models/wiki_page.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../providers/todo_provider.dart';
import '../providers/goal_provider.dart';
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import 'markdown_view.dart';
import 'knowledge_panel.dart';
import 'todo_view.dart';
import 'goal_view.dart';

String? explicitTopicName(String text) {
  final match =
      RegExp(r'^/topic\s+(.+)$', caseSensitive: false).firstMatch(text) ??
      RegExp(r'^进入主题[：:]\s*(.+)$').firstMatch(text) ??
      RegExp(r'^#([^\s#].*)$').firstMatch(text);
  final name = match?.group(1)?.trim();
  return name == null || name.isEmpty ? null : name;
}

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
    clearReplyFailed(ref, conversationId);
    if (isMounted == null || isMounted()) {
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);
      ref.invalidate(pendingActionsProvider);
      // AI confirmation may have created or revised a wiki page/todo.
      // Refresh both navigation surfaces so the new object is visible
      // immediately after the assistant response completes.
      ref.invalidate(wikiPagesProvider);
      ref.invalidate(todosProvider);
      ref.invalidate(activeAiProviderProvider);
      ref.invalidate(todayTokenUsageProvider);
    }
  } catch (e) {
    // 失败补充界面（错误气泡 + 上滑）同样依赖 ref，卸载后调用会抛异常，
    // 与成功路径一致用 isMounted 守住。
    if (isMounted == null || isMounted()) {
      addConversationNotice(ref, conversationId, aiFailureNotice(e));
      ref.read(scrollRequestProvider.notifier).update((v) => v + 1);
      // 记下失败的是**哪一条**消息：重新生成的入口跟着消息 id 走，不再依赖
      // 列表位置——后续追加消息或搜索/日期筛选都会让位置条件失效（P9）。
      final failedMessageId = await latestUserMessageId(ref, conversationId);
      if (failedMessageId != null && (isMounted == null || isMounted())) {
        markReplyFailed(ref, conversationId, failedMessageId);
      }
      onError?.call();
    }
  } finally {
    // 缓存 notifier 是 StateHolder：异步 gap 后 ref 不可用，走 update 原子改写
    generatingNotifier.update(
      (current) => {...current}..remove(conversationId),
    );
  }
}

/// 会话里最后一条 user 消息的 id：生成失败时用它把「重新生成」入口钉在
/// 那条消息上（Rust 侧 generateReply 始终回复最新一条 user 消息）。
Future<String?> latestUserMessageId(
  WidgetRef ref,
  String conversationId,
) async {
  final messages = await ref
      .read(conversationRepositoryProvider)
      .getMessages(conversationId);
  for (final message in messages.reversed) {
    if (message.isUser) return message.id;
  }
  return null;
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
  final TextEditingController _searchController = TextEditingController();
  bool _expanded = false;
  int _shownSince = 0; // 展开后从消息开头跳过的条数
  bool _nearBottom = true;
  bool _pendingScrollToBottom = true; // 会话刚切换/初始加载后，数据到达时滚到最新
  String _searchQuery = '';
  DateTime? _selectedDate;
  bool _showSearch = false;

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
    _searchController.dispose();
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
    // 待办 / provider / token / 待入库草稿等头部统计一律由 _NowStatus 自己
    // 订阅：放在这里 watch 会让这些与消息无关的状态每次变化都重建整份消息
    // 列表（P13）。

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
        _NowStatus(
          messages: messagesAsync.value ?? const [],
          showSearch: _showSearch,
          onOpenDrafts: () => showDialog<void>(
            context: context,
            builder: (_) => const _KnowledgeDraftsDialog(),
          ),
          onOpenTodos: () => showDialog<void>(
            context: context,
            builder: (_) => Dialog(
              child: ConstrainedBox(
                constraints: const BoxConstraints(
                  maxWidth: 620,
                  maxHeight: 560,
                ),
                child: const SizedBox(
                  width: 620,
                  height: 560,
                  child: TodoListView(),
                ),
              ),
            ),
          ),
          onOpenGoals: () => showDialog<void>(
            context: context,
            builder: (_) => Dialog(
              child: ConstrainedBox(
                constraints: const BoxConstraints(
                  maxWidth: 560,
                  maxHeight: 560,
                ),
                child: const SizedBox(
                  width: 560,
                  height: 560,
                  // 目标条数上限 3，弹窗不会过高；仍包一层滚动以容纳历史列表。
                  child: SingleChildScrollView(child: GoalListView()),
                ),
              ),
            ),
          ),
          onOpenTopics: () =>
              ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki),
          onToggleSearch: () => setState(() => _showSearch = !_showSearch),
          // 复制全部并进「今天」行：有消息时显示，无消息时隐藏。
          onCopyAll: (messagesAsync.value?.isNotEmpty ?? false)
              ? () => _copyToClipboard(
                  context,
                  _formatConversationForCopy(messagesAsync.value!),
                  '对话',
                )
              : null,
        ),
        if (_showSearch) _buildSearchBar(messagesAsync.value ?? const []),
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
              final matching = messages.where((m) {
                if (_selectedDate != null &&
                    (m.createdAt.year != _selectedDate!.year ||
                        m.createdAt.month != _selectedDate!.month ||
                        m.createdAt.day != _selectedDate!.day)) {
                  return false;
                }
                if (_searchQuery.trim().isEmpty) return true;
                return m.isUser &&
                    m.content.toLowerCase().contains(
                      _searchQuery.trim().toLowerCase(),
                    );
              }).toList();
              final total = matching.length;
              final shownSince = _searchQuery.trim().isNotEmpty
                  ? 0
                  : _expanded
                  ? _shownSince
                  : (total > _windowSize ? total - _windowSize : 0);
              final hasMore = shownSince > 0;
              // 「重新生成」的目标 = 会话里最后一条 user 消息（Rust 侧
              // generateReply 始终回复最新那条 user 消息）。按 **id** 判定而
              // 不是「是不是列表最后一条」：列表尾部追加任何消息、或搜索/日期
              // 筛选把尾部挡掉，位置条件都会失效，失败就再也点不到（P9）。
              String? latestUserMessageId;
              for (final candidate in messages.reversed) {
                if (candidate.isUser) {
                  latestUserMessageId = candidate.id;
                  break;
                }
              }
              // 最后一条 user 消息是否已有 AI 回复：找到它之后继续往前找，
              // 遇见 assistant 即已回复（此时不应再提供「重新生成」）。
              // 没有这条判断，最后一条 user 消息即使已被回复仍会一直挂着
              // 「重新生成」按钮（message.id == latestUserMessageId 恒真）。
              var latestUserHasReply = false;
              if (latestUserMessageId != null) {
                for (var i = messages.length - 1; i >= 0; i--) {
                  final m = messages[i];
                  if (m.id == latestUserMessageId) break;
                  if (!m.isUser) {
                    latestUserHasReply = true;
                    break;
                  }
                }
              }
              // 只订阅本会话的失败标记，不订阅整张 map（避免无关会话的变化
              // 重建整份消息列表）。
              final failedReplyId = ref.watch(
                failedReplyMessageIdProvider.select((map) => map[selectedId]),
              );
              final tailCount = notices.length + (isAiTyping ? 1 : 0);
              final rows = <Object>[];
              String? previousDay;
              for (var i = shownSince; i < total; i++) {
                final message = matching[i];
                final day = DateFormat('yyyy年MM月dd日').format(message.createdAt);
                if (day != previousDay) {
                  rows.add(day);
                  previousDay = day;
                }
                rows.add(message);
              }

              return ListView.builder(
                controller: _scrollController,
                padding: const EdgeInsets.all(AppTheme.space4),
                itemCount: rows.length + (hasMore ? 1 : 0) + tailCount,
                itemBuilder: (context, index) {
                  if (hasMore && index == 0) {
                    return _buildLoadMoreButton(total);
                  }
                  final libIndex = index - (hasMore ? 1 : 0);
                  if (libIndex < rows.length) {
                    final row = rows[libIndex];
                    if (row is String) return _DayDivider(label: row);
                    final message = row as Message;
                    // 「重新生成」出现在**待回复的那条用户消息**上：会话最后一条
                    // user 消息（生成失败、或还没生成），或上一轮被记为失败的那条。
                    // 生成期间不显示（此时以「AI 正在思考…」占位反馈）。
                    final needsReply =
                        message.isUser &&
                        !isAiTyping &&
                        ((message.id == latestUserMessageId &&
                                !latestUserHasReply) ||
                            (failedReplyId != null &&
                                message.id == failedReplyId));
                    return _MessageBubble(
                      message: message,
                      showRetry: needsReply,
                      onRetry: () => _retryAiGeneration(selectedId),
                    );
                  }
                  final tailIndex = libIndex - rows.length;
                  if (tailIndex < notices.length) {
                    return _NoticeBubble(text: notices[tailIndex]);
                  }
                  return const _AiTypingBubble();
                },
              );
            },
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (error, stack) => Center(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  '加载消息失败\n$error',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
                ),
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

  Widget _buildSearchBar(List<Message> messages) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
      child: Row(
        children: [
          const Spacer(),
          IconButton(
            tooltip: '按日期定位',
            icon: Icon(
              Icons.calendar_today_outlined,
              size: 18,
              color: _selectedDate == null
                  ? AppTheme.textSecondary
                  : AppTheme.accentPrimary,
            ),
            onPressed: () => _pickDate(messages),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: TextField(
              controller: _searchController,
              decoration: InputDecoration(
                isDense: true,
                hintText: '搜索你说过的话',
                hintStyle: TextStyle(color: AppTheme.textTertiary),
                prefixIcon: Icon(
                  Icons.search,
                  size: 18,
                  color: AppTheme.textSecondary,
                ),
                suffixIcon: (_searchQuery.isNotEmpty || _selectedDate != null)
                    ? IconButton(
                        tooltip: '清除定位',
                        icon: const Icon(Icons.clear, size: 17),
                        onPressed: _clearLocation,
                      )
                    : null,
                filled: true,
                fillColor: AppTheme.surface1,
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
                  borderSide: BorderSide(color: AppTheme.accentPrimary),
                ),
              ),
              onChanged: (value) => setState(() {
                _searchQuery = value;
                _expanded = true;
                _shownSince = 0;
              }),
            ),
          ),
          const SizedBox(width: 8),
          IconButton(
            tooltip: '回到现在',
            icon: Icon(
              Icons.vertical_align_bottom,
              size: 18,
              color: AppTheme.textSecondary,
            ),
            onPressed: _returnToNow,
          ),
        ],
      ),
    );
  }

  Future<void> _pickDate(List<Message> messages) async {
    final now = DateTime.now();
    final dates = messages.map((m) => m.createdAt).toList()..sort();
    final first = dates.isEmpty ? DateTime(now.year - 1) : dates.first;
    final picked = await showDatePicker(
      context: context,
      initialDate: _selectedDate ?? now,
      firstDate: DateTime(first.year, first.month, first.day),
      lastDate: DateTime(now.year, now.month, now.day),
      helpText: '跳到日期',
      cancelText: '取消',
      confirmText: '跳转',
    );
    if (picked != null && mounted) setState(() => _selectedDate = picked);
  }

  void _clearLocation() {
    _searchController.clear();
    setState(() {
      _searchQuery = '';
      _selectedDate = null;
    });
  }

  void _returnToNow() {
    _clearLocation();
    _scheduleScrollToBottom();
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

class _KnowledgeDraftsDialog extends ConsumerWidget {
  const _KnowledgeDraftsDialog();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final drafts = actions.where(
      (action) => action.action == 'save_knowledge_draft',
    );
    final conversationId = ref.watch(selectedConversationIdProvider);
    return AlertDialog(
      title: Text('待入库草稿（${drafts.length}）'),
      content: SizedBox(
        width: 540,
        height: min(440, MediaQuery.sizeOf(context).height * .55),
        child: drafts.isEmpty
            ? const Center(child: Text('当前对话没有待入库草稿'))
            : ListView(
                children: [
                  for (final action in drafts)
                    Builder(
                      builder: (context) {
                        Map<String, dynamic> payload;
                        try {
                          payload = Map<String, dynamic>.from(
                            jsonDecode(action.argsJson) as Map,
                          );
                        } catch (_) {
                          return const SizedBox.shrink();
                        }
                        final title = payload['title']?.toString() ?? '';
                        return ListTile(
                          leading: const Icon(Icons.article_outlined),
                          title: Text(
                            title,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                          subtitle: const Text('待确认，未入库'),
                          trailing: const Icon(Icons.chevron_right),
                          onTap: conversationId == null
                              ? null
                              : () => showDialog<void>(
                                  context: context,
                                  builder: (_) => _KnowledgeDraftDialog(
                                    conversationId: conversationId,
                                    actionId: action.id,
                                    title: title,
                                    kind:
                                        payload['kind']?.toString() ?? 'topic',
                                    content:
                                        payload['content_md']?.toString() ?? '',
                                  ),
                                ),
                        );
                      },
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('关闭'),
        ),
      ],
    );
  }
}

class _KnowledgeDraftDialog extends ConsumerStatefulWidget {
  const _KnowledgeDraftDialog({
    required this.conversationId,
    required this.actionId,
    required this.title,
    required this.kind,
    required this.content,
  });

  final String conversationId;
  final String actionId;
  final String title;
  final String kind;
  final String content;

  @override
  ConsumerState<_KnowledgeDraftDialog> createState() =>
      _KnowledgeDraftDialogState();
}

class _KnowledgeDraftDialogState extends ConsumerState<_KnowledgeDraftDialog> {
  bool _saving = false;
  String? _error;

  Future<void> _delete() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('删除草稿？'),
        content: Text('《${widget.title}》将不再等待入库。已保存的知识页不会受到影响。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('删除草稿'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(conversationRepositoryProvider)
          .declineKnowledgeDraft(widget.conversationId, widget.actionId);
      if (!mounted) return;
      ref.invalidate(pendingActionsProvider);
      Navigator.of(context).pop();
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '删除失败：$e';
          _saving = false;
        });
      }
    }
  }

  Future<void> _save() async {
    if (_saving) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final result = await ref
          .read(conversationRepositoryProvider)
          .confirmKnowledgeDraft(widget.conversationId, widget.actionId);
      if (!mounted) return;
      ref.invalidate(pendingActionsProvider);
      ref.invalidate(wikiPagesProvider);
      final messenger = ScaffoldMessenger.of(context);
      Navigator.of(context).pop();
      messenger.showSnackBar(SnackBar(content: Text(result)));
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '保存失败：$e';
          _saving = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.title),
    content: SizedBox(
      width: 620,
      height: min(480, MediaQuery.sizeOf(context).height * .65),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '类型：${widget.kind} · 待确认，未入库',
            style: TextStyle(color: AppTheme.textSecondary),
          ),
          const SizedBox(height: 12),
          Expanded(
            child: SingleChildScrollView(
              child: MarkdownView(markdown: widget.content),
            ),
          ),
          if (_error != null)
            Text(
              _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
        ],
      ),
    ),
    actions: [
      TextButton.icon(
        onPressed: _saving ? null : _delete,
        icon: const Icon(Icons.delete_outline),
        label: const Text('删除草稿'),
      ),
      TextButton(
        onPressed: _saving ? null : () => Navigator.of(context).pop(),
        child: const Text('返回'),
      ),
      FilledButton.icon(
        onPressed: _saving ? null : _save,
        icon: _saving
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.save_outlined),
        label: const Text('保存到知识库'),
      ),
    ],
  );
}

class _PendingRelationsBanner extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final relation = actions
        .cast<dynamic>()
        .where((a) => a.action == 'propose_people_relations')
        .toList();
    if (relation.isEmpty) return const SizedBox.shrink();
    // argsJson 来自 DB，可能因旧数据/截断/版本迁移而非法：build 中 bare
    // jsonDecode 一旦抛错会拖垮整个消息列表渲染，解析失败改为渲染占位。
    Map<String, dynamic> payload = const <String, dynamic>{};
    try {
      final decoded = jsonDecode(relation.first.argsJson);
      if (decoded is Map<String, dynamic>) {
        payload = decoded;
      }
    } catch (_) {
      // 保持默认空 payload：badge 渲染"无法解析"占位，不崩消息列表。
    }
    final people = (payload['people'] as List? ?? const [])
        .map((item) => (item as Map)['name']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .join('、');
    final targets = (payload['relations'] as List? ?? const [])
        .map((item) => (item as Map)['target']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .toSet()
        .join('、');
    final degraded = payload.isEmpty && people.isEmpty && targets.isEmpty;
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: .1),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Icon(
            Icons.auto_awesome_outlined,
            size: 18,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              degraded
                  ? '有一条人物关联建议，但数据无法解析。'
                  : '发现人物：$people\n关联事项：$targets\n回复“好”确认保存，回复“不要”忽略。',
              style: const TextStyle(fontSize: 12, height: 1.5),
            ),
          ),
          if (!degraded && _hasAmbiguity(ref, payload))
            TextButton(
              onPressed: () => _resolveAmbiguity(context, ref, relation.first),
              child: const Text('选择'),
            ),
        ],
      ),
    );
  }

  bool _hasAmbiguity(WidgetRef ref, Map<String, dynamic> payload) {
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    final people = (payload['people'] as List? ?? const []).map(
      (item) => (item as Map)['name']?.toString() ?? '',
    );
    final targets = (payload['relations'] as List? ?? const []).map(
      (item) => (item as Map)['target']?.toString() ?? '',
    );
    return [...people, ...targets].any(
      (name) =>
          pages
              .where(
                (page) =>
                    page.title.trim().toLowerCase() ==
                    name.trim().toLowerCase(),
              )
              .length >
          1,
    );
  }

  /// 人物/事项同名歧义的选择流程（P19）：
  /// - `argsJson` 来自库里存的动作参数，历史脏数据或未来方言都可能让它不是
  ///   JSON 对象——直接 `jsonDecode` 抛出会把整条流程炸掉，这里收口成提示。
  /// - 选择全部收集完再**一次性**落库：中途 return 会让用户已经点过的选择被
  ///   静默丢弃，所以取消时明确提示「未保存」，而不是无声退出。
  /// - 每个对话框前查 `mounted`：前一个对话框本身就是一次异步 gap。
  Future<void> _resolveAmbiguity(
    BuildContext context,
    WidgetRef ref,
    dynamic action,
  ) async {
    final Map<String, dynamic> payload;
    try {
      final decoded = jsonDecode(action.argsJson);
      if (decoded is! Map) {
        _notifyAmbiguity(context, '动作参数格式异常，无法选择人物');
        return;
      }
      payload = Map<String, dynamic>.from(decoded);
    } catch (_) {
      _notifyAmbiguity(context, '动作参数无法解析，无法选择人物');
      return;
    }
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    final relations = (payload['relations'] as List? ?? const [])
        .map((item) => Map<String, dynamic>.from(item as Map))
        .toList();
    for (final relation in relations) {
      final person = relation['person']?.toString() ?? '';
      final target = relation['target']?.toString() ?? '';
      final peopleMatches = pages
          .where(
            (page) =>
                page.title.trim().toLowerCase() == person.trim().toLowerCase(),
          )
          .toList();
      if (peopleMatches.length > 1) {
        if (!context.mounted) return;
        final selected = await _pickPage(
          context,
          '选择人物「$person」',
          peopleMatches,
        );
        if (selected == null) {
          if (!context.mounted) return;
          _notifyAmbiguity(context, '已取消选择，未保存本次修改');
          return;
        }
        relation['from_slug'] = selected.slug;
      }
      final targetMatches = pages
          .where(
            (page) =>
                page.title.trim().toLowerCase() == target.trim().toLowerCase(),
          )
          .toList();
      if (targetMatches.length > 1) {
        if (!context.mounted) return;
        final selected = await _pickPage(
          context,
          '选择事项「$target」',
          targetMatches,
        );
        if (selected == null) {
          if (!context.mounted) return;
          _notifyAmbiguity(context, '已取消选择，未保存本次修改');
          return;
        }
        relation['to_slug'] = selected.slug;
      }
    }
    if (!context.mounted) return;
    payload['relations'] = relations;
    await ref
        .read(conversationRepositoryProvider)
        .updatePendingActionArgs(action.id, jsonEncode(payload));
    if (!context.mounted) return;
    ref.invalidate(pendingActionsProvider);
  }

  /// 歧义选择流程的失败/取消提示（context 失效后静默丢弃，避免用已销毁的 context）。
  void _notifyAmbiguity(BuildContext context, String message) {
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(message)));
  }

  Future<WikiPage?> _pickPage(
    BuildContext context,
    String title,
    List<WikiPage> pages,
  ) {
    return showDialog<WikiPage>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final page in pages)
                ListTile(
                  title: Text(page.title),
                  subtitle: Text(
                    '${page.kindLabel} · ${page.summary}',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  onTap: () => Navigator.pop(dialogContext, page),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('取消'),
          ),
        ],
      ),
    );
  }
}

/// 会话内临时提示气泡（AI 回复失败等）。仅内存态——不写库，
/// 不进入对话历史 / 记忆注入 / AI 上下文；发送下一条消息后即清除。
/// 只承载失败原因文案；「重新生成」入口在最后一条用户消息上。
class _DayDivider extends StatelessWidget {
  final String label;

  const _DayDivider({required this.label});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space4),
      child: Row(
        children: [
          Expanded(child: Divider(color: AppTheme.surface3)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
            child: Text(
              label,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          ),
          Expanded(child: Divider(color: AppTheme.surface3)),
        ],
      ),
    );
  }
}

/// 会话顶部状态条。统计项（待办 / 话题 / provider / token / 待入库草稿）
/// 一律在**这里**订阅，而不是在消息列表所在的 State 里 watch：它们与消息
/// 内容无关，放在父级 build 里会让每次 todo/wiki/token 变化都重建整份消息
/// 列表（P13）。
class _NowStatus extends ConsumerWidget {
  final List<Message> messages;
  final bool showSearch;
  final VoidCallback? onOpenDrafts;
  final VoidCallback? onOpenTodos;
  final VoidCallback? onOpenGoals;
  final VoidCallback? onOpenTopics;
  final VoidCallback? onToggleSearch;
  final VoidCallback? onCopyAll;
  const _NowStatus({
    required this.messages,
    this.showSearch = false,
    this.onOpenDrafts,
    this.onOpenTodos,
    this.onOpenGoals,
    this.onOpenTopics,
    this.onToggleSearch,
    this.onCopyAll,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
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
    final summary = '对话${count}次';
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
          final compact = constraints.maxWidth < 660;
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
                    //const SizedBox(width: 14),
                    // TextButton(
                    //   onPressed: onOpenTopics,
                    //   style: TextButton.styleFrom(
                    //     visualDensity: VisualDensity.compact,
                    //   ),
                    //   child: Text(
                    //     '主题 (${topicCreatedToday ?? 0}/${topicUpdatedToday ?? 0}）',
                    //     style: TextStyle(
                    //       fontSize: 12,
                    //       color: AppTheme.textSecondary,
                    //     ),
                    //   ),
                    // ),
                    const SizedBox(width: 6),
                    TextButton.icon(
                      onPressed: onOpenTodos,
                      icon: const Icon(Icons.check_circle_outlined, size: 16),
                      label: Text(
                        '待办${openTodos ?? 0}项',
                        // style: TextStyle(
                        //   fontSize: 12,
                        //   color: AppTheme.textSecondary,
                        // ),
                      ),
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                      ),
                    ),
                    const Spacer(),
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
                    if (tokenUsage != null) ...[
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
                            'Token${_formatTokens(tokenUsage.totalTokens)}',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                        ],
                      ),
                    ],
                    const SizedBox(width: 12),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (activeProvider != null) ...[
                          Icon(
                            Icons.smart_toy_outlined,
                            size: 14,
                            color: AppTheme.textTertiary,
                          ),
                          const SizedBox(width: 5),
                          Text(
                            activeProvider,
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                          const SizedBox(width: 12),
                        ],
                        //const SizedBox(width: 8),
                        if (onCopyAll != null)
                          IconButton(
                            tooltip: '复制全部对话',
                            visualDensity: VisualDensity.compact,
                            icon: const Icon(Icons.copy_all_rounded, size: 17),
                            color: AppTheme.textSecondary,
                            onPressed: onCopyAll,
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
                  padding: const EdgeInsets.only(top: 2),
                  child: Row(
                    children: [
                      // 窄屏时目标入口落在第二行：它没有独立一级 tab，这里是唯一入口，
                      // 空态也必须可见（故空态只显示「目标」，且用 accent 着色）。
                      TextButton.icon(
                        onPressed: onOpenGoals,
                        icon: Icon(
                          Icons.flag_outlined,
                          size: 14,
                          color: activeGoals.isEmpty
                              ? AppTheme.accentPrimary
                              : AppTheme.textTertiary,
                        ),
                        label: Text(
                          goalLabel,
                          style: TextStyle(
                            fontSize: 12,
                            color: activeGoals.isEmpty
                                ? AppTheme.accentPrimary
                                : AppTheme.textTertiary,
                          ),
                        ),
                        style: TextButton.styleFrom(
                          visualDensity: VisualDensity.compact,
                          padding: const EdgeInsets.symmetric(horizontal: 6),
                        ),
                      ),
                      if (activeProvider != null) ...[
                        const SizedBox(width: 8),
                        Icon(
                          Icons.smart_toy_outlined,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 5),
                        Flexible(
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
                        const SizedBox(width: 10),
                      ],
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
                      if (tokenUsage != null) ...[
                        const SizedBox(width: 10),
                        Icon(
                          Icons.data_usage_outlined,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                        const SizedBox(width: 5),
                        Text(
                          'Token ${_formatTokens(tokenUsage.totalTokens)}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ],
                      const Spacer(),
                      if (onCopyAll != null)
                        IconButton(
                          tooltip: '复制全部对话',
                          visualDensity: VisualDensity.compact,
                          icon: const Icon(Icons.copy_all_rounded, size: 17),
                          color: AppTheme.textSecondary,
                          onPressed: onCopyAll,
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

class _MessageBubble extends ConsumerWidget {
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
  Widget build(BuildContext context, WidgetRef ref) {
    final isUser = message.isUser;
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
                        if (isUser) _buildRecordability(context, ref),
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
                    child: isUser
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
                          ),
                  ),

                  // 生成失败/未生成：最后一条用户消息上提供「重新生成」入口
                  if (!isUser && message.content.contains('[['))
                    KnowledgeCitationsButton(messageId: message.id),
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
          ),

          if (isUser) ...[
            const SizedBox(width: AppTheme.space3),
            _buildAvatar(isUser: true),
          ],
        ],
      ),
    );
  }

  Widget _buildRecordability(BuildContext context, WidgetRef ref) {
    final state = ref.watch(messageRecordabilityProvider(message.id));
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
      ref.invalidate(messageRecordabilityProvider(message.id));
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
      if (context.mounted)
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('操作失败：$error')));
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
  bool _showInputGuide = false;
  String? _completionMarker;
  String _completionQuery = '';
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
    _controller.addListener(_updateCompletion);
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
    _controller.removeListener(_updateCompletion);
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _updateCompletion() {
    final cursor = _controller.selection.baseOffset;
    if (cursor < 0 || cursor > _controller.text.length) return;
    final beforeCursor = _controller.text.substring(0, cursor);
    final match = RegExp(r'(?:^|\s)([@#])([^\s@#]*)$').firstMatch(beforeCursor);
    final marker = match?.group(1);
    final query = match?.group(2) ?? '';
    if (marker == _completionMarker && query == _completionQuery) return;
    setState(() {
      _completionMarker = marker;
      _completionQuery = query;
    });
  }

  void _applyCompletion(String title) {
    final cursor = _controller.selection.baseOffset;
    final marker = _completionMarker;
    if (cursor < 0 || marker == null) return;
    final replacementStart = cursor - _completionQuery.length - 1;
    final next = _controller.text.replaceRange(
      replacementStart,
      cursor,
      '$marker$title ',
    );
    final nextCursor = replacementStart + marker.length + title.length + 1;
    _controller.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: nextCursor),
    );
    _focusNode.requestFocus();
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

  void _insertGuideExample(String selection) {
    _controller.text = selection;
    _controller.selection = TextSelection.collapsed(offset: selection.length);
    setState(() => _showInputGuide = false);
    _focusNode.requestFocus();
  }

  Future<void> _pickDirectoryForImport() async {
    final path = await FilePicker.getDirectoryPath(dialogTitle: '选择要导入的目录');
    if (!mounted || path == null || path.trim().isEmpty) return;
    final prompt = '看看 `$path` 下的项目，导入到知识库';
    _controller
      ..text = prompt
      ..selection = TextSelection.collapsed(offset: prompt.length);
    _focusNode.requestFocus();
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
      final topic = explicitTopicName(text);
      if (topic != null) {
        await _enterTopic(topic);
        _controller.clear();
        _pendingSubmissionText = null;
        _pendingSubmissionKey = null;
        return;
      }
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
      // 新的 user 消息取代了上一轮的失败目标：清掉标记，避免「重新生成」
      // 入口继续挂在那条已被后续消息取代的消息上（P9）。
      clearReplyFailed(ref, conversationId);
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
        // FocusNode 与 State 同生命周期：卸载后 requestFocus 会抛
        // "A FocusNode was used after being disposed"，必须与 mounted 同守卫。
        _focusNode.requestFocus();
      }
    }

    if (!mounted) return;
    // 接着触发 AI 分析生成回复（不阻塞输入，出错单独反馈）。
    // 「生成中」状态由 _generateAiReply 内部设置，确保 invalidate
    // 时列表仍在 data 分支（可渲染 notice），而非 AsyncLoading（空跑）。
    await _generateAiReply(conversationId);
  }

  Future<void> _enterTopic(String title) async {
    final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    WikiPage? existing;
    for (final page in pages) {
      if (page.kind == 'topic' &&
          page.title.toLowerCase() == title.toLowerCase()) {
        existing = page;
        break;
      }
    }
    final page =
        existing ??
        await bridge.saveTextPage(
          text: '# $title\n\n',
          title: title,
          tags: const ['topic'],
        );
    ref.invalidate(wikiPagesProvider);
    ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
    openWikiPageTab(ref, page);
    if (mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('已进入主题：${page.title}')));
    }
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
          ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
          openWikiPageTab(ref, existing);
          return;
        }
        final fetch = (await bridge.fetchTweet(url)).withInputRecord(input.id);
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
        openWikiTweetTab(ref, fetch);
      } else {
        final fetch = (await bridge.fetchImportUrl(url))
            .withInputRecord(input.id);
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
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
    final pages = ref.watch(wikiPagesProvider).value ?? const [];
    final completionPages = _completionMarker == null
        ? const <WikiPage>[]
        : pages
              .where((page) {
                final allowed = _completionMarker == '@'
                    ? page.kind == 'person'
                    : page.kind == 'topic' || page.kind == 'project';
                return allowed &&
                    page.status != 'archived' &&
                    page.title.toLowerCase().contains(
                      _completionQuery.toLowerCase(),
                    );
              })
              .take(6)
              .toList();

    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (completionPages.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: AppTheme.space2),
              child: _InputCompletionPanel(
                marker: _completionMarker!,
                pages: completionPages,
                onSelected: _applyCompletion,
              ),
            ),
          AnimatedSize(
            duration: const Duration(milliseconds: 160),
            curve: Curves.easeOut,
            child: _showInputGuide
                ? Padding(
                    padding: const EdgeInsets.only(bottom: AppTheme.space3),
                    child: _InputGuidePanel(onInsert: _insertGuideExample),
                  )
                : const SizedBox.shrink(),
          ),
          Row(
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
                    prefixIcon: _showInputGuide
                        ? IconButton(
                            tooltip: '关闭输入格式',
                            icon: const Icon(Icons.close, size: 20),
                            color: AppTheme.textSecondary,
                            onPressed: () =>
                                setState(() => _showInputGuide = false),
                          )
                        : PopupMenuButton<String>(
                            tooltip: '添加内容',
                            icon: const Icon(
                              Icons.add_circle_outline,
                              size: 20,
                            ),
                            onSelected: (value) {
                              if (value == 'directory') {
                                _pickDirectoryForImport();
                              } else {
                                setState(() => _showInputGuide = true);
                              }
                            },
                            itemBuilder: (_) => const [
                              PopupMenuItem(
                                value: 'directory',
                                child: ListTile(
                                  contentPadding: EdgeInsets.zero,
                                  leading: Icon(
                                    Icons.folder_open_outlined,
                                    size: 18,
                                  ),
                                  title: Text('选择目录'),
                                ),
                              ),
                              PopupMenuItem(
                                value: 'guide',
                                child: ListTile(
                                  contentPadding: EdgeInsets.zero,
                                  leading: Icon(Icons.help_outline, size: 18),
                                  title: Text('查看输入格式'),
                                ),
                              ),
                            ],
                          ),
                    hintStyle: TextStyle(color: AppTheme.textTertiary),
                    filled: true,
                    fillColor: AppTheme.surface2,
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(
                        AppTheme.radiusMedium,
                      ),
                      borderSide: BorderSide(color: AppTheme.surface3),
                    ),
                    enabledBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(
                        AppTheme.radiusMedium,
                      ),
                      borderSide: BorderSide(color: AppTheme.surface3),
                    ),
                    focusedBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(
                        AppTheme.radiusMedium,
                      ),
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
              IconButton.filled(
                onPressed: busy ? null : _handleSubmit,
                style: IconButton.styleFrom(
                  backgroundColor: AppTheme.accentPrimary,
                  disabledBackgroundColor: AppTheme.surface3,
                ),
                icon: busy
                    ? const SizedBox(
                        width: 20,
                        height: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.arrow_upward, size: 16),
                tooltip: isGenerating ? 'AI 正在思考…' : '发送',
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _InputGuidePanel extends StatelessWidget {
  final ValueChanged<String> onInsert;

  const _InputGuidePanel({required this.onInsert});

  @override
  Widget build(BuildContext context) {
    const items = [
      (Icons.person_outline, '@人名', '标注人物', '@张伟 '),
      (Icons.tag_outlined, '#主题名', '进入主题', '#付款流程'),
      (Icons.link_outlined, '链接', '导入内容', 'https://'),
      (
        Icons.folder_open_outlined,
        '目录',
        '导入项目或文件',
        '看看 `/path/to/project` 下的项目，导入到知识库',
      ),
      (Icons.check_circle_outline, '确认 / 取消', '处理 AI 草案', '确认'),
    ];
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final item in items)
            _InputGuideItem(
              icon: item.$1,
              title: item.$2,
              description: item.$3,
              onTap: () => onInsert(item.$4),
            ),
        ],
      ),
    );
  }
}

class _InputCompletionPanel extends StatelessWidget {
  final String marker;
  final List<WikiPage> pages;
  final ValueChanged<String> onSelected;

  const _InputCompletionPanel({
    required this.marker,
    required this.pages,
    required this.onSelected,
  });

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerLeft,
      child: Material(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        child: Container(
          constraints: const BoxConstraints(maxWidth: 420),
          decoration: BoxDecoration(
            border: Border.all(color: AppTheme.surface3),
            borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final page in pages)
                InkWell(
                  onTap: () => onSelected(page.title),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 8,
                    ),
                    child: Row(
                      children: [
                        Expanded(
                          child: Text(
                            '$marker${page.title}',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(color: AppTheme.textPrimary),
                          ),
                        ),
                        const SizedBox(width: 12),
                        Text(
                          page.kindLabel,
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _InputGuideItem extends StatelessWidget {
  final IconData icon;
  final String title;
  final String description;
  final VoidCallback onTap;

  const _InputGuideItem({
    required this.icon,
    required this.title,
    required this.description,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 17, color: AppTheme.accentPrimary),
              const SizedBox(width: 6),
              Text(title, style: TextStyle(color: AppTheme.textPrimary)),
              const SizedBox(width: 6),
              Text(
                description,
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
            ],
          ),
        ),
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
