import 'dart:io';

import 'package:flutter/foundation.dart';

import 'app_pressable.dart';
import 'glass.dart';
import 'suggestion_feedback.dart';
import 'save_topic_dialog.dart';
// ignore: unused_import — 翻篇入口暂隐藏，放量时恢复
import 'page_mark_review.dart';
import 'chat_proposal_review.dart';
import 'chat_run_status.dart';
import 'chat_input_queue.dart';
import '../providers/chat_run_provider.dart';
import '../bridge/api.dart' as api;
import '../bridge/api.dart' show ChatInputMode;

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
import '../providers/state_holder.dart';
import '../providers/wiki_provider.dart';
import '../providers/todo_provider.dart';
import '../providers/goal_provider.dart';
import '../utils/error_report.dart';
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import 'markdown_view.dart';
import 'knowledge_panel.dart';
import 'todo_view.dart';
import 'goal_view.dart';
import 'reminder_banner.dart';

// ── 消息区拆分为 part（行为零变化，见 message_area_parts/）──
part 'message_area_parts/drafts_sheets.dart';
part 'message_area_parts/pending_banner.dart';
part 'message_area_parts/now_status.dart';
part 'message_area_parts/message_bubble.dart';
part 'message_area_parts/input.dart';
part 'message_area_parts/input_panels.dart';
part 'message_area_parts/scroll_search.dart';
const int _windowSize = 20;
const int _pageSize = 20;

/// 会话内搜索开关：放 provider 层，标题栏（MessageArea 之外）也能切换。
/// 不是全局单例——family 按 conversationId 区分，各会话互不干扰。
final messageSearchVisibleProvider = NotifierProvider.autoDispose
    .family<StateHolder<bool>, bool, String>(
      (conversationId) => StateHolder<bool>(false),
    );

/// 「存为主题」预填指令：AI 气泡悬浮按钮点一下写入这里，
/// _MessageInput 监听后填进输入框（不直接发送，用户可改可发）。
/// 比让按钮直接调工具更透明：用户看到将要发什么，草稿仍走确认门。
final saveTopicDraftProvider = NotifierProvider.autoDispose
    .family<StateHolder<String?>, String?, String>(
      (conversationId) => StateHolder<String?>(null),
    );

String? explicitTopicName(String text) {
  final match =
      RegExp(r'^/topic\s+(.+)$', caseSensitive: false).firstMatch(text) ??
      RegExp(r'^进入主题[：:]\s*(.+)$').firstMatch(text) ??
      RegExp(r'^#([^\s#].*)$').firstMatch(text);
  final name = match?.group(1)?.trim();
  return name == null || name.isEmpty ? null : name;
}

/// 发送与重试均绑定已保存的输入；真实 bridge 交给共享持久 run 控制器。
/// 未初始化 bridge 的内存测试仓库保留同步应答兼容路径。
Future<void> runAiGeneration(
  WidgetRef ref,
  String conversationId, {
  required String inputMessageId,
  bool Function()? isMounted,
  VoidCallback? onError,
}) async {
  final repository = ref.read(conversationRepositoryProvider);
  if (repository.supportsChatRuns) {
    await ref
        .read(chatRunControllerProvider(conversationId))
        .start(inputMessageId);
    return;
  }
  final generatingNotifier = ref.read(aiGeneratingProvider.notifier);
  try {
    setAiGenerating(ref, conversationId, true);
    final repo = ref.read(conversationRepositoryProvider);
    await repo.generateReply(conversationId);
    if (isMounted == null || isMounted()) {
      clearReplyFailed(ref, conversationId);
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
      markReplyFailed(ref, conversationId, inputMessageId);
      onError?.call();
    }
  } finally {
    // 缓存 notifier 是 StateHolder：异步 gap 后 ref 不可用，走 update 原子改写
    generatingNotifier.update(
      (current) => {...current}..remove(conversationId),
    );
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

  final detail = stripErrorShell(e, stripExceptionPrefix: false);
  return 'AI 回复失败：$detail';
}

bool get _isMobilePlatform => !kIsWeb && (Platform.isAndroid || Platform.isIOS);

/// 移动端用 bottom sheet 呈现面板（Material 惯例：大屏对话框在小屏上
/// 上下留白多、圆角悬浮感重）；桌面端保持居中对话框。
Future<void> _showAdaptivePanel(
  BuildContext context, {
  required Widget dialog,
  required Widget Function(BuildContext) sheetBuilder,
}) {
  if (_isMobilePlatform) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      backgroundColor: AppTheme.surface0,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: sheetBuilder,
    );
  }
  return showDialog<void>(context: context, builder: (_) => dialog);
}

/// 待入库草稿面板。抽成顶层函数供标题栏「今天」操作行复用。
void openKnowledgeDraftsDialog(BuildContext context) {
  _showAdaptivePanel(
    context,
    dialog: const _KnowledgeDraftsDialog(),
    sheetBuilder: (sheetContext) => DraggableScrollableSheet(
      initialChildSize: 0.7,
      minChildSize: 0.4,
      maxChildSize: 0.95,
      expand: false,
      builder: (context, scrollController) =>
          _KnowledgeDraftsSheet(scrollController: scrollController),
    ),
  );
}

void openTodoDialog(BuildContext context) {
  _showAdaptivePanel(
    context,
    dialog: Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 620, maxHeight: 560),
        child: const SizedBox(width: 620, height: 560, child: TodoListView()),
      ),
    ),
    sheetBuilder: (sheetContext) =>
        _PanelSheet(title: '待办', builder: (context) => const TodoListView()),
  );
}

