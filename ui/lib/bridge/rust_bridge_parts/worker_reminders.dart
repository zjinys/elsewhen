part of '../rust_bridge_repository.dart';

//! RustBridgeRepository 便捷薄委托（worker_reminders 域）。
extension RepoWorkerReminders on RustBridgeRepository {
  void _pushAiActivity(AiActivity activity) {
    _backgroundAiActivityStack.add(activity);
    backgroundAiActivity.value = activity;
  }

  void _popAiActivity(AiActivity activity) {
    final index = _backgroundAiActivityStack.lastIndexOf(activity);
    if (index >= 0) _backgroundAiActivityStack.removeAt(index);
    backgroundAiActivity.value = _backgroundAiActivityStack.isEmpty
        ? null
        : _backgroundAiActivityStack.last;
  }

  Future<T> _withAiActivity<T>(
    AiActivity activity,
    Future<T> Function() body,
  ) async {
    _pushAiActivity(activity);
    try {
      return await body();
    } finally {
      _popAiActivity(activity);
    }
  }

  void _wakeAnalysisWorker() {
    _idleWakes = 0;
    if (!_initialized || _disposed || !runBackgroundWorker) return;
    if (_analysisWorkerRunning) {
      _analysisRerunRequested = true;
      return;
    }
    if (_analysisWorkerScheduled) return;
    _analysisWorkerScheduled = true;
    Future<void>(() async {
      _analysisWorkerScheduled = false;
      if (_analysisWorkerRunning || _disposed) return;
      _analysisWorkerRunning = true;
      try {
        do {
          _analysisRerunRequested = false;
          // 定时提醒/习惯问候由 5s 定时器单独驱动（含空闲退避期间），
          // 这里不再重复 tick，避免同一拍做两次本地查询。
          // Keep draining bounded Rust batches while pending work is
          // immediately available. Retry jobs are revisited by the timer once
          // available_at expires.
          _pushAiActivity(AiActivity.analysis);
          try {
            while (!_disposed) {
              final result = await triggerAnalysis();
              // 结构化结果取代字符串契约（P2-9）：只有明确“已处理”才继续
              // 排空下一批；NoProvider 停表，交给启动/周期 5s timer 重唤。
              if (result is! api.AnalysisTriggerResult_Processed ||
                  result.count <= 0) {
                break;
              }
              final stats = await getAnalysisJobStats();
              if (stats.pending == 0) break;
            }
          } finally {
            _popAiActivity(AiActivity.analysis);
          }
        } while (_analysisRerunRequested && !_disposed);
        // 分析排空后再做知识消化：同一 worker 串行，避免与分析并发调用 provider；
        // 消化依赖分析结果（可记录性），先分析后消化也最省重试。
        await _drainKnowledgeDigest();
        // 周期报告 + 偏差检测（R3）：同一 worker 串行（Q4：同一 timer）；
        // 内部自带幂等/空窗跳过/检测退避，仅在有事发生时记日志
        //（该 tick 在空闲期每 ≈30s 一次，逐拍打印会刷屏）。
        try {
          final reportTick = await _withAiActivity(
            AiActivity.periodReport,
            () => api.tickPeriodicReports(),
          );
          final String? eventful = switch (reportTick) {
            api.PeriodReportTickResult_Idle() ||
            api.PeriodReportTickResult_NoProvider() => null,
            _ => reportTick.toString(),
          };
          if (eventful != null) debugPrint('period report: $eventful');
        } catch (e) {
          debugPrint('period report tick failed: $e');
        }
        // 网络同步与分析/消化串行复用同一条 worker 通道：P3 的有界 tick
        // （每 peer ≤3 封 × ≤4 页，≤8 peer）复用持久重试与回执台账，防线在
        // Rust 侧（确认会话 + 工作区租约 + 活跃成员 + 受信路由）。到达的新
        // 事件/知识会让下一拍分析与消化自然接续；视图刷新走知识修订号。
        final before = knowledgeRevision.value;
        await _runNetworkSyncTick();
        if (!_disposed && knowledgeRevision.value == before) {
          // 有新到达时分析与消化已在上面跑过；这里无需额外动作，保留分支
          // 以便将来按到达内容做定向刷新。
        }
        if (!_disposed && !_analysisRerunRequested) {
          final changed = await _withAiActivity(
            AiActivity.insights,
            () => api.tickKnowledgeInsights(),
          );
          if (!_disposed && changed > 0) knowledgeRevision.value++;
        }
      } catch (e) {
        // Queue state remains durable; the next wake retries after startup or
        // the periodic timer without surfacing an error in the save path.
        //
        // 不上抛是有意的（保存路径不该被后台 worker 的失败带崩），但**完全静默**
        // 不好：若每 tick 都因同一个原因失败（比如抢锁 BUSY），就会永远查不出来。
        // 这里只记不抛，让问题在日志里留下痕迹。
        debugPrint('analysis worker failed: $e');
      } finally {
        _analysisWorkerRunning = false;
        if (_analysisRerunRequested) _wakeAnalysisWorker();
      }
    });
  }

