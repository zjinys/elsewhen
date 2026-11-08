import '../l10n/notice_messages.dart';
import 'app_error_notice.dart';

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/conversation.dart';
import '../models/tweet_fetch.dart';
import '../providers/chat_run_provider.dart';
import '../providers/conversation_provider.dart';
import '../providers/goal_provider.dart';
import '../theme/app_theme.dart';
import 'chat_input_queue.dart';
import 'chat_proposal_review.dart';
import 'chat_run_status.dart';
import 'goal_chat_ui_blocks.dart';
import 'wiki_ai_chat_panel.dart';
import '../l10n/localization.dart';

/// 目标页 AI 面板（goal_mentor）：每条活跃目标一个专属讨论会话
/// （`ensureGoalChat`）；未选中目标时是 [studio] 立项工作室（保留会话
/// `ensureGoalStudio`）。讨论上下文由 system_context 的 goal_mentor /
/// 目标工作室阶段注入；全程在目标页内展开，不切对话 tab。AI 变更
/// （暂停/达成/重启/行动调整、计划拆解）全部走 WriteConfirm 提议卡，
/// 本面板只负责对话与展示，不做任何直接写库。
///
/// 可操作选项 UI：AI 回复里的 ```elsewhen-choices 围栏块
/// （JSON 数组，见 goal_mentor 幕僚提示）渲染成可点选按钮，
/// 点击即把该选项作为用户消息发送；块本身不渲染为原始 JSON。
///
/// 生成式 UI：```elsewhen-ui 围栏块（UITree JSON，见
/// goal_chat_ui_blocks.dart 的 guardrail 目录）渲染成原生组件卡片
/// （指标/行动/困难/选项按钮），动作只开放 send_message 与
/// fill_draft；解析失败自动回退纯文本，绝不展示原始 JSON。
class GoalChatPanel extends ConsumerStatefulWidget {
  const GoalChatPanel({
    super.key,
    this.goalId = '',
    this.goalContent = '',
    this.studio = false,
    this.initialSeed,
    this.onMinimize,
    this.onDragUpdate,
    this.fillHeight = false,
  });

  /// 目标 id；[studio] 为 true 时留空（工作室不绑定具体目标）。
  final String goalId;
  final String goalContent;

  /// 立项工作室形态：走保留会话 `ensureGoalStudio`（AI 立项/拆解专用，
  /// 不进普通对话界面），标题与开场白换成「AI 立项工作室」。
  final bool studio;

  /// 预填种子草稿（卡内嵌形态的入口按需传入；目标页主区走
  /// [goalPanelSeedProvider]）。只写入输入框草稿，**绝不自动发送**。
  final String? initialSeed;

  final VoidCallback? onMinimize;

  /// 拖动回调（浮动形态时用；卡内嵌形态传 null）。
  final ValueChanged<Offset>? onDragUpdate;

  /// 占满可用高度（左右分栏形态用 true）；false 时消息区限高滚动
  /// （卡内嵌形态）。
  final bool fillHeight;

  @override
  ConsumerState<GoalChatPanel> createState() => _GoalChatPanelState();
}

/// elsewhen-choices 块的解析结果：剥块正文 + 可点选选项。
class _ChoicesBlock {
  final String strippedContent;
  final List<String> options;
  const _ChoicesBlock({required this.strippedContent, required this.options});
}

class _GoalChatPanelState extends ConsumerState<GoalChatPanel> {
  String? _conversationId;
  List<Message> _messages = [];
  bool _ready = false;
  bool _sending = false;
  bool get _busy =>
      _sending ||
      (_conversationId != null &&
          ref.read(conversationRepositoryProvider).supportsChatRuns &&
          ref.read(chatRunControllerProvider(_conversationId!)).state.busy);
  bool get _inputBusy =>
      _sending ||
      (_busy && !ref.read(conversationRepositoryProvider).supportsChatQueue);
  String? _error;
  final _inputController = TextEditingController();
  final _inputFocus = FocusNode();
  final _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    // 回车行为与主对话一致：Enter 直接发送，Shift+Enter 换行。
    // 多行输入下 onSubmitted 不触发，需在焦点节点拦 Enter。
    _inputFocus.onKeyEvent = (node, event) {
      final isEnter = event.logicalKey == LogicalKeyboardKey.enter;
      final isShift = HardwareKeyboard.instance.isShiftPressed;
      if (event is KeyDownEvent && isEnter && !isShift) {
        _send(_inputController.text);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    };
    if (widget.initialSeed != null) {
      _inputController.text = widget.initialSeed!;
      _inputController.selection = TextSelection.collapsed(
        offset: _inputController.text.length,
      );
    }
    _ensureConversation();
  }

