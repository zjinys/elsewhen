part of '../wiki_page_detail_view.dart';

//! AI 整理 Markdown tab（tweet 校验+预览保存）

// ─────────────────────────────────────────────
// 推文预览 tab：内容 + 对话 + 保存
// ─────────────────────────────────────────────

/// 剥掉模型偶尔给整段回复套上的代码围栏（```markdown ... ```），取纯正文。
String _unwrapCodeFence(String reply) {
  final text = reply.trim();
  final match = RegExp(r'^```[a-zA-Z]*\s*\n([\s\S]*?)\s*```$').firstMatch(text);
  return match == null ? text : match.group(1)!.trim();
}

class _TweetTabBody extends ConsumerStatefulWidget {
  final TweetFetch fetch;

  const _TweetTabBody({required this.fetch});

  @override
  ConsumerState<_TweetTabBody> createState() => _TweetTabBodyState();
}

class _TweetTabBodyState extends ConsumerState<_TweetTabBody> {
  bool _saving = false;
  bool _saved = false;
  Object? _saveError;

  // 临时内容对话（不入库，直到点击保存；对话本身也不写库）
  final List<ContentChatMessage> _chat = [];
  bool _chatBusy = false;
  Object? _chatError;
  final _chatController = TextEditingController();
  final _chatFocusNode = FocusNode();
  final _scrollController = ScrollController();

  // AI 整理结果（Markdown）：一次性生成、不进对话历史；保存时优先入库此版本
  String? _beautified;
  bool _beautifying = false;
  Object? _beautifyError;

