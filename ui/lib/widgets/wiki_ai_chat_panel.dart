import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/conversation.dart';
import '../models/tweet_fetch.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';

/// 页内 AI 处理面板：围绕当前页面聊天（总结/补充/改写）。
///
/// 需要改页时模型调用 save_wiki_revision，确认后才写库。
/// 由 [wikiChatNode] 块（正文对话块）与页面详情页底部共用，
/// 行为一致，仅宿主不同。
class WikiAiChatPanel extends ConsumerStatefulWidget {
  final String slug;

  const WikiAiChatPanel({super.key, required this.slug});

  @override
  ConsumerState<WikiAiChatPanel> createState() => _WikiAiChatPanelState();
}

class _WikiAiChatPanelState extends ConsumerState<WikiAiChatPanel> {
  String? _conversationId;
  List<Message> _messages = [];
  bool _ready = false;
  bool _busy = false;
  String? _error;
  final _inputController = TextEditingController();
  final _scrollController = ScrollController();

  /// 快捷指令：点击即把对应 prompt 发给本页 AI（结果需确认才写库）
  static const _quickPrompts = [
    _QuickPrompt(
      label: '沉淀决定',
      prompt: '回顾本页最近的讨论，只提取已经明确的决定和依据。请提出对当前页面的修订，并通过 save_wiki_revision 确认门让我确认后再写入；不要直接修改页面。',
    ),
    _QuickPrompt(
      label: '整理下一步',
      prompt: '回顾本页最近的讨论，整理可执行的下一步和未决问题。请提出对当前页面的修订，并通过 save_wiki_revision 确认门让我确认后再写入；不要直接修改页面。',
    ),
    _QuickPrompt(label: '总结', prompt: '给我总结一下这一页，用要点列出核心信息'),
    _QuickPrompt(label: '提取要点', prompt: '提取这一页的关键信息，按重要性列出'),
    _QuickPrompt(label: '写抖音文案', prompt: '根据这一页内容，写一段适合发抖音的文案'),
    _QuickPrompt(label: '翻译成英文', prompt: '把这一页翻译成英文'),
  ];

  @override
  void initState() {
    super.initState();
    _ensureConversation();
  }