  @override
  void didUpdateWidget(covariant GoalChatPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.goalId != widget.goalId ||
        oldWidget.studio != widget.studio) {
      _conversationId = null;
      _messages = [];
      _ready = false;
      _sending = false;
      _error = null;
      _inputController.clear();
      _ensureConversation();
    }
  }

  @override
  void dispose() {
    _inputController.dispose();
    _inputFocus.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  /// 面板 scope：目标 id，或立项工作室的保留 id（也是预填草稿的键）。
  String get _scopeKey => widget.studio ? goalStudioId : widget.goalId;

  /// 把草稿填入输入框并聚焦（不发送）。
  void _applySeed(String seed) {
    if (seed.trim().isEmpty) return;
    _inputController.text = seed;
    _inputController.selection = TextSelection.collapsed(offset: seed.length);
    _inputFocus.requestFocus();
  }

  Future<void> _ensureConversation() async {
    final goalId = widget.goalId;
    final studio = widget.studio;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      // 工作室走保留会话（不绑定目标），目标讨论走目标专属会话。
      final conv = studio
          ? await repo.ensureGoalStudio()
          : await repo.ensureGoalChat(goalId);
      if (!mounted || widget.studio != studio || widget.goalId != goalId) {
        return;
      }
      setState(() {
        _conversationId = conv.id;
        _ready = true;
      });
      await _loadMessages();
    } catch (e) {
      if (!mounted || widget.studio != studio || widget.goalId != goalId) {
        return;
      }
      setState(() {
        _ready = true;
        _error = appFailure(e);
      });
    }
  }

  Future<void> _loadMessages() async {
    final id = _conversationId;
    if (id == null) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final messages = await repo.listMessages(id);
      if (!mounted || _conversationId != id) return;
      setState(() => _messages = messages);
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = appFailure(e));
    }
  }

  Future<void> _send(String raw) async {
    final text = raw.trim();
    final id = _conversationId;
    if (text.isEmpty || id == null || _inputBusy) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final durable = ref.read(conversationRepositoryProvider).supportsChatRuns;
    final generatingNotifier = ref.read(aiGeneratingProvider.notifier);
    try {
      // 持久化用户消息。goal 会话无 page task，直接走主 sendMessage
      // 入口（幂等性由 durable run 的 inputMessageId 绑定保证）。
      final saved = await repo.sendMessage(id, 'user', text);
      if (!mounted || _conversationId != id) return;
      _inputController.clear();
      setState(() => _messages.add(saved));
      _scrollToBottom();
      if (durable) {
        ref.invalidate(goalChatMessagesProvider(id));
        try {
          final repository = ref.read(conversationRepositoryProvider);
          if (repository.supportsChatQueue) {
            await repository.scheduleChatInput(id, saved.id);
            if (mounted) {
              await ref.read(chatRunControllerProvider(id)).restore();
            }
          } else {
            await ref.read(chatRunControllerProvider(id)).start(saved.id);
          }
        } catch (_) {
          if (mounted) {
            setState(() => _error = appNotice('savedButNotEligible'));
          }
        }
      } else {
        // 非 durable 兼容路径：内存内同步生成。
        generatingNotifier.update((current) => {...current, id});
        final reply = await repo.generateReply(id);
        if (!mounted || _conversationId != id) return;
        setState(
          () => _messages.add(
            Message(
              id: 'ai-${DateTime.now().microsecondsSinceEpoch}',
              conversationId: id,
              role: MessageRole.assistant,
              content: reply,
              createdAt: DateTime.now(),
            ),
          ),
        );
      }
    } catch (e) {
      if (!mounted || _conversationId != id) return;
      setState(() => _error = appFailure(e));
    } finally {
      if (!durable) {
        generatingNotifier.update((current) => {...current}..remove(id));
      }
      if (mounted && _conversationId == id) {
        setState(() => _sending = false);
        _scrollToBottom();
      }
    }
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      _scrollController.animateTo(
        _scrollController.position.maxScrollExtent,
        duration: const Duration(milliseconds: 200),
        curve: Curves.easeOut,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final id = _conversationId;
    // 预填种子（「和 AI 拆解」/ 航向校准「聚焦讨论」写 goalPanelSeedProvider）：
    // 填入输入框并聚焦，绝不自动发送。post-frame 消费，兼容面板还没挂载时
    // 就已经写入的种子。
    final scope = _scopeKey;
    final pendingSeed = ref.watch(goalPanelSeedProvider(scope));
    if (pendingSeed != null && pendingSeed.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || _scopeKey != scope) return;
        _applySeed(pendingSeed);
        ref.read(goalPanelSeedProvider(scope).notifier).set(null);
      });
    }
    final durable = ref.watch(conversationRepositoryProvider).supportsChatRuns;
    if (id != null && durable) {
      ref.watch(chatRunStateProvider(id).select((value) => value.value?.busy));
      ref.listen(goalChatMessagesProvider(id), (previous, next) {
        if ((next.value?.length ?? 0) > (previous?.value?.length ?? 0)) {
          _scrollToBottom();
        }
      });
    }
    final displayedMessages = id != null && durable
        ? ref.watch(goalChatMessagesProvider(id)).value ?? _messages
        : _messages;
    final runInput = id != null && durable
        ? ref.watch(
            chatRunStateProvider(id)
                .select((value) => value.value?.run?.inputMessageId),
          )
        : null;
    final unansweredInput =
        displayedMessages.isNotEmpty && displayedMessages.last.isUser
        ? displayedMessages.last.id
        : null;
    final messages = ListView(
      controller: _scrollController,
      shrinkWrap: true,
      padding: const EdgeInsets.all(AppTheme.space3),
      children: [
        WikiChatBubble(
          message: ContentChatMessage(
            role: 'assistant',
            content: widget.studio
                ? context.l10n.goalStudioWelcome
                : context.l10n.goalChatWelcome,
          ),
        ),
        for (final m in displayedMessages)
          _buildMessage(
            m,
            canGenerate:
                durable &&
                !_busy &&
                m.id == unansweredInput &&
                m.id != runInput,
          ),
        if (durable && id != null) ChatRunStatus(conversationId: id),
        if (durable && id != null) ChatInputQueue(conversationId: id),
        if (_busy && !durable) const WikiChatBubble.pending(),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(top: AppTheme.space2),
            child: AppErrorMessage(
              action: context.l10n.load,
              error: _error!,
              style: TextStyle(fontSize: 12, color: AppTheme.error),
            ),
          ),
      ],
    );
    return Container(
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          // 头部
          GestureDetector(
            behavior: HitTestBehavior.opaque,
            onPanUpdate: widget.onDragUpdate == null
                ? null
                : (details) => widget.onDragUpdate!(details.delta),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(
                AppTheme.space3,
                AppTheme.space2,
                AppTheme.space3,
                AppTheme.space2,
              ),
              child: Row(
                children: [
                  Icon(
                    Icons.auto_awesome,
                    size: 14,
                    color: AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      widget.studio
                          ? context.l10n.goalStudio
                          : context.l10n.goalDiscussionTitle(
                              _shortGoal(widget.goalContent),
                            ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        color: AppTheme.textSecondary,
                      ),
                    ),
                  ),
                  if (_conversationId != null)
                    ChatProposalReviewButton(
                      conversationId: _conversationId,
                      compact: true,
                      onChanged: () {
                        // 提议执行后目标阶段可能变化，交由宿主刷新。
                        // 这里保守刷新目标相关 provider。
                        ref.invalidate(conversationsProvider);
                      },
                    ),
                  if (widget.onMinimize != null)
                    IconButton(
                      tooltip: context.l10n.collapseDiscussion,
                      onPressed: widget.onMinimize,
                      icon: const Icon(Icons.remove, size: 18),
                      visualDensity: VisualDensity.compact,
                    ),
                  if (!_ready)
                    const SizedBox(
                      width: 12,
                      height: 12,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                ],
              ),
            ),
          ),
          Divider(height: 1, color: AppTheme.surface3),
          // 消息区：分栏形态占满剩余高度；卡内嵌限高滚动
          if (widget.fillHeight)
            Expanded(child: messages)
          else
            Flexible(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 280),
                child: messages,
              ),
            ),
          // 输入区
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space3,
              0,
              AppTheme.space3,
              AppTheme.space3,
            ),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    key: const ValueKey('goal-panel-input'),
                    controller: _inputController,
                    focusNode: _inputFocus,
                    enabled: _ready && !_inputBusy,
                    // 与主对话输入框一致：默认 1 行、最多 4 行；
                    // Enter 发送、Shift+Enter 换行（onKeyEvent 拦截）。
                    maxLines: 4,
                    minLines: 1,
                    decoration: InputDecoration(
                      hintText: widget.studio
                          ? context.l10n.goalStudioInputHint
                          : context.l10n.goalChatInputHint,
                      hintStyle: TextStyle(
                        fontSize: 12,
                        color: AppTheme.textTertiary,
                      ),
                      isDense: true,
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 8,
                      ),
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
                          width: 1.5,
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: AppTheme.space2),
                IconButton.filled(
                  onPressed: (_ready && !_inputBusy)
                      ? () => _send(_inputController.text)
                      : null,
                  style: IconButton.styleFrom(
                    backgroundColor: AppTheme.accentPrimary,
                    disabledBackgroundColor: AppTheme.surface3,
                  ),
                  icon: const Icon(Icons.arrow_upward, size: 16),
                  tooltip: _busy ? context.l10n.queueNext : context.l10n.send,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildMessage(Message message, {bool canGenerate = false}) {
    // 可操作选项：assistant 消息里的 elsewhen-choices 围栏块渲染成
    // 可点选按钮（点击即发送），块本身从 markdown 正文中剥掉。
    if (!message.isUser) {
      // 生成式 UI：elsewhen-ui 块（UITree JSON）优先渲染为原生组件；
      // 解析失败回退到 choices/纯文本路径。
      final ui = extractGoalUiBlock(message.content);
      if (ui != null) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (ui.strippedContent.isNotEmpty)
              WikiChatBubble(
                message: ContentChatMessage(
                  role: message.role.name,
                  content: ui.strippedContent,
                ),
              ),
            GoalUiBlock(
              tree: ui.tree,
              onAction: (name, params) => _handleUiAction(name, params),
            ),
          ],
        );
      }
      final choices = _extractChoices(message.content);
      if (choices != null) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            WikiChatBubble(
              message: ContentChatMessage(
                role: message.role.name,
                content: choices.strippedContent,
              ),
            ),
            _buildChoiceChips(choices.options),
          ],
        );
      }
    }
    final bubble = WikiChatBubble(
      message: ContentChatMessage(
        role: message.role.name,
        content: message.content,
      ),
    );
    if (canGenerate) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          bubble,
          TextButton(
            onPressed: () => ref
                .read(chatRunControllerProvider(message.conversationId))
                .start(message.id),
            child: Text(context.l10n.generateReply),
          ),
        ],
      );
    }
    return bubble;
  }

  /// 选项按钮组：点击即把选项原文作为用户消息发送。
  /// 生成中/输入中禁用，避免并发发送。
  Widget _buildChoiceChips(List<String> options) {
    final enabled = _ready && !_inputBusy;
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space3, left: 34),
      child: Wrap(
        spacing: 6,
        runSpacing: 6,
        children: [
          for (final option in options)
            ActionChip(
              label: Text(option, style: const TextStyle(fontSize: 12)),
              labelPadding: const EdgeInsets.symmetric(horizontal: 8),
              visualDensity: VisualDensity.compact,
              backgroundColor: AppTheme.accentPrimary.withValues(alpha: 0.08),
              side: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.35),
              ),
              onPressed: enabled ? () => _send(option) : null,
            ),
        ],
      ),
    );
  }

  /// 生成式 UI 动作：只开放 send_message 与 fill_draft；
  /// 未识别动作静默忽略（guardrail 兑底，目标写操作仍走提议卡）。
  void _handleUiAction(String name, Map<String, dynamic>? params) {
    final text = params?['message']?.toString().trim() ?? '';
    if (text.isEmpty) return;
    switch (name) {
      case 'send_message':
        _send(text);
        break;
      case 'fill_draft':
        _inputController.text = text;
        _inputController.selection = TextSelection.collapsed(
          offset: text.length,
        );
        _inputFocus.requestFocus();
        break;
    }
  }

  String _shortGoal(String content) {
    final t = content.trim().replaceAll('\n', ' ');
    return t.length <= 24 ? t : '${t.substring(0, 24)}…';
  }

  /// 提取 assistant 消息里的 elsewhen-choices 围栏块。
  /// 返回 null 表示无选项块；否则返回剥块后的正文与选项列表。
  _ChoicesBlock? _extractChoices(String content) {
    final pattern = RegExp(
      r'```elsewhen-choices\s*\n([\s\S]*?)\n?```',
      caseSensitive: false,
    );
    final match = pattern.firstMatch(content);
    if (match == null) return null;
    final raw = match.group(1) ?? '';
    final options = <String>[];
    try {
      final decoded = jsonDecode(raw);
      if (decoded is List) {
        for (final item in decoded) {
          if (item is Map && item['label'] is String) {
            final label = (item['label'] as String).trim();
            if (label.isNotEmpty) options.add(label);
          } else if (item is String && item.trim().isNotEmpty) {
            options.add(item.trim());
          }
        }
      }
    } catch (_) {
      // 宽松回退：非 JSON 时按行取列表项（模型偶发格式漂移的兕底）。
      for (final line in raw.split('\n')) {
        final t = line.trim().replaceFirst(RegExp(r'^[-*•]\s*'), '');
        if (t.isNotEmpty && !t.startsWith('{') && !t.startsWith('[')) {
          options.add(t);
        }
      }
    }
    if (options.isEmpty) return null;
    final stripped = content.replaceRange(match.start, match.end, '').trim();
    return _ChoicesBlock(strippedContent: stripped, options: options);
  }
}
