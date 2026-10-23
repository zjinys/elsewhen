part of '../message_area.dart';

//! 底部消息输入区。

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

  Future<void> _handleSubmit({
    ChatInputMode mode = ChatInputMode.generate,
  }) async {
    final recordOnly = mode == ChatInputMode.recordOnly;
    final text = _controller.text.trim();
    if (text.isEmpty || _isSubmitting) return;

    final conversationId = ref.read(selectedConversationIdProvider);
    if (conversationId == null) {
      _showError('请先选择一个会话');
      return;
    }
    final repository = ref.read(conversationRepositoryProvider);
    final running =
        ref.read(aiGeneratingProvider).contains(conversationId) ||
        (repository.supportsChatRuns &&
            ref.read(chatRunControllerProvider(conversationId)).state.busy);
    if (running && !repository.supportsChatQueue && !recordOnly) return;
    // Bind the displayed run before saving. If it finishes during the save,
    // Rust preserves the input without silently steering a different run.
    final targetRun = repository.supportsChatRuns
        ? ref.read(chatRunControllerProvider(conversationId)).state.run
        : null;
    if (mode == ChatInputMode.supplement &&
        (targetRun == null || !chatRunIsActive(targetRun.status))) {
      _showError('当前没有可补充的任务，请选择保存或排队。');
      return;
    }

    setState(() => _isSubmitting = true);
    final submissionKey = _submissionKeyFor(text);
    String? inputMessageId;

    try {
      final topic = explicitTopicName(text);
      if (topic != null && !recordOnly && !running) {
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
      if (isUrl && !recordOnly && !running) {
        await _routeUrlToImportPreview(text, submissionKey);
        _controller.clear();
        _pendingSubmissionText = null;
        _pendingSubmissionKey = null;
        return;
      }
      final repo = ref.read(conversationRepositoryProvider);
      // 先把用户消息写入库（不依赖 AI）；返回的消息 id 是后续 AI 生成的
      // 输入锚点（chat run 绑定 inputMessageId，失败重试也钉在这条消息上）。
      final sentMessage = await repo.sendMessage(
        conversationId,
        text,
        idempotencyKey: submissionKey,
      );
      inputMessageId = sentMessage.id;
      if (!mounted) return;

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

    if (!mounted || recordOnly) return;
    if (repository.supportsChatQueue) {
      try {
        if (mode == ChatInputMode.supplement) {
          await repository.enqueueChatSupplement(
            conversationId,
            inputMessageId,
            targetRun!.id,
          );
        } else {
          await repository.scheduleChatInput(conversationId, inputMessageId);
          if (mounted)
            await ref.read(chatRunControllerProvider(conversationId)).restore();
        }
      } catch (_) {
        if (mounted) _showError('输入已保存，但生成资格未确认。请从该消息继续生成，不要重新发送。');
      }
      return;
    }
    // 接着触发 AI 分析生成回复（不阻塞输入，出错单独反馈）。
    // 「生成中」状态由 _generateAiReply 内部设置，确保 invalidate
    // 时列表仍在 data 分支（可渲染 notice），而非 AsyncLoading（空跑）。
    await _generateAiReply(conversationId, inputMessageId);
  }

  Future<void> _enterTopic(String title) async {
    final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    WikiPageRef? existing;
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

  Future<void> _generateAiReply(
    String conversationId,
    String inputMessageId,
  ) async {
    // The saved message is the request identity, never a later "latest" input.
    await runAiGeneration(
      ref,
      conversationId,
      inputMessageId: inputMessageId,
      isMounted: () => mounted,
      onError: () {
        if (mounted) setState(() {});
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final selectedId = ref.watch(selectedConversationIdProvider);
    // 「存为主题」预填：AI 气泡悬浮按钮写入 provider，这里填进输入框并聚焦。
    // 填一次即清空 provider（置 null），避免切会话/重建时重复填入。
    if (selectedId != null) {
      ref.listen(saveTopicDraftProvider(selectedId), (prev, next) {
        if (next != null) {
          _controller.text = next;
          _controller.selection = TextSelection.fromPosition(
            TextPosition(offset: next.length),
          );
          _focusNode.requestFocus();
          ref.read(saveTopicDraftProvider(selectedId).notifier).set(null);
        }
      });
      // 习惯问候「聊聊」预填：同上，填一次即清空，绝不自动发送。
      ref.listen(habitChatDraftProvider(selectedId), (prev, next) {
        if (next != null) {
          _controller.text = next;
          _controller.selection = TextSelection.fromPosition(
            TextPosition(offset: next.length),
          );
          _focusNode.requestFocus();
          ref.read(habitChatDraftProvider(selectedId).notifier).set(null);
        }
      });
    }
    final durableBusy =
        selectedId != null &&
        ref.watch(conversationRepositoryProvider).supportsChatRuns &&
        ref.watch(
          chatRunStateProvider(selectedId)
              .select((value) => value.value?.busy ?? true),
        );
    final isGenerating =
        selectedId != null &&
        (ref.watch(aiGeneratingProvider).contains(selectedId) || durableBusy);
    final queueEnabled = ref
        .watch(conversationRepositoryProvider)
        .supportsChatQueue;
    final busy = _isSubmitting || (isGenerating && !queueEnabled);
    final pages = ref.watch(wikiPagesProvider).value ?? const [];
    final completionPages = _completionMarker == null
        ? const <WikiPageRef>[]
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
              AppPressable(
                scaleOnPress: 0.92,
                duration: const Duration(milliseconds: 120),
                enabled: !busy,
                child: IconButton.filled(
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
                  tooltip: isGenerating && queueEnabled ? '保存并排队下一问' : '发送',
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