  Future<void> _tickDueReminders() async {
    try {
      final due = await api.popDueReminders(
        deviceId: RustBridgeRepository._reminderDeviceId,
      );
      if (due.isNotEmpty) dueReminders.value = due;
    } catch (e) {
      // 故障隔离：提醒 tick 失败只记日志，绝不带崩后续分析/消化/同步。
      debugPrint('due reminders tick failed: $e');
    }
  }

  Future<void> confirmReminder(String todoId) async {
    await api.confirmReminder(
      todoId: todoId,
      deviceId: RustBridgeRepository._reminderDeviceId,
    );
    _clearDueReminder(todoId);
  }

  Future<void> snoozeReminder(String todoId, {int minutes = 10}) async {
    await api.snoozeReminder(
      todoId: todoId,
      minutes: minutes,
      deviceId: RustBridgeRepository._reminderDeviceId,
    );
    _clearDueReminder(todoId);
  }

  void dismissReminder(String todoId) => _clearDueReminder(todoId);

  void _clearDueReminder(String todoId) {
    final cur = dueReminders.value;
    if (cur.any((d) => d.todoId == todoId)) {
      dueReminders.value = cur.where((d) => d.todoId != todoId).toList();
    }
  }

  Future<void> setTodoReminder(String id, bool reminder) =>
      api.setTodoReminder(id: id, reminder: reminder);

  // ── 习惯主动问候 ────────────────────────────────────────────────

  Future<void> _tickHabitNudges() async {
    try {
      final result = await api.tickHabitNudges();
      if (_disposed) return;
      pendingHabitNudges.value = result.pendingCount;
      if (result.messageIds.isNotEmpty) {
        habitNudgeRevision.value++;
      }
    } catch (e) {
      // 故障隔离：问候 tick 失败不能影响提醒/分析/同步。
      debugPrint('habit nudge tick failed: $e');
    }
  }

  Future<api.HabitNudgeSuggestionDto> getHabitNudgeSuggestion(
    String pageSlug,
  ) => api.getHabitNudgeSuggestion(pageSlug: pageSlug);

  Future<api.HabitNudgeDto?> getHabitNudgeForPage(String pageSlug) =>
      api.getHabitNudgeForPage(pageSlug: pageSlug);

  Future<api.HabitNudgeDto?> getHabitNudge(String id) =>
      api.getHabitNudge(id: id);

  Future<api.HabitNudgeDto> saveHabitNudge(api.HabitNudgeInputDto input) async {
    final dto = await api.saveHabitNudge(input: input);
    habitNudgeRevision.value++;
    return dto;
  }

  Future<bool> deleteHabitNudge(String id) async {
    final deleted = await api.deleteHabitNudge(id: id);
    if (deleted) habitNudgeRevision.value++;
    return deleted;
  }

  Future<List<api.HabitNudgeStateDto>> listHabitNudgeStates() =>
      api.listHabitNudgeStates();

  Future<int> pendingHabitNudgeCount() async {
    final count = await api.pendingHabitNudgeCount();
    pendingHabitNudges.value = count;
    return count;
  }

  Future<api.HabitNudgeAnswerDto> answerHabitNudge(
    String messageId,
    bool done,
  ) async {
    final result = await api.answerHabitNudge(
      messageId: messageId,
      response: done ? 'done' : 'missed',
    );
    habitNudgeRevision.value++;
    pendingHabitNudges.value = await api.pendingHabitNudgeCount();
    return result;
  }

  Future<void> snoozeHabitNudge(String messageId) async {
    await api.snoozeHabitNudge(messageId: messageId);
    habitNudgeRevision.value++;
  }

  Future<void> dismissHabitNudge(String messageId) async {
    await api.dismissHabitNudge(messageId: messageId);
    habitNudgeRevision.value++;
    pendingHabitNudges.value = await api.pendingHabitNudgeCount();
  }

  Future<String?> parseReminderTime(String text) =>
      api.parseReminderTime(text: text);

  /// P3 网络同步 tick：锁/占用/未初始化时 Rust 返回 `ran == false`；错误
  /// 只记入状态页，绝不影响本地记录（本地写路径不经过 relay）。
  Future<void> _runNetworkSyncTick() async {
    if (_disposed) return;
    try {
      final result = await api.triggerNetworkSync();
      if (_disposed) return;
      lastSyncTick.value = result;
      lastSyncTickError.value = null;
      if (result.ran && (result.pulledAccepted > 0 || result.uploaded > 0)) {
        // 有新事件到达：唤醒分析队列并让知识视图在消化后刷新。会话/消息
        // 视图只订阅 syncArrivalRevision（拉到东西就即时上台，不等消化）。
        knowledgeRevision.value++;
        if (result.pulledAccepted > 0) syncArrivalRevision.value++;
        _wakeAnalysisWorker();
      }
    } catch (e) {
      if (_disposed) return;
      lastSyncTickError.value = e.toString();
      debugPrint('network sync tick failed: $e');
    }
  }

  /// 逐批排空到期的知识消化任务（没有手动触发入口，只由 worker 调用）。
  /// 有新输入时让位给分析队列（finally 会重新唤醒 worker）。
  Future<void> _drainKnowledgeDigest() async {
    _pushAiActivity(AiActivity.digest);
    try {
      while (!_disposed && !_analysisRerunRequested) {
        final result = await api.triggerKnowledgeDigest();
        // Idle / NoProvider / Failed 都停：失败已退避回队，由 timer 在到期后重试。
        if (result is! api.KnowledgeDigestTickResult_Processed) break;
        if (!_disposed) knowledgeRevision.value++;
      }
    } finally {
      _popAiActivity(AiActivity.digest);
    }
  }

  Future<List<Conversation>> listConversations() async {
    final dtos = await api.listConversations();
    return dtos.map(_fromConversationDto).toList();
  }

  Future<List<Conversation>> listArchivedConversations() async {
    final dtos = await api.listArchivedConversations();
    return dtos.map(_fromConversationDto).toList();
  }

  Conversation _fromConversationDto(api.ConversationDto dto) {
    return Conversation(
      id: dto.id,
      title: dto.title,
      tag: dto.tag,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
      messageCount: dto.messageCount,
      lastMessagePreview: dto.lastMessagePreview,
      archived: dto.archived,
      wikiPageSlug: dto.wikiPageSlug,
    );
  }

  Future<void> renameConversation(String conversationId, String title) =>
      api.renameConversation(conversationId: conversationId, title: title);

  Future<void> setConversationArchived(String conversationId, bool archived) =>
      api.setConversationArchived(
        conversationId: conversationId,
        archived: archived,
      );

  Future<bool> deleteArchivedConversation(String conversationId) =>
      api.deleteArchivedConversation(conversationId: conversationId);

  // ── 个人经验规则库 ──

  Future<List<Rule>> listRules() async {
    final dtos = await api.listRules();
    return dtos
        .map(
          (dto) => Rule(
            id: dto.id,
            content: dto.content,
            status: dto.status,
            createdAt: DateTime.parse(dto.createdAt).toLocal(),
            triggerScene: dto.triggerScene,
            triggerKeywords: dto.triggerKeywords,
          ),
        )
        .toList();
  }

  Future<String> addRule(String content) => api.addRule(content: content);

  Future<void> deleteRule(String ruleId) => api.deleteRule(ruleId: ruleId);