  @override
  void dispose() {
    _inputController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _ensureConversation() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final conv = await repo.ensureWikiPageChat(widget.slug);
      if (!mounted) return;
      setState(() {
        _conversationId = conv.id;
        _ready = true;
      });
      await _loadMessages();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _ready = true;
        _error = '会话初始化失败：${e.toString().replaceFirst('Exception: ', '')}';
      });
    }
  }

  Future<void> _loadMessages() async {
    final id = _conversationId;
    if (id == null) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final messages = await repo.listMessages(id);
      if (!mounted) return;
      setState(() => _messages = messages);
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(
        () => _error = '消息加载失败：${e.toString().replaceFirst('Exception: ', '')}',
      );
    }
  }

  Future<void> _send(String raw) async {
    final text = raw.trim();
    final id = _conversationId;
    if (text.isEmpty || id == null || _busy) return;
    setState(() {
      _messages.add(
        Message(
          id: 'local-${DateTime.now().microsecondsSinceEpoch}',
          conversationId: id,
          role: MessageRole.user,
          content: text,
          createdAt: DateTime.now(),
        ),
      );
      _busy = true;
      _error = null;
    });
    _inputController.clear();
    _scrollToBottom();
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.sendMessage(id, 'user', text);
      // 追加 AI 回复
      final reply = await repo.generateReply(id);
      if (!mounted) return;
      setState(() {
        _messages.add(
          Message(
            id: 'ai-${DateTime.now().microsecondsSinceEpoch}',
            conversationId: id,
            role: MessageRole.assistant,
            content: reply,
            createdAt: DateTime.now(),
          ),
        );
        _busy = false;
      });
      _scrollToBottom();
      // 页面内容可能被修订：让页面详情 provider 失效以刷新
      ref.invalidate(wikiPageProvider(widget.slug));
      ref.invalidate(pageRelationsProvider(widget.slug));
      ref.invalidate(wikiDerivativesProvider(widget.slug));
      ref.invalidate(wikiPagesProvider);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = e.toString().replaceFirst('Exception: ', '');
      });
      _scrollToBottom();
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

  Future<void> _returnToMainConversation() async {
    final main = await ref.read(mainConversationProvider.future);
    if (!mounted) return;
    ref.read(selectedConversationIdProvider.notifier).state = main.id;
    ref.read(sidebarTabProvider.notifier).state = SidebarTab.conversation;
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(top: AppTheme.space2),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // 头部
          Padding(
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
                  'AI 处理本页',
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
                const Spacer(),
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
              ],
            ),
          ),
          Divider(height: 1, color: AppTheme.surface3),
          // 快捷指令：一键触发常见处理，结果需确认才写库
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space3,
              AppTheme.space2,
              AppTheme.space3,
              0,
            ),
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final p in _quickPrompts)
                  _QuickPromptChip(
                    label: p.label,
                    enabled: _ready && !_busy,
                    onTap: () => _send(p.prompt),
                  ),
              ],
            ),
          ),
          const SizedBox(height: AppTheme.space2),
          // 消息区
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 160),
            child: ListView(
              controller: _scrollController,
              shrinkWrap: true,
              padding: const EdgeInsets.all(AppTheme.space3),
              children: [
                WikiChatBubble(
                  message: const ContentChatMessage(
                    role: 'assistant',
                    content: '👋 我可以帮你总结、提取要点、补充或改写这一页；需要写回知识库时会先给你确认。',
                  ),
                ),
                for (final m in _messages)
                  WikiChatBubble(
                    message: ContentChatMessage(
                      role: m.role.name,
                      content: m.content,
                    ),
                  ),
                if (_busy) const WikiChatBubble.pending(),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: AppTheme.space2),
                    child: Text(
                      _error!,
                      style: TextStyle(fontSize: 12, color: AppTheme.error),
                    ),
                  ),
              ],
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
                    controller: _inputController,
                    enabled: _ready && !_busy,
                    minLines: 1,
                    maxLines: 3,
                    onSubmitted: (_) => _send(_inputController.text),
                    decoration: InputDecoration(
                      hintText: '就这一页问问 AI…（回车发送）',
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
                  onPressed: (_ready && !_busy)
                      ? () => _send(_inputController.text)
                      : null,
                  style: IconButton.styleFrom(
                    backgroundColor: AppTheme.accentPrimary,
                    disabledBackgroundColor: AppTheme.surface3,
                  ),
                  icon: const Icon(Icons.arrow_upward, size: 16),
                  tooltip: '发送',
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// 页内 AI 快捷指令定义
class _QuickPrompt {
  final String label;
  final String prompt;

  const _QuickPrompt({required this.label, required this.prompt});
}

/// 快捷指令 chip：点击把对应 prompt 交给页面 AI
class _QuickPromptChip extends StatelessWidget {
  final String label;
  final bool enabled;
  final VoidCallback onTap;

  const _QuickPromptChip({
    required this.label,
    required this.enabled,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: enabled ? onTap : null,
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          color: enabled
              ? AppTheme.accentPrimary.withValues(alpha: 0.10)
              : AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(
            color: enabled
                ? AppTheme.accentPrimary.withValues(alpha: 0.35)
                : AppTheme.surface3,
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.bolt,
              size: 11,
              color: enabled ? AppTheme.accentPrimary : AppTheme.textTertiary,
            ),
            const SizedBox(width: 4),
            Text(
              label,
              style: TextStyle(
                fontSize: 11,
                color: enabled ? AppTheme.accentPrimary : AppTheme.textTertiary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 聊天气泡：页面 AI 对话与导入预览共用
class WikiChatBubble extends StatelessWidget {
  final ContentChatMessage message;
  final bool pending;

  const WikiChatBubble({super.key, required this.message}) : pending = false;

  const WikiChatBubble.pending({super.key})
    : message = const ContentChatMessage(role: 'assistant', content: ''),
      pending = true;

  @override
  Widget build(BuildContext context) {
    final isUser = message.role == 'user';
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space3),
      child: Row(
        mainAxisAlignment: isUser
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(),
            const SizedBox(width: 8),
          ] else
            const SizedBox(width: 48),
          Flexible(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: isUser
                    ? AppTheme.accentPrimary.withValues(alpha: 0.15)
                    : AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
              ),
              child: pending
                  ? Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        SizedBox(
                          width: 12,
                          height: 12,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                        SizedBox(width: 8),
                        Text(
                          '思考中…',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ],
                    )
                  : SelectableText(
                      message.content,
                      style: TextStyle(
                        fontSize: 13,
                        height: 1.55,
                        color: AppTheme.textPrimary,
                      ),
                    ),
            ),
          ),
          if (isUser) const SizedBox(width: 8),
        ],
      ),
    );
  }

  Widget _buildAvatar() {
    return Container(
      width: 26,
      height: 26,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: 0.18),
        shape: BoxShape.circle,
      ),
      child: Text(
        'EW',
        style: TextStyle(
          fontSize: 10,
          fontWeight: FontWeight.w700,
          color: AppTheme.accentPrimary,
        ),
      ),
    );
  }
}