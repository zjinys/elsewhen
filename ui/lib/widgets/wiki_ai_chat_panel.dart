import 'dart:math';

import 'suggestion_feedback.dart';
import 'chat_page_task_scope.dart';
import 'chat_proposal_review.dart';
import 'chat_run_status.dart';
import 'chat_input_queue.dart';
import '../providers/chat_run_provider.dart';
import '../bridge/api.dart' as api;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/conversation.dart';
import '../models/tweet_fetch.dart';
import '../models/wiki_page.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import 'markdown_view.dart';
import 'knowledge_panel.dart';
import 'knowledge_import.dart';

part 'wiki_ai_chat_panel_parts/panel_task.dart';
part 'wiki_ai_chat_panel_parts/panel_render.dart';
part 'wiki_ai_chat_panel_parts/panel_bubbles.dart';
// ── 页内 AI 聊天面板按任务/渲染/气泡拆分（同库 part，零行为变化）──

/// 页内 AI 处理面板：围绕当前页面聊天（总结/补充/改写）。
///
/// 需要改页时模型调用 save_wiki_revision，确认后才写库。
/// 由 [wikiChatNode] 块（正文对话块）与页面详情页底部共用，
/// 行为一致，仅宿主不同。
class WikiAiChatPanel extends ConsumerStatefulWidget {
  final String slug;
  final VoidCallback? onMinimize;
  final ValueChanged<Offset>? onDragUpdate;

  const WikiAiChatPanel({
    super.key,
    required this.slug,
    this.onMinimize,
    this.onDragUpdate,
  });

  @override
  ConsumerState<WikiAiChatPanel> createState() => _WikiAiChatPanelState();
}