  /// 配置规则触发（决策辅助 D1）：keywords 空 = 不参与事件预筛。
  Future<void> setRuleTrigger(
    String ruleId, {
    required String triggerScene,
    required List<String> triggerKeywords,
  }) => api.setRuleTrigger(
    ruleId: ruleId,
    triggerScene: triggerScene,
    triggerKeywords: triggerKeywords,
  );

  /// 提醒偏好：instant / review_only / off。
  Future<void> setRuleTriggerMode(String mode) =>
      api.setRuleTriggerMode(mode: mode);

  Future<String> getRuleTriggerMode() => api.getRuleTriggerMode();

  /// D6 采纳反馈：记录 / 清除（feedback 传 null）。
  Future<void> setRuleHitFeedback(String id, String? feedback) =>
      api.setRuleHitFeedback(id: id, feedback: feedback);

  /// D6 情感回访：读 / 写偏好；打开应用时的一次检查。
  Future<api.CompanionPrefsDto> getCompanionPrefs() => api.getCompanionPrefs();
  Future<void> setCompanionPrefs({
    required bool enabled,
    required int intervalDays,
  }) => api.setCompanionPrefs(enabled: enabled, intervalDays: intervalDays);
  Future<api.CompanionRevisitDto?> checkCompanionRevisit() =>
      api.checkCompanionRevisit();

  /// 最近的即时提醒与聚合（规则库设置的「最近提醒」读面）。
  /// 最近事件读面（遗留收口切片 C）：列表卡不拖分析结果。
  Future<List<api.EventSummaryDto>> listRecentEvents({
    int days = 3650,
    int limit = 50,
  }) => api.listRecentEvents(days: days, limit: limit);

  /// 事件旁标记（D1 旧账）：某事件的未拒绝命中。
  Future<List<RuleHit>> listEventRuleHits(String eventId) => api
      .listEventRuleHits(eventId: eventId)
      .then(
        (dtos) => dtos
            .map(
              (dto) => RuleHit(
                id: dto.id,
                ruleId: dto.ruleId,
                ruleContent: dto.ruleContent,
                status: dto.status,
                createdAt: DateTime.parse(dto.createdAt).toLocal(),
                feedback: dto.feedback,
              ),
            )
            .toList(),
      );

  Future<List<RuleHit>> listRecentRuleHits({int limit = 8}) => api
      .listRecentRuleHits(limit: limit)
      .then(
        (dtos) => dtos
            .map(
              (dto) => RuleHit(
                id: dto.id,
                ruleId: dto.ruleId,
                ruleContent: dto.ruleContent,
                status: dto.status,
                createdAt: DateTime.parse(dto.createdAt).toLocal(),
                feedback: dto.feedback,
              ),
            )
            .toList(),
      );

  /// 最近的对话影子校验命中（迁移 54：聊天预筛 → 后台 LLM 判定）。
  Future<List<api.ChatRuleHitDto>> listRecentChatRuleHits({int limit = 30}) =>
      api.listRecentChatRuleHits(limit: limit);

  Future<Conversation> createConversation({String? title, String? tag}) async {
    final dto = await api.createConversation(title: title, tag: tag);
    return _fromConversationDto(dto);
  }

  Future<List<api.PendingActionDto>> listPendingActions(
    String conversationId,
  ) => api.listPendingActions(conversationId: conversationId);

  Future<bool> updatePendingActionArgs(
    String conversationId,
    api.PendingActionDto action,
    String argsJson,
  ) => api.updatePendingActionArgs(
    conversationId: conversationId,
    actionId: action.id,
    argsJson: argsJson,
    expectedVersion: action.version,
    approvalToken: action.approvalToken,
  );

  Future<String> confirmKnowledgeDraft(
    String conversationId,
    api.PendingActionDto action,
  ) => api.confirmKnowledgeDraft(
    conversationId: conversationId,
    actionId: action.id,
    expectedVersion: action.version,
    approvalToken: action.approvalToken,
  );

  Future<void> declineKnowledgeDraft(
    String conversationId,
    api.PendingActionDto action,
  ) => api.declineKnowledgeDraft(
    conversationId: conversationId,
    actionId: action.id,
    expectedVersion: action.version,
    approvalToken: action.approvalToken,
  );

