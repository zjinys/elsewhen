part of '../wiki_ai_chat_panel.dart';

//! 任务与恢复域（task 恢复/重导/重扫/回复保存）
extension _PanelTask on _WikiAiChatPanelState {
  Future<void> _resetTask() async {
    final slug = widget.slug;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      final material = await repo.getWikiChatMaterial(slug);
      if (!mounted || widget.slug != slug) return;
      setState(() {
        _pageTask = ChatPageTask('local_question', [material]);
        _taskRestoreError = null;
      });
    } catch (e) {
      if (mounted && widget.slug == slug) {
        setState(() => _taskRestoreError = '重新读取材料失败：$e');
      }
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
      setState(
        () => _error = '消息加载失败：${e.toString().replaceFirst('Exception: ', '')}',
      );
    }
  }

  Future<void> _send(
    String raw, {
    bool recordOnly = false,
    bool supplement = false,
  }) async {
    final text = raw.trim();
    final id = _conversationId;
    if (text.isEmpty || id == null || _inputBusy) return;
    final target = ref.read(conversationRepositoryProvider).supportsChatRuns
        ? ref.read(chatRunControllerProvider(id)).state.run
        : null;
    if (supplement && (target == null || !chatRunIsActive(target.status))) {
      setState(() => _error = '当前没有可补充的任务，请选择保存或排队。');
      return;
    }
    setState(() {
      _sending = true;
      _error = null;
    });
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final durable = ref.read(conversationRepositoryProvider).supportsChatRuns;
    final generatingNotifier = ref.read(aiGeneratingProvider.notifier);
    try {
      final signature = '$id\n$text\n${_pageTask?.json}';
      if (_pendingInputSignature != signature) {
        _pendingInputSignature = signature;
        _pendingInputKey =
            'wiki-${DateTime.now().microsecondsSinceEpoch}-${Random.secure().nextInt(1 << 32)}';
      }
      final saved = repo.supportsPageTasks && _pageTask != null
          ? await repo.submitWikiChatInput(
              id,
              text,
              idempotencyKey: _pendingInputKey!,
              taskJson: _pageTask!.json,
            )
          : await repo.sendMessage(id, 'user', text);
      if (!mounted || _conversationId != id) return;
      _pendingInputSignature = null;
      _pendingInputKey = null;
      _inputController.clear();
      setState(() => _messages.add(saved));
      _scrollToBottom();
      if (durable) {
        ref.invalidate(wikiChatMessagesProvider(id));
        if (recordOnly) return;
        try {
          final repository = ref.read(conversationRepositoryProvider);
          if (supplement) {
            await repository.enqueueChatSupplement(id, saved.id, target!.id);
          } else if (repository.supportsChatQueue) {
            await repository.scheduleChatInput(id, saved.id);
            if (mounted)
              await ref.read(chatRunControllerProvider(id)).restore();
          } else {
            await ref.read(chatRunControllerProvider(id)).start(saved.id);
          }
        } catch (_) {
          if (mounted)
            setState(() => _error = '输入已保存，但生成资格未确认；请从该消息继续生成，不要重新发送。');
        }
      } else {
        setAiGenerating(ref, id, true);
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
        ref.invalidate(wikiPageProvider(widget.slug));
        ref.invalidate(pageRelationsProvider(widget.slug));
        ref.invalidate(wikiDerivativesProvider(widget.slug));
        ref.invalidate(wikiPagesProvider);
      }
    } catch (e) {
      if (!mounted || _conversationId != id) return;
      setState(() => _error = e.toString().replaceFirst('Exception: ', ''));
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

  void _fillPrompt(String prompt) {
    _inputController
      ..text = prompt
      ..selection = TextSelection.collapsed(offset: prompt.length);
  }

  Future<void> _handleTask(String action) async {
    if (_pageTask != null &&
        _pageTask!.materials.isNotEmpty &&
        {'summary', 'extract', 'decisions', 'next', 'copy'}.contains(action)) {
      setState(
        () => _pageTask = ChatPageTask(
          'full_summary',
          _pageTask!.materials.take(1).toList(),
        ),
      );
    }
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
        if (_pageTask != null && _pageTask!.materials.isNotEmpty) {
          setState(
            () => _pageTask = ChatPageTask(
              'local_question',
              _pageTask!.materials.take(1).toList(),
              startChar: _pageTask!.startChar,
              endChar: _pageTask!.endChar,
            ),
          );
        }
        _fillPrompt('请把选定的原文片段翻译成 XX 语言，保留结构和专业含义；只处理已提供的片段，先说明范围，不声称翻译整页。');
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
}