class _WikiAiChatPanelState extends ConsumerState<WikiAiChatPanel> {
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
      _taskRestoreError != null ||
      (_busy && !ref.read(conversationRepositoryProvider).supportsChatQueue);
  bool _saving = false;
  bool _reimporting = false;
  bool _rescanning = false;
  String? _error;
  ChatPageTask? _pageTask;
  String? _taskRestoreError;
  String? _pendingInputSignature;
  String? _pendingInputKey;
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
    _ensureConversation();
  }

  @override
  void didUpdateWidget(covariant WikiAiChatPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.slug != widget.slug) {
      _conversationId = null;
      _messages = [];
      _ready = false;
      _sending = false;
      _pageTask = null;
      _taskRestoreError = null;
      _error = null;
      _pendingInputSignature = null;
      _pendingInputKey = null;
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

  Future<void> _ensureConversation() async {
    final slug = widget.slug;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final conv = await repo.ensureWikiPageChat(slug);
      ChatPageTask? task;
      String? taskError;
      if (repo.supportsPageTasks) {
        try {
          final saved = await repo.getWikiChatTask(conv.id);
          task = saved == null
              ? ChatPageTask('local_question', [
                  await repo.getWikiChatMaterial(slug),
                ])
              : await ChatPageTask.restore(saved, repo.getWikiChatMaterial);
        } catch (e) {
          taskError = '原任务暂不能恢复：$e';
        }
      }
      if (!mounted || widget.slug != slug) return;
      setState(() {
        _pageTask = task;
        _taskRestoreError = taskError;
        _conversationId = conv.id;
        _ready = true;
      });
      await _loadMessages();
    } catch (e) {
      if (!mounted || widget.slug != slug) return;
      setState(() {
        _ready = true;
        _error = '会话初始化失败：${e.toString().replaceFirst('Exception: ', '')}';
      });
    }
  }


  @override
  Widget build(BuildContext context) {
    final id = _conversationId;
    final durable = ref.watch(conversationRepositoryProvider).supportsChatRuns;
    if (id != null && durable) {
      ref.watch(chatRunStateProvider(id).select((value) => value.value?.busy));
      ref.listen(wikiChatMessagesProvider(id), (previous, next) {
        if ((next.value?.length ?? 0) > (previous?.value?.length ?? 0)) {
          _scrollToBottom();
        }
      });
    }
    final displayedMessages = id != null && durable
        ? ref.watch(wikiChatMessagesProvider(id)).value ?? _messages
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
    final page = ref.watch(wikiPageProvider(widget.slug)).value;
    final isProject = page?.kind == 'project';
    final canReimport =
        !isProject && page?.sourceUrl?.trim().isNotEmpty == true;
    return Container(
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // 有界高度（详情页右侧栏 / 底部弹窗）时消息区弹性撑满、输入框钉在底部；
          // 无界高度（正文内嵌对话块）时保持收缩布局，避免 Expanded 在滚动容器里报错。
          final fill = constraints.hasBoundedHeight;
          final messages = ListView(
            controller: _scrollController,
            shrinkWrap: !fill,
            padding: const EdgeInsets.all(AppTheme.space3),
            children: [
              WikiChatBubble(
                message: const ContentChatMessage(
                  role: 'assistant',
                  content: '👋 我可以帮你总结、提取要点、补充或改写这一页；需要写回知识库时会先给你确认。\n如果这是从本地目录导入的项目，也可以直接说“重新扫描这个项目目录”，我会按最新文件重新导入。',
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
                  child: Text(
                    _error!,
                    style: TextStyle(fontSize: 12, color: AppTheme.error),
                  ),
                ),
            ],
          );
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: fill ? MainAxisSize.max : MainAxisSize.min,
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
                      Text(
                        'AI对话',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                      const Spacer(),
                      if (_conversationId != null)
                        ChatProposalReviewButton(
                          conversationId: _conversationId,
                          compact: true,
                          onChanged: () {
                            ref.invalidate(wikiPageProvider(widget.slug));
                            ref.invalidate(
                              wikiDerivativesProvider(widget.slug),
                            );
                          },
                        ),
                      if (widget.onDragUpdate != null)
                        Tooltip(
                          message: '拖动以移动对话窗口',
                          child: Icon(
                            Icons.drag_indicator,
                            size: 17,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      if (widget.onMinimize != null)
                        IconButton(
                          tooltip: '缩小对话窗口',
                          onPressed: widget.onMinimize,
                          icon: const Icon(Icons.remove, size: 18),
                          visualDensity: VisualDensity.compact,
                        ),
                      if (widget.slug.startsWith('topic/'))
                        TextButton.icon(
                          onPressed: _returnToMainConversation,
                          icon: const Icon(Icons.arrow_back, size: 14),
                          label: const Text('返回主对话'),
                          style: TextButton.styleFrom(
                            padding: const EdgeInsets.symmetric(horizontal: 6),
                            visualDensity: VisualDensity.compact,
                          ),
                        ),
                      if (!_ready)
                        const SizedBox(
                          width: 12,
                          height: 12,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                      if (_reimporting || _rescanning)
                        const Padding(
                          padding: EdgeInsets.symmetric(horizontal: 8),
                          child: SizedBox(
                            width: 14,
                            height: 14,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                        )
                      else
                        PopupMenuButton<String>(
                          enabled: _ready && !_busy,
                          tooltip: 'AI 任务',
                          color: AppTheme.surface1,
                          surfaceTintColor: Colors.transparent,
                          icon: const Icon(Icons.more_vert, size: 18),
                          onSelected: _handleTask,
                          itemBuilder: (context) => [
                            _taskMenuItem(
                              'summary',
                              Icons.summarize_outlined,
                              '总结这一页',
                            ),
                            _taskMenuItem(
                              'extract',
                              Icons.format_list_bulleted,
                              '提取关键点',
                            ),
                            _taskMenuItem(
                              'decisions',
                              Icons.gavel_outlined,
                              '沉淀决定和依据',
                            ),
                            _taskMenuItem(
                              'next',
                              Icons.checklist_outlined,
                              '整理下一步',
                            ),
                            _taskMenuDivider(),
                            _taskMenuItem('translate', Icons.translate, '翻译片段'),
                            _taskMenuItem(
                              'copy',
                              Icons.campaign_outlined,
                              '写文案',
                            ),
                            if (isProject) ...[
                              _taskMenuDivider(),
                              _taskMenuItem(
                                'rescan',
                                Icons.refresh,
                                '重新扫描项目目录',
                              ),
                            ] else if (canReimport) ...[
                              _taskMenuDivider(),
                              _taskMenuItem(
                                'reimport',
                                Icons.refresh,
                                '重新导入来源',
                              ),
                            ],
                          ],
                        ),
                    ],
                  ),
                ),
              ),
              Divider(height: 1, color: AppTheme.surface3),
              // 消息区
              if (fill)
                Expanded(child: messages)
              else
                ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 160),
                  child: messages,
                ),
              if (_taskRestoreError != null)
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(_taskRestoreError!),
                      const Text('重新选择将使用当前页的新版本，并清除旧选区和对照材料。'),
                      TextButton(
                        onPressed: _sending ? null : _resetTask,
                        child: const Text('明确重新选择任务与材料'),
                      ),
                    ],
                  ),
                ),
              if (_pageTask != null)
                ChatPageTaskScope(
                  repository: ref.read(
                    storageRepositoryProvider,
                  ) as RustBridgeRepository,
                  task: _pageTask!,
                  enabled: !_sending,
                  onChanged: (task) => setState(() => _pageTask = task),
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
                        controller: _inputController,
                        focusNode: _inputFocus,
                        enabled: _ready && !_inputBusy,
                        // 与主对话输入框一致：默认 1 行、最多 4 行；
                        // Enter 发送、Shift+Enter 换行（onKeyEvent 拦截）。
                        maxLines: 4,
                        minLines: 1,
                        decoration: InputDecoration(
                          hintText: '就这一页问问 AI…',
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
                      tooltip: _busy ? '保存并排队下一问' : '发送',
                    ),
                    if (ref
                        .watch(conversationRepositoryProvider)
                        .supportsChatQueue)
                      PopupMenuButton<String>(
                        tooltip: '输入保存方式',
                        enabled: _ready && !_sending,
                        onSelected: (mode) => _send(
                          _inputController.text,
                          recordOnly: mode == 'record',
                          supplement: mode == 'supplement',
                        ),
                        itemBuilder: (_) => [
                          const PopupMenuItem(
                            value: 'record',
                            child: Text('仅保存，不生成回复'),
                          ),
                          if (_conversationId != null &&
                              ref
                                  .read(
                                    chatRunControllerProvider(_conversationId!),
                                  )
                                  .state
                                  .active)
                            const PopupMenuItem(
                              value: 'supplement',
                              child: Text('补充当前任务'),
                            ),
                        ],
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
}
