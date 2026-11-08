part of '../wiki_ai_chat_panel.dart';

// ignore_for_file: invalid_use_of_protected_member

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
        setState(() => _taskRestoreError = appFailure(e));
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
      setState(() => _error = appFailure(e));
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
      setState(() => _error = appNotice('noSupplementTask'));
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
        _fillPrompt(context.l10n.summarizePagePrompt);
      case 'extract':
        _fillPrompt(context.l10n.keyPointsPrompt);
      case 'decisions':
        _fillPrompt(context.l10n.decisionsPrompt);
      case 'next':
        _fillPrompt(context.l10n.nextStepsPrompt);
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
        _fillPrompt(context.l10n.translateSelectionPrompt);
      case 'copy':
        _fillPrompt(context.l10n.writeCopyPrompt);
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.projectRescannedAssets)),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = appFailure(e));
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
            title: Text(context.l10n.replaceProjectPage),
            content: Text(context.l10n.replaceProjectDescription),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: Text(context.l10n.cancel),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: Text(context.l10n.replace),
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
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.projectPageReplaced)),
        );
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.savedAsDerivative(page.title))),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(appFailure(e))));
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
        setState(() => _error = appNotice('pageMissing'));
        return;
      }
      page = loaded;
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = appFailure(e));
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
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(context.l10n.projectRescanned)));
      } catch (e) {
        if (!mounted) return;
        setState(() => _error = appFailure(e));
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
          .showSnackBar(SnackBar(content: Text(context.l10n.sourceReimported)));
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = appFailure(e));
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
    return context.l10n.aiSummary;
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