  @override
  void initState() {
    super.initState();
    // 回车直接发送；Shift+回车换行（与 MessageArea 一致）
    _chatFocusNode.onKeyEvent = (node, event) {
      final isEnter = event.logicalKey == LogicalKeyboardKey.enter;
      final isShift = HardwareKeyboard.instance.isShiftPressed;
      if (event is KeyDownEvent && isEnter && !isShift) {
        _sendChat(_chatController.text);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    };
  }

  @override
  void dispose() {
    _chatController.dispose();
    _chatFocusNode.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_saving || _saved) return;
    setState(() {
      _saving = true;
      _saveError = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final page = await saveKnowledgeImport(
        context,
        ref,
        ImportFetch(
          sourceUrl: widget.fetch.url,
          sourceKind: 'tweet',
          contentMd: widget.fetch.text,
          title: widget.fetch.title,
          authorName: widget.fetch.authorName,
          screenName: widget.fetch.screenName,
        ),
        const ['tweet'],
      );
      if (page == null) {
        if (mounted) setState(() => _saving = false);
        return;
      }
      if (_beautified != null) {
        await repo.createWikiDerivative(
          basedOnSlug: page.slug,
          contentType: 'summary',
          title: '${page.title} · Markdown',
          contentMd: _beautified!,
        );
      }
      final inputRecordId = widget.fetch.inputRecordId;
      if (inputRecordId != null) {
        await repo.finishUrlInput(inputRecordId, wikiPageSlug: page.slug);
      }
      if (!mounted) return;
      setState(() {
        _saving = false;
        _saved = true;
      });
      // 刷新左侧知识库列表，让「来源」分组出现新页
      ref.invalidate(wikiPagesProvider);
      // 本次『重新抓取+保存』更新的是同一 slug 的旧页：使页面内容 provider 失效，
      // 否则已缓存的旧内容会一直显示（比如旧版本存的纯链接）。
      ref.invalidate(wikiPageProvider(page.slug));
      ref.invalidate(pageRelationsProvider(page.slug));
      // 直接打开已保存的页面，立即看到最新内容，避免再点左侧列表却读到缓存旧页
      openWikiPageTab(ref, page);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.l10n.wikiSavedTitle(page.title)),
          behavior: SnackBarBehavior.floating,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(999)),
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _saveError = e;
      });
    }
  }

  Future<void> _sendChat(String raw) async {
    final text = raw.trim();
    if (text.isEmpty || _chatBusy) return;
    setState(() {
      _chat.add(ContentChatMessage(role: 'user', content: text));
      _chatError = null;
      _chatBusy = true;
    });
    _chatController.clear();
    _scrollToBottom();
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      // 只传最近若干条，控制单次请求体积
      final history = _chat.length > 24
          ? _chat.sublist(_chat.length - 24)
          : List<ContentChatMessage>.from(_chat);
      final reply = await repo.generateContentChat(
        content: widget.fetch.fullContent,
        messages: history,
      );
      if (!mounted) return;
      setState(() {
        _chat.add(ContentChatMessage(role: 'assistant', content: reply));
        _chatBusy = false;
      });
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _chatBusy = false;
        _chatError = e;
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

  /// 「AI 整理为 Markdown」：复用一次性内容对话入口（固定指令，不进聊天历史、
  /// 不写库）；保存时原文仍保留，整理稿作为附有来源的独立产物保存。
  Future<void> _beautify() async {
    if (_beautifying || _saved || _saving) return;
    setState(() {
      _beautifying = true;
      _beautifyError = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final reply = await repo.formatImportContent(widget.fetch.fullContent);
      if (!mounted) return;
      setState(() {
        _beautified = _unwrapCodeFence(reply);
        _beautifying = false;
      });
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _beautifying = false;
        _beautifyError = e;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _buildHeader(),
        Divider(height: 1, color: AppTheme.surface3),
        Expanded(
          child: ListView(
            controller: _scrollController,
            padding: const EdgeInsets.all(AppTheme.space4),
            children: [
              _buildTweetCard(),
              const SizedBox(height: AppTheme.space3),
              _buildBeautifySection(),
              const SizedBox(height: AppTheme.space4),
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    context.l10n.tweetDiscuss,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                  const SizedBox(height: 4),
                  if (_chat.isEmpty)
                    Text(
                      context.l10n.tweetDiscussHelp,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              for (final msg in _chat) WikiChatBubble(message: msg),
              if (_chatBusy) const WikiChatBubble.pending(),
              if (_chatError != null)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: AppErrorMessage(
                    action: context.l10n.aiReply,
                    error: _chatError!,
                  ),
                ),
              if (_saveError != null)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: AppErrorMessage(
                    action: context.l10n.save,
                    error: _saveError!,
                  ),
                ),
            ],
          ),
        ),
        _buildChatInput(),
      ],
    );
  }

  Widget _buildHeader() {
    final fetch = widget.fetch;
    final author =
        fetch.authorName ??
        (fetch.screenName != null
            ? '@${fetch.screenName}'
            : context.l10n.tweetUnknownAuthor);
    final mainTitle = (fetch.title?.trim().isNotEmpty ?? false)
        ? fetch.title!.trim()
        : context.l10n.tweetByAuthor(author);
    final heading = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          mainTitle,
          style: TextStyle(
            fontSize: 16,
            fontWeight: FontWeight.w700,
            color: AppTheme.textPrimary,
            height: 1.3,
          ),
        ),
        const SizedBox(height: 2),
        Text(
          '@${fetch.screenName ?? '—'} · ${context.l10n.wikiPostTab(fetch.tweetId)}',
          style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
        ),
      ],
    );
    final save = _saved
        ? Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.check_circle_outline,
                size: 14,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 4),
              Text(
                context.l10n.tweetSaved,
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.accentPrimary,
                ),
              ),
            ],
          )
        : FilledButton.icon(
            onPressed: _saving ? null : _save,
            icon: _saving
                ? const SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.bookmark_add_outlined, size: 16),
            label: Text(_saving ? context.l10n.saving : context.l10n.wikiSave),
          );
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space6,
        AppTheme.space4,
        AppTheme.space3,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) => constraints.maxWidth < 560
            ? Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  heading,
                  const SizedBox(height: 8),
                  Align(alignment: Alignment.centerRight, child: save),
                ],
              )
            : Row(
                children: [
                  Expanded(child: heading),
                  const SizedBox(width: 16),
                  save,
                ],
              ),
      ),
    );
  }

  Widget _buildTweetCard() {
    final fetch = widget.fetch;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.article_outlined,
                size: 14,
                color: AppTheme.textTertiary,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  context.l10n.tweetOriginal,
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
              ),
              InkWell(
                onTap: () {
                  Clipboard.setData(ClipboardData(text: fetch.url));
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text(context.l10n.tweetLinkCopied),
                      behavior: SnackBarBehavior.floating,
                    ),
                  );
                },
                borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                child: Padding(
                  padding: EdgeInsets.all(4),
                  child: Icon(
                    Icons.link,
                    size: 14,
                    color: AppTheme.textTertiary,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          ContentFontScope(
            child: SelectableText(
              fetch.text,
              style: TextStyle(
                fontSize: 14,
                height: 1.7,
                color: AppTheme.textPrimary,
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// 「AI 整理为 Markdown」区块：触发按钮 + 整理结果预览（MarkdownView 渲染）。
  /// 与下方「与 AI 讨论」互不干扰：一次性整理不走对话历史、不写库。
  Widget _buildBeautifySection() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            TextButton.icon(
              onPressed: (_beautifying || _saved || _saving) ? null : _beautify,
              icon: _beautifying
                  ? const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.auto_awesome_outlined, size: 16),
              label: Text(
                _beautifying
                    ? context.l10n.tweetFormatting
                    : (_beautified == null
                          ? context.l10n.tweetFormat
                          : context.l10n.tweetFormatAgain),
              ),
            ),
            if (_beautified != null) ...[
              TextButton(
                onPressed: () => setState(() => _beautified = null),
                child: Text(context.l10n.tweetDiscardFormat),
              ),
              Text(
                context.l10n.tweetSeparateDraftHelp,
                style: TextStyle(fontSize: 11, color: AppTheme.accentPrimary),
              ),
            ],
          ],
        ),
        if (_beautifyError != null)
          Padding(
            padding: const EdgeInsets.only(top: AppTheme.space2),
            child: AppErrorMessage(
              action: context.l10n.tweetFormat,
              error: _beautifyError!,
            ),
          ),
        if (_beautified != null) ...[
          const SizedBox(height: AppTheme.space2),
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(AppTheme.space4),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
              border: Border.all(
                color: AppTheme.accentPrimary.withValues(alpha: 0.35),
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  context.l10n.tweetFormatPreview,
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
                const SizedBox(height: 8),
                MarkdownView(markdown: _beautified!),
              ],
            ),
          ),
        ],
      ],
    );
  }

  Widget _buildChatInput() {
    return Container(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space4,
        AppTheme.space3,
        AppTheme.space4,
        AppTheme.space4,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Expanded(
            child: TextField(
              controller: _chatController,
              focusNode: _chatFocusNode,
              enabled: !_chatBusy,
              minLines: 1,
              maxLines: 4,
              decoration: InputDecoration(
                hintText: context.l10n.tweetChatHint,
                hintStyle: TextStyle(
                  fontSize: 13,
                  color: AppTheme.textTertiary,
                ),
                filled: true,
                fillColor: AppTheme.surface2,
                isDense: true,
                contentPadding: EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 10,
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(
                    color: AppTheme.accentPrimary,
                    width: 1.5,
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          IconButton.filled(
            onPressed: _chatBusy ? null : () => _sendChat(_chatController.text),
            style: IconButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
              disabledBackgroundColor: AppTheme.surface3,
            ),
            icon: const Icon(Icons.arrow_upward, size: 16),
            tooltip: context.l10n.send,
          ),
        ],
      ),
    );
  }
}