void openGoalDialog(BuildContext context) {
  _showAdaptivePanel(
    context,
    dialog: Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560, maxHeight: 560),
        child: const SizedBox(
          width: 560,
          height: 560,
          // 目标条数上限 3，弹窗不会过高；仍包一层滚动以容纳历史列表。
          child: SingleChildScrollView(child: GoalListView()),
        ),
      ),
    ),
    sheetBuilder: (sheetContext) => _PanelSheet(
      title: '目标',
      // 目标条数上限 3，面板不会过高；仍包一层滚动以容纳历史列表。
      builder: (context) => const SingleChildScrollView(child: GoalListView()),
    ),
  );
}

class MessageArea extends ConsumerStatefulWidget {
  const MessageArea({super.key});

  @override
  ConsumerState<MessageArea> createState() => _MessageAreaState();
}

class _MessageAreaState extends ConsumerState<MessageArea> {
  /// 初始窗口与每页加载量：只显示最近 N 条，旧的通过「显示更早」逐步展开

  final ScrollController _scrollController = ScrollController();
  final TextEditingController _searchController = TextEditingController();
  bool _expanded = false;
  int _shownSince = 0; // 展开后从消息开头跳过的条数
  bool _nearBottom = true;
  bool _pendingScrollToBottom = true; // 会话刚切换/初始加载后，数据到达时滚到最新
  String _searchQuery = '';
  DateTime? _selectedDate;
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
    final showSearch =
        selectedId != null &&
        ref.watch(messageSearchVisibleProvider(selectedId));
    // 正在生成 AI 回复的会话（发送后反馈「AI 生成中」占位气泡）
    final generatingIds = ref.watch(aiGeneratingProvider);
    final durable = ref.watch(conversationRepositoryProvider).supportsChatRuns;
    final durableBusy =
        selectedId != null &&
        durable &&
        ref.watch(
          chatRunStateProvider(selectedId)
              .select((value) => value.value?.busy ?? true),
        );
    final durableRunInput = selectedId != null && durable
        ? ref.watch(
            chatRunStateProvider(selectedId)
                .select((value) => value.value?.run?.inputMessageId),
          )
        : null;
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
          conversationId: selectedId,
          messages: messagesAsync.value ?? const [],
          showSearch: showSearch,
          onOpenDrafts: () => openKnowledgeDraftsDialog(context),
          // 翻篇记录入口暂隐藏：与「翻篇到此」同批放量。
          onOpenPageMarks: null,
          onOpenTodos: () => openTodoDialog(context),
          onOpenGoals: () => openGoalDialog(context),
          onToggleSearch: () => ref
              .read(messageSearchVisibleProvider(selectedId).notifier)
              .set(!showSearch),
          // 复制全部并进「今天」行：有消息时显示，无消息时隐藏。
          onCopyAll: (messagesAsync.value?.isNotEmpty ?? false)
              ? () => _copyToClipboard(
                  context,
                  _formatConversationForCopy(messagesAsync.value!),
                  '对话',
                )
              : null,
        ),
        const ReminderBanner(),
        if (showSearch) _buildSearchBar(messagesAsync.value ?? const []),
        _PendingRelationsBanner(),
        // Messages list
        Expanded(
          child: messagesAsync.when(
            // reload（同步到达重查/本地 invalidate）时沿用旧数据继续渲染：
            // 否则列表被拆成 loading 再装回，滚动位置丢到顶部——
            // 「多端同步后对话定位丢失/跳一下」的元凶之一。跨会话的
            // 残留数据由下方 stale 守卫识别降级。
            skipLoadingOnReload: true,
            data: (messages) {
              // 切会话时 provider 会先短暂携带上一个会话的消息：按
              // conversationId 识别并降级为 loading，且不在假数据上白白
              // 消耗 _pendingScrollToBottom。
              // 例外：「主对话流」的消息列表本就是**全部普通会话的合并时
              // 间线**（见 ConversationRepository.getMessages 主会话特例），
              // 消息的 conversation_id 天然各不相同，不能套此判定。
              final mainConvId = ref.watch(mainConversationProvider).value?.id;
              final isMainConversation = selectedId == mainConvId;
              if (!isMainConversation &&
                  messages.isNotEmpty &&
                  messages.first.conversationId != selectedId) {
                return const Center(child: CircularProgressIndicator());
              }
              // 当前会话是否正在生成 AI 回复（渲染尾部占位气泡）
              final legacyTyping = generatingIds.contains(selectedId);
              final isAiTyping = legacyTyping || durableBusy;

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
              final tailCount = notices.length + (legacyTyping ? 1 : 0);
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
                        (!durable ||
                            (durableRunInput != message.id &&
                                message.conversationId == selectedId)) &&
                        ((message.id == latestUserMessageId &&
                                !latestUserHasReply) ||
                            (failedReplyId != null &&
                                message.id == failedReplyId));
                    return _MessageBubble(
                      message: message,
                      showRetry: needsReply,
                      onRetry: () => _retryAiGeneration(selectedId, message.id),
                      isLastMessage: libIndex == rows.length - 1,
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

        if (durable)
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 220),
            child: SingleChildScrollView(
              child: Column(
                children: [
                  ChatRunStatus(conversationId: selectedId),
                  ChatInputQueue(conversationId: selectedId),
                ],
              ),
            ),
          ),
        // Input area
        _MessageInput(),
      ],
    );
  }
}