  Future<List<api.ChatProposalDto>> listChatProposals(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) => api.listChatProposals(
    conversationId: conversationId,
    offset: offset,
    limit: limit,
  );

  Future<List<api.ChatDecisionDto>> listChatDecisions(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) => api.listChatDecisions(
    conversationId: conversationId,
    offset: offset,
    limit: limit,
  );

  Future<api.ChatProposalDto> editChatProposal(
    api.ChatReviewRequest request,
    String argsJson,
  ) => api.editChatProposal(request: request, argsJson: argsJson);

  Future<api.ChatDecisionDto> decideChatProposal(
    api.ChatReviewRequest request, {
    required bool accept,
  }) => api.decideChatProposal(request: request, accept: accept);

  Future<List<Message>> getChildMessages(String parentId) async {
    final dtos = await api.getChildMessages(parentId: parentId);
    return dtos
        .map(
          (dto) => Message(
            id: dto.id,
            conversationId: dto.conversationId,
            parentMessageId: dto.parentMessageId,
            role: MessageRole.fromString(dto.role),
            content: dto.content,
            createdAt: DateTime.parse(dto.createdAt).toLocal(),
            kind: dto.kind,
            messageRef: dto.messageRef,
          ),
        )
        .toList();
  }

  Future<List<Message>> getMessageChain(String messageId) async {
    final dtos = await api.getMessageChain(messageId: messageId);
    return dtos
        .map(
          (dto) => Message(
            id: dto.id,
            conversationId: dto.conversationId,
            parentMessageId: dto.parentMessageId,
            role: MessageRole.fromString(dto.role),
            content: dto.content,
            createdAt: DateTime.parse(dto.createdAt).toLocal(),
            kind: dto.kind,
            messageRef: dto.messageRef,
          ),
        )
        .toList();
  }

  Future<Message> submitWikiChatInput(
    String conversationId,
    String content, {
    required String idempotencyKey,
    required String taskJson,
  }) async {
    final dto = await api.submitWikiChatInput(
      conversationId: conversationId,
      content: content,
      idempotencyKey: idempotencyKey,
      taskJson: taskJson,
    );
    return Message(
      id: dto.id,
      conversationId: dto.conversationId,
      parentMessageId: dto.parentMessageId,
      role: MessageRole.fromString(dto.role),
      content: dto.content,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      kind: dto.kind,
      messageRef: dto.messageRef,
    );
  }

  /// Legacy in-memory test repositories can keep their synchronous reply path.
  /// A successfully initialized real bridge always uses durable generation.

  void _watchChatQueue(String conversationId) {
    if (_initialized && !_disposed && runChatQueueWorker) {
      _chatQueueDispatcher.watch(conversationId);
    }
  }

  Future<bool> _pollChatQueue(String conversationId) async {
    if (_disposed) return false;
    final started = await dispatchChatInputQueue(conversationId);
    if (_disposed) return false;
    // A lost dispatch response can still have committed a run. Reconcile its
    // identity without recreating input or automatically retrying generation.
    final latest = started ?? await getLatestChatRun(conversationId);
    if (_disposed) return false;
    if (latest != null && _observedQueueRuns[conversationId] != latest.id) {
      _observedQueueRuns[conversationId] = latest.id;
      chatQueueRunStarted.value = latest;
    }
    final queue = await getChatInputQueue(conversationId);
    if (queue.paused) _observedQueueRuns.remove(conversationId);
    return !queue.paused;
  }

  Future<api.ChatRunDto?> scheduleChatInput(
    String conversationId,
    String inputMessageId,
  ) async {
    try {
      return await api.scheduleChatInput(
        conversationId: conversationId,
        inputMessageId: inputMessageId,
      );
    } finally {
      _watchChatQueue(conversationId);
    }
  }

  Future<List<api.UnscheduledChatInputDto>> listUnscheduledChatInputs(
    String conversationId, {
    int offset = 0,
  }) => api.listUnscheduledChatInputs(
    conversationId: conversationId,
    offset: offset,
    limit: 20,
  );
}
