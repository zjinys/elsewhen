import 'suggestion_feedback.dart';
import '../bridge/api.dart' as api;

import 'package:flutter/material.dart';
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
  bool _busy = false;
  bool _saving = false;
  bool _reimporting = false;
  bool _rescanning = false;
  String? _error;
  final _inputController = TextEditingController();
  final _scrollController = ScrollController();

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
    final generatingNotifier = ref.read(aiGeneratingProvider.notifier);
    setAiGenerating(ref, id, true);
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
    } finally {
      generatingNotifier.update((current) => {...current}..remove(id));
    }
  }

  void _fillPrompt(String prompt) {
    _inputController
      ..text = prompt
      ..selection = TextSelection.collapsed(offset: prompt.length);
  }

  Future<void> _handleTask(String action) async {
    switch (action) {
      case 'summary':
        _fillPrompt('请总结这一页，提炼最重要的信息。');
      case 'extract':
        _fillPrompt('请提取这一页的关键事实、判断和可复用信息。');
      case 'decisions':
        _fillPrompt('请整理这一页中已经明确的决定及其依据。');
      case 'next':
        _fillPrompt('请整理这一页的下一步行动和仍未解决的问题。');
      case 'translate':
        _fillPrompt('请把这一页翻译成 XX 语言，保留结构和专业含义。');
      case 'copy':
        _fillPrompt('请根据这一页内容，写一版适合发布到 XX 渠道的文案，目标受众是 XX，语气是 XX。');
      case 'rescan':
        await _rescanProject();
      case 'reimport':
        await _reimport();
    }
  }

  PopupMenuItem<String> _taskMenuItem(
    String value,
    IconData icon,
    String label,
  ) {
    return PopupMenuItem(
      value: value,
      child: ListTile(
        dense: true,
        contentPadding: EdgeInsets.zero,
        leading: Icon(icon, color: AppTheme.textSecondary),
        title: Text(label, style: TextStyle(color: AppTheme.textPrimary)),
      ),
    );
  }

  PopupMenuEntry<String> _taskMenuDivider() => PopupMenuItem<String>(
    enabled: false,
    height: 1,
    padding: EdgeInsets.zero,
    child: Divider(height: 1, color: AppTheme.surface3),
  );

  Future<void> _rescanProject() async {
    if (_busy || _saving || _rescanning) return;
    setState(() {
      _rescanning = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.refreshProjectPage(widget.slug);
      if (!mounted) return;
      ref.invalidate(wikiPageProvider(widget.slug));
      ref.invalidate(wikiPagesProvider);
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('项目目录已重新扫描，资产知识页已更新')));
    } catch (e) {
      if (!mounted) return;
      setState(
        () => _error = '重新扫描失败：${e.toString().replaceFirst('Exception: ', '')}',
      );
    } finally {
      if (mounted) setState(() => _rescanning = false);
    }
  }

  Future<void> _saveReply(Message message) async {
    final content = message.content;
    if (_busy || _saving) return;
    final text = content.trim();
    if (text.isEmpty) return;
    setState(() => _saving = true);
    try {
      // 读页失败（DB 错误/页面在首次 build 后被删）不能被静默放弃：
      // 与下方保存失败的异常同路径反馈。
      final sourcePage = await ref.read(wikiPageProvider(widget.slug).future);
      if (!mounted) return;
      if (sourcePage?.kind == 'project') {
        final confirmed = await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('替换项目知识页正文？'),
            content: const Text('当前 AI 回复将替换整篇正文。原内容会保留在页面修订记录中。'),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('取消'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('替换'),
              ),
            ],
          ),
        );
        if (confirmed != true || !mounted) return;
      }
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      if (sourcePage?.kind == 'project') {
        await repo.saveWikiPageContent(
          slug: widget.slug,
          contentMd: text,
          reason: '页内 AI 对话确认替换项目资产档案',
          expectedUpdatedAt: sourcePage!.updatedAt.toUtc().toIso8601String(),
        );
        if (!mounted) return;
        ref.invalidate(wikiPageProvider(widget.slug));
        ref.invalidate(wikiPagesProvider);
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text('已替换当前项目知识页正文')));
        return;
      }
      final page = await api.createArtifactFromMessage(
        slug: widget.slug,
        messageId: message.id,
        contentType: 'AI 总结',
        title: _deriveTitle(text),
      );
      if (!mounted) return;
      ref.invalidate(wikiDerivativesProvider(widget.slug));
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('已保存为派生产物：${page.title}')));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('保存失败：${e.toString().replaceFirst('Exception: ', '')}'),
        ),
      );
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _reimport() async {
    if (_busy || _saving || _reimporting) return;
    final WikiPage page;
    try {
      final loaded = await ref.read(wikiPageProvider(widget.slug).future);
      if (!mounted) return;
      if (loaded == null) {
        setState(() => _error = '页面不存在，可能已被删除');
        return;
      }
      page = loaded;
    } catch (e) {
      if (!mounted) return;
      setState(
        () => _error = '读取页面失败：${e.toString().replaceFirst('Exception: ', '')}',
      );
      return;
    }
    final url = page.sourceUrl;
    if (url == null || url.trim().isEmpty) return;
    // 本地目录来源（file://）：按记录路径重扫目录，不走 HTTP 抓取。
    if (page.isLocalPath) {
      setState(() {
        _reimporting = true;
        _error = null;
      });
      try {
        final repo =
            ref.read(storageRepositoryProvider) as RustBridgeRepository;
        await repo.refreshProjectPage(page.slug);
        if (!mounted) return;
        ref.invalidate(wikiPageProvider(widget.slug));
        ref.invalidate(wikiPagesProvider);
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text('项目目录已重新扫描，知识页已更新')));
      } catch (e) {
        if (!mounted) return;
        setState(
          () =>
              _error = '重新扫描失败：${e.toString().replaceFirst('Exception: ', '')}',
        );
      } finally {
        if (mounted) setState(() => _reimporting = false);
      }
      return;
    }
    setState(() {
      _reimporting = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final fetched = await repo.fetchImportUrl(url);
      if (!mounted) return;
      final saved = await saveKnowledgeImport(context, ref, fetched, page.tags);
      if (saved == null) return;
      if (!mounted) return;
      ref.invalidate(wikiPageProvider(widget.slug));
      ref.invalidate(wikiPagesProvider);
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('已重新导入，知识页内容已更新')));
    } catch (e) {
      if (!mounted) return;
      setState(
        () => _error = '重新导入失败：${e.toString().replaceFirst('Exception: ', '')}',
      );
    } finally {
      if (mounted) setState(() => _reimporting = false);
    }
  }

  String _deriveTitle(String content) {
    for (final line in content.split('\n')) {
      final t = line.replaceAll(RegExp(r'^[#>*\-\s]+'), '').trim();
      if (t.isNotEmpty) {
        return t.length > 40 ? '${t.substring(0, 40)}…' : t;
      }
    }
    return 'AI 总结';
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

  Widget _buildMessage(Message message) {
    final isProject =
        ref.watch(wikiPageProvider(widget.slug)).value?.kind == 'project';
    final bubble = WikiChatBubble(
      message: ContentChatMessage(
        role: message.role.name,
        content: message.content,
      ),
      onExpand: message.role == MessageRole.assistant
          ? () => _showExpandedReply(message.content)
          : null,
    );
    if (message.role != MessageRole.assistant ||
        message.content.trim().isEmpty) {
      return bubble;
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        bubble,
        Padding(
          padding: const EdgeInsets.only(left: 40, bottom: AppTheme.space2),
          child: Wrap(
            spacing: 4,
            children: [
              if (message.content.contains('[['))
                KnowledgeCitationsButton(messageId: message.id),
              if (kSuggestionFeedbackEnabled)
                SuggestionFeedbackControl(
                  messageId: message.id,
                  content: message.content,
                ),
              TextButton.icon(
                onPressed: (_busy || _saving)
                    ? null
                    : () => _saveReply(message),
                icon: Icon(
                  isProject
                      ? Icons.find_replace_outlined
                      : Icons.bookmark_add_outlined,
                  size: 14,
                ),
                label: Text(isProject ? '替换正文' : '保存为产出'),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void _showExpandedReply(String content) {
    showDialog<void>(
      context: context,
      builder: (context) => Dialog(
        child: ConstrainedBox(
          constraints: BoxConstraints(
            maxWidth: 900,
            maxHeight: MediaQuery.sizeOf(context).height * .88,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 12, 12, 8),
                child: Row(
                  children: [
                    const Expanded(child: Text('AI 回复')),
                    IconButton(
                      tooltip: '关闭',
                      onPressed: () => Navigator.pop(context),
                      icon: const Icon(Icons.close),
                    ),
                  ],
                ),
              ),
              const Divider(height: 1),
              Flexible(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(24),
                  child: SelectionArea(child: MarkdownView(markdown: content)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _returnToMainConversation() async {
    final main = await ref.read(mainConversationProvider.future);
    if (!mounted) return;
    ref.read(selectedConversationIdProvider.notifier).set(main.id);
    ref.read(sidebarTabProvider.notifier).set(SidebarTab.conversation);
  }

  @override
  Widget build(BuildContext context) {
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
              for (final m in _messages) _buildMessage(m),
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
                            _taskMenuItem('translate', Icons.translate, '翻译'),
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
                        maxLines: 1,
                        textInputAction: TextInputAction.send,
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
          );
        },
      ),
    );
  }
}

/// 聊天气泡：页面 AI 对话与导入预览共用
class WikiChatBubble extends StatelessWidget {
  final ContentChatMessage message;
  final bool pending;
  final VoidCallback? onExpand;

  const WikiChatBubble({super.key, required this.message, this.onExpand})
    : pending = false;

  const WikiChatBubble.pending({super.key})
    : message = const ContentChatMessage(role: 'assistant', content: ''),
      pending = true,
      onExpand = null;

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
            child: GestureDetector(
              onDoubleTap: onExpand,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
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
                    : isUser
                    ? ContentFontScope(
                        child: SelectableText(
                          message.content,
                          style: TextStyle(
                            fontSize: 13,
                            height: 1.55,
                            color: AppTheme.textPrimary,
                          ),
                        ),
                      )
                    : MarkdownView(
                        markdown: message.content,
                        baseStyle: TextStyle(
                          fontSize: 13,
                          height: 1.55,
                          color: AppTheme.textPrimary,
                        ),
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
