part of 'wiki_page_detail_view.dart';

// ─────────────────────────────────────────────
// 任意网址预览 tab：内容 + 保存（点保存才入库）
// ─────────────────────────────────────────────

class _ImportFetchTabBody extends ConsumerStatefulWidget {
  final ImportFetch fetch;

  const _ImportFetchTabBody({required this.fetch});

  @override
  ConsumerState<_ImportFetchTabBody> createState() =>
      _ImportFetchTabBodyState();
}

class _ImportFetchTabBodyState extends ConsumerState<_ImportFetchTabBody> {
  bool _saving = false;
  bool _saved = false;
  String? _saveError;
  final _tagsController = TextEditingController();

  @override
  void dispose() {
    _tagsController.dispose();
    super.dispose();
  }

  /// 解析标签输入：空格 / 逗号 / 顿号分隔，去掉 # 前缀
  List<String> _parsedTags() => _tagsController.text
      .split(RegExp(r'[\s,，、]+'))
      .map((t) => t.trim().replaceFirst(RegExp(r'^#+'), '').trim())
      .where((t) => t.isNotEmpty)
      .toList();

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
        widget.fetch,
        _parsedTags(),
      );
      if (page == null) return;
      final inputRecordId = widget.fetch.inputRecordId;
      if (inputRecordId != null) {
        await repo.finishUrlInput(inputRecordId, wikiPageSlug: page.slug);
      }
      if (!mounted) return;
      setState(() => _saved = true);
      ref.invalidate(wikiPagesProvider);
      openWikiPageTab(ref, page);
      closeWikiTab(ref, wikiImportTabId(widget.fetch));
    } catch (e) {
      if (!mounted) return;
      setState(() => _saveError = e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final fetch = widget.fetch;
    final title = fetch.displayTitle;
    final host = Uri.tryParse(fetch.sourceUrl)?.host ?? fetch.sourceUrl;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // 头部：标题 + 来源 + 保存
        Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space6,
            AppTheme.space4,
            AppTheme.space6,
            AppTheme.space4,
          ),
          decoration: BoxDecoration(
            border: Border(
              bottom: BorderSide(color: AppTheme.surface3, width: 1),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: fetch.isTweet
                          ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                          : AppTheme.surface3,
                      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                    ),
                    child: Text(
                      fetch.isTweet ? '推文' : '网页',
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: fetch.isTweet
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                        color: AppTheme.textPrimary,
                        height: 1.3,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AppTheme.space2),
              Row(
                children: [
                  Icon(Icons.link, size: 12, color: AppTheme.textTertiary),
                  const SizedBox(width: 4),
                  Expanded(
                    child: Text(
                      host,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              TextField(
                controller: _tagsController,
                enabled: !_saving && !_saved,
                style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
                cursorColor: AppTheme.accentPrimary,
                decoration: InputDecoration(
                  hintText: '标签（可选，空格分隔，例如：方法 投资）',
                  hintStyle: TextStyle(
                    fontSize: 12.5,
                    color: AppTheme.textTertiary,
                  ),
                  prefixIcon: Icon(Icons.sell_outlined, size: 15),
                  prefixIconConstraints: const BoxConstraints(
                    minWidth: 32,
                    minHeight: 32,
                  ),
                  isDense: true,
                  contentPadding: const EdgeInsets.symmetric(vertical: 8),
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
                      width: 1.5,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: AppTheme.space2),
              Row(
                children: [
                  const Spacer(),
                  if (_saved)
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 6,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.accentPrimary.withValues(alpha: 0.14),
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusMedium,
                        ),
                      ),
                      child: Row(
                        children: [
                          Icon(
                            Icons.check,
                            size: 14,
                            color: AppTheme.accentPrimary,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            '已保存到知识库',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.accentPrimary,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ],
                      ),
                    )
                  else
                    FilledButton.icon(
                      onPressed: _saving ? null : _save,
                      icon: _saving
                          ? const SizedBox(
                              width: 14,
                              height: 14,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.bookmark_add_outlined, size: 16),
                      label: Text(_saving ? '保存中…' : '保存到知识库'),
                    ),
                ],
              ),
              if (_saveError != null) ...[
                const SizedBox(height: AppTheme.space2),
                Text(
                  _saveError!,
                  style: TextStyle(fontSize: 12, color: AppTheme.error),
                ),
              ],
            ],
          ),
        ),
        // 正文预览
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(AppTheme.space4),
            child: ContentFontScope(
              child: SelectableText(
                fetch.contentMd,
                style: TextStyle(
                  fontSize: 13.5,
                  color: AppTheme.textPrimary,
                  height: 1.7,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 推文预览 tab：内容 + 对话 + 保存
// ─────────────────────────────────────────────

/// 「AI 整理为 Markdown」固定指令：一次性整理、不进对话历史、不写库；
/// 要求模型只输出 Markdown 正文（结果仅用于替换保存入库的正文）。
const String _kBeautifyInstruction =
    '请把上面的抓取内容整理为结构良好的 Markdown 文档：\n'
    '- 保留全部事实与观点，不新增、不删减、不改写立场；\n'
    '- 按语义合理分段；并列要点用无序列表，步骤/流程用有序列表；\n'
    '- 内容较长时提炼 1-3 个「##」小标题；\n'
    '- 裸露链接整理为 [文字](url) 形式；#话题# 与 @提及 原样保留；\n'
    '- 只输出 Markdown 正文本身，不要任何解释、前言或代码围栏。';

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
  String? _saveError;

  // 临时内容对话（不入库，直到点击保存；对话本身也不写库）
  final List<ContentChatMessage> _chat = [];
  bool _chatBusy = false;
  String? _chatError;
  final _chatController = TextEditingController();
  final _chatFocusNode = FocusNode();
  final _scrollController = ScrollController();

  // AI 整理结果（Markdown）：一次性生成、不进对话历史；保存时优先入库此版本
  String? _beautified;
  bool _beautifying = false;
  String? _beautifyError;

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
          title: '${page.title} · 整理稿',
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
          content: Text('已保存到知识库：《${page.title}》'),
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
        _saveError = '保存失败：${e.toString().replaceFirst('Exception: ', '')}';
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
        _chatError = 'AI 回复失败：${e.toString().replaceFirst('Exception: ', '')}';
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
      final reply = await repo.generateContentChat(
        content: widget.fetch.fullContent,
        messages: const [
          ContentChatMessage(role: 'user', content: _kBeautifyInstruction),
        ],
      );
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
        _beautifyError =
            'AI 整理失败：${e.toString().replaceFirst('Exception: ', '')}';
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
              Row(
                children: [
                  Text(
                    '与 AI 讨论这篇推文',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                  const Spacer(),
                  if (_chat.isEmpty)
                    Text(
                      '保存前可先和 AI 过一遍，点「保存到知识库」才入库',
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
                  child: Text(
                    _chatError!,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.error,
                      height: 1.5,
                    ),
                  ),
                ),
              if (_saveError != null)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: Text(
                    _saveError!,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.error,
                      height: 1.5,
                    ),
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
        (fetch.screenName != null ? '@${fetch.screenName}' : '未知作者');
    // 文章型推文用文章标题，普通推文用「{作者} 的推文」
    final mainTitle = (fetch.title?.trim().isNotEmpty ?? false)
        ? fetch.title!.trim()
        : '$author 的推文';
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space6,
        AppTheme.space4,
        AppTheme.space3,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
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
                  '@${fetch.screenName ?? '—'} · 推文 ${fetch.tweetId}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
          if (_saved)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.14),
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.check_circle_outline,
                    size: 14,
                    color: AppTheme.accentPrimary,
                  ),
                  SizedBox(width: 4),
                  Text(
                    '已保存',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.accentPrimary,
                    ),
                  ),
                ],
              ),
            )
          else
            FilledButton.icon(
              onPressed: _saving ? null : _save,
              icon: _saving
                  ? const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.bookmark_add_outlined, size: 16),
              label: Text(_saving ? '保存中…' : '保存到知识库'),
            ),
        ],
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
              Text(
                '抓取内容（原文）',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
              const Spacer(),
              InkWell(
                onTap: () {
                  Clipboard.setData(ClipboardData(text: fetch.url));
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(
                      content: Text('推文链接已复制'),
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
        Row(
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
                    ? 'AI 整理中…'
                    : (_beautified == null ? 'AI 整理为 Markdown' : '重新整理'),
              ),
            ),
            if (_beautified != null) ...[
              TextButton(
                onPressed: () => setState(() => _beautified = null),
                child: const Text('放弃整理'),
              ),
              const Spacer(),
              Text(
                '保存原文，整理稿另存为产物',
                style: TextStyle(fontSize: 11, color: AppTheme.accentPrimary),
              ),
            ],
          ],
        ),
        if (_beautifyError != null)
          Padding(
            padding: const EdgeInsets.only(top: AppTheme.space2),
            child: Text(
              _beautifyError!,
              style: TextStyle(
                fontSize: 12,
                color: AppTheme.error,
                height: 1.5,
              ),
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
                  'AI 整理结果（Markdown 预览）',
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
                hintText: '就这篇推文问问 AI…（回车发送，Shift+回车换行）',
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
            tooltip: '发送',
          ),
        ],
      ),
    );
  }
}
