import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated_io.dart'
    show ExternalLibrary;
import 'package:path_provider/path_provider.dart';

import '../data/storage_repository.dart';
import '../models/event.dart';
import '../models/analysis.dart';
import '../models/conversation.dart';
import '../models/wiki_page.dart';
import '../models/todo.dart';
import '../models/import_fetch.dart';
import '../models/token_usage.dart';
import '../models/tweet_fetch.dart';
import '../models/rule.dart';
import '../models/relation.dart';
import '../models/goal.dart';
import '../sync/sync_setup_controller.dart';
import 'api.dart' as api;
import 'chat_queue_dispatcher.dart';
import 'generated.dart/frb_generated.dart';

/// Rust bridge implementation of storage repository
class RustBridgeRepository implements StorageRepository {
  final String? databasePath;

  /// 是否启动周期性后台 worker（分析排空 + 知识消化）。
  ///
  /// 生产恒为 true。集成测试应传 false：worker 会在测试自己的 FFI 调用之间插入
  /// `triggerAnalysis` / `_drainKnowledgeDigest` 等**写**操作，而每个 API 调用都各自
  /// `Store::open` 一个新连接，于是两者抢同一个 SQLite 文件的锁，随事件循环交错
  /// 时序随机撞出 `SQLITE_BUSY`——表现为「每次失败的用例和调用点都不同」。
  /// 测试要推进队列时显式调 `triggerAnalysis()` 即可，不需要后台线程代替它。
  final bool runBackgroundWorker;
  final bool runChatQueueWorker;
  late final SyncSetupController localSync;
  late final _chatQueueDispatcher = ChatQueueDispatcher(poll: _pollChatQueue);
  final Map<String, String> _observedQueueRuns = {};
  final ValueNotifier<api.ChatRunDto?> chatQueueRunStarted = ValueNotifier(
    null,
  );
  bool _initialized = false;
  bool _analysisWorkerRunning = false;
  bool _analysisWorkerScheduled = false;
  bool _analysisRerunRequested = false;
  bool _disposed = false;
  Timer? _analysisTimer;

  /// 后台知识消化进行中（标题栏 AI 活动指示订阅，见 [knowledgeDigestBusyProvider]）。
  final ValueNotifier<bool> knowledgeDigestBusy = ValueNotifier(false);

  /// 最近一次网络同步 tick 的结果（状态页展示范围/待同步/最近成功/错误）。
  /// `ran == false` 表示本轮按防线跳过（锁定/他 worker 占用/快照导入中），不是错误。
  final ValueNotifier<api.SyncTickResultDto?> lastSyncTick = ValueNotifier(
    null,
  );

  /// 最近一次网络同步 tick 的错误（成功或跳过时清空）。
  final ValueNotifier<String?> lastSyncTickError = ValueNotifier(null);

  /// Changes produced by the background worker invalidate visible knowledge views.
  final ValueNotifier<int> knowledgeRevision = ValueNotifier(0);

  /// 仅在同步 tick 真正拉到对端数据（pulledAccepted>0）时递增。会话/消息
  /// 列表只挂这个信号——分析/消化产出的 bump 与会话视图无关，挂到
  /// knowledgeRevision 上会让聊天页被无关重查反复拆装（列表闪跳、滚动
  /// 位置丢失）。
  final ValueNotifier<int> syncArrivalRevision = ValueNotifier(0);

  /// 打包成 .app（macOS DMG）后，frb 默认加载路径（CWD 相对的 ioDirectory /
  /// elsewhen.framework）在 Finder 启动时找不到 dylib——CWD 是 `/`。
  /// 这里显式从 app 包内 Frameworks 目录加载（打包脚本会把 libelsewhen.dylib
  /// 拷到 Contents/Frameworks/）。开发/测试环境下该文件不存在，返回 null
  /// 走 frb 默认加载链（ioDirectory = ../target/release/）。
  ExternalLibrary? _bundledRustLibrary() {
    // Android：APK 内的 .so 不在文件系统上，dlopen 由系统 linker 完成
    // （libelsewhen.so 打包在 APK lib/<abi>/ 下，见 android/app/build.gradle.kts）。
    if (Platform.isAndroid) {
      return ExternalLibrary.open('libelsewhen.so');
    }
    if (!Platform.isMacOS) return null;
    final dylib =
        '${File(Platform.resolvedExecutable).parent.path}'
        '/../Frameworks/libelsewhen.dylib';
    return File(dylib).existsSync() ? ExternalLibrary.open(dylib) : null;
  }

  RustBridgeRepository({
    this.databasePath,
    this.runBackgroundWorker = true,
    this.runChatQueueWorker = true,
    SyncSetupController? syncSetupController,
  }) {
    localSync =
        syncSetupController ??
        SyncSetupController(autoEnrollment: runBackgroundWorker);
    localSync.onReady = _wakeAnalysisWorker;
  }

  /// 桌面平台让 Rust 侧 `ProjectDirs` 自解析；移动端 `directories` crate
  /// 未实现（ProjectDirs 恒为 None），由 path_provider 取应用私有目录传入。
  Future<String?> _resolveDatabasePath() async {
    if (databasePath != null) return databasePath;
    if (Platform.isAndroid || Platform.isIOS) {
      final dir = await getApplicationSupportDirectory();
      return dir.path;
    }
    return null;
  }

  @override
  Future<void> initialize() async {
    if (_initialized) return;

    await RustLib.init(externalLibrary: _bundledRustLibrary());
    // initBridge 走 Result<String> 契约：失败以异常上抛（P2-9），
    // 取代原先 startsWith('Error ') 的字符串判断。
    try {
      await api.initBridge(databasePath: await _resolveDatabasePath());
    } catch (e) {
      throw StateError('初始化存储失败: $e');
    }
    _initialized = true;
    // Missing/locked native keys pause sync work, not local storage or browsing.
    await localSync.restoreIfConfigured();
    if (!runBackgroundWorker) return;
    // Resume durable work after process restart. The timer also covers retry
    // backoff expiry and the Rust-side 50-job invocation bound.
    _analysisTimer = Timer.periodic(const Duration(seconds: 5), (_) {
      // 空转退避：连续空闲时每 6 拍（≈30s）才做一次完整序列（开库+vault+
      // 同步 tick+洞察 tick），其余拍零 FFI 直接返回。实测（release 空转）
      // 每 5s 整序列贡献 ~5% CPU 毛刺，退避后抹平。显式唤醒（记事件/配
      // key/同步到达/rerun）走 _wakeAnalysisWorker，会清零计数恢复 5s 跟拍。
      // 已知代价（如实记）：重试 backoff 到期后最多等到下一拍完整序列，
      // 即空闲期重试延迟从 ≤5s 变 ≤30s（重回活跃后立即恢复）。
      _idleWakes++;
      if (_idleWakes % _idleWakeFullEvery != 0) return;
      _wakeAnalysisWorker();
    });
    _wakeAnalysisWorker();
  }

  @override
  Future<Event> recordEvent(String rawText) async {
    final dto = await api.recordEvent(rawText: rawText);
    _wakeAnalysisWorker();
    return Event(
      id: dto.id,
      rawText: dto.rawText,
      recordedAt: DateTime.parse(dto.recordedAt).toLocal(),
      source: dto.source,
      status: dto.status,
    );
  }

  Future<api.InputRecordDto> submitInput(
    String rawText, {
    String source = 'main_input',
    String? idempotencyKey,
  }) async {
    final input = await api.submitInput(
      rawText: rawText,
      source: source,
      idempotencyKey: idempotencyKey,
    );
    _wakeAnalysisWorker();
    return input;
  }

  Future<api.InputRecordDto> beginUrlInput(
    String url, {
    String? idempotencyKey,
  }) => api.beginUrlInput(rawText: url, idempotencyKey: idempotencyKey);

  Future<api.InputRecordDto> finishUrlInput(
    String inputId, {
    String? wikiPageSlug,
    bool failed = false,
  }) => api.finishUrlInput(
    inputId: inputId,
    wikiPageSlug: wikiPageSlug,
    failed: failed,
  );

  @override
  Future<void> recordUnifiedInput(
    String rawText, {
    String source = 'capture',
  }) async {
    await submitInput(rawText, source: source);
  }

  Future<Message> submitConversationInput(
    String conversationId,
    String content, {
    String? idempotencyKey,
  }) async {
    final input = await api.submitConversationInput(
      conversationId: conversationId,
      rawText: content,
      idempotencyKey: idempotencyKey,
    );
    final messageId = input.messageId;
    if (messageId == null) {
      throw StateError('统一输入没有关联 message');
    }
    _wakeAnalysisWorker();
    final messages = await listMessages(conversationId);
    return messages.firstWhere((message) => message.id == messageId);
  }

  Future<List<api.DailyEntryDto>> listDailyEntries(String date) =>
      api.listDailyEntries(date: date);

  Future<api.DailyOverviewDto> getDailyOverview(String date) =>
      api.getDailyOverview(date: date);

  /// 回顾页周/月拼装统计（R1：本地实时聚合，无 AI、无持久化）。
  Future<api.PeriodStatsDto> getPeriodStats(String periodType, String date) =>
      api.getPeriodStats(periodType: periodType, date: date);

  /// 周期盘点报告读面（R4）：最新在前的报告列表（含未读/偏差条目）。
  Future<List<api.DeviationReportDto>> listDeviationReports({
    required String periodType,
    int limit = 12,
  }) => api.listDeviationReports(periodType: periodType, limit: limit);

  /// 标记报告已读（幂等；首次的 read\_at 保留）。
  Future<void> markDeviationReportRead(String id) =>
      api.markDeviationReportRead(id: id);

  /// 忽略单条偏差（置位保留记录；报告兜底置已读）。
  Future<void> dismissDeviation(String reportId, int index) =>
      api.dismissDeviation(reportId: reportId, index: index);

  Future<api.DailyReviewResult> generateDailyReview(String date) =>
      api.generateDailyReview(date: date);

  Future<List<api.EntityFactDto>> listEntityFacts(
    String entityKind,
    String entitySlug,
  ) => api.listEntityFacts(entityKind: entityKind, entitySlug: entitySlug);

  Future<bool> deleteEntityFact(String id) => api.deleteEntityFact(id: id);

  Future<List<String>> listEntityAliases(String kind, String slug) =>
      api.listEntityAliases(entityKind: kind, entitySlug: slug);

  Future<void> addEntityAlias(String kind, String slug, String alias) =>
      api.addEntityAlias(entityKind: kind, entitySlug: slug, alias: alias);

  Future<bool> mergeEntity(String kind, String sourceSlug, String targetSlug) =>
      api.mergeEntity(
        entityKind: kind,
        sourceSlug: sourceSlug,
        targetSlug: targetSlug,
      );

  Future<api.EntityMergeStatusDto?> getEntityMergeStatus(String sourceSlug) =>
      api.getEntityMergeStatus(sourceSlug: sourceSlug);

  Future<bool> undoEntityMerge(String sourceSlug) =>
      api.undoEntityMerge(sourceSlug: sourceSlug);

  Future<api.EventAnalysisDetailDto?> getEventAnalysisDetail(String eventId) =>
      api.getEventAnalysisDetail(eventId: eventId);

  Future<api.MessageRecordabilityDto?> getMessageRecordability(
    String messageId,
  ) => api.getMessageRecordability(messageId: messageId);

  Future<api.MessageRecordabilityDto> setEventRecordability(
    String eventId,
    bool recordable,
  ) => api.setEventRecordability(eventId: eventId, recordable: recordable);

  Future<bool> reanalyzeEvent(String eventId) async {
    final queued = await api.reanalyzeEvent(eventId: eventId);
    if (queued) _wakeAnalysisWorker();
    return queued;
  }

  @override
  Future<List<Event>> listEvents() async {
    final dtos = await api.listEvents();
    return dtos
        .map(
          (dto) => Event(
            id: dto.id,
            rawText: dto.rawText,
            recordedAt: DateTime.parse(dto.recordedAt).toLocal(),
            source: dto.source,
            status: dto.status,
          ),
        )
        .toList();
  }

  @override
  Future<List<Analysis>> listAnalyses() async {
    final dtos = await api.listAnalyses();
    return dtos
        .map(
          (dto) => Analysis(
            eventType: dto.eventType,
            confidence: dto.confidence,
            summary: dto.summary,
            clarifications: dto.clarifications,
          ),
        )
        .toList();
  }

  Future<api.AnalysisJobStatsDto> getAnalysisJobStats() =>
      api.getAnalysisJobStats();

  Future<api.KnowledgeDigestStatsDto> getKnowledgeDigestStats() =>
      api.getKnowledgeDigestStats();

  /// 知识消化队列明细（只读）。[status] 为 null 时返回全部状态。
  Future<List<api.KnowledgeDigestJobDto>> listKnowledgeDigestJobs({
    String? status,
    int limit = 200,
  }) => api.listKnowledgeDigestJobs(status: status, limit: limit);

  /// 知识消化运行日志（只读），按开始时间倒序。
  Future<List<api.KnowledgeDigestRunDto>> listKnowledgeDigestRuns({
    int limit = 100,
  }) => api.listKnowledgeDigestRuns(limit: limit);

  @override
  Future<String?> getAiProvider() async {
    return await api.getAiProvider();
  }

  @override
  Future<api.AnalysisTriggerResult> triggerAnalysis() async {
    return await api.triggerAnalysis();
  }

  /// Schedule analysis without making input persistence wait for the network.
  /// Multiple saves coalesce into one in-flight invocation.
  /// 空转退避参数：每 _idleWakeFullEvery 拍（每拍 5s，6 拍≈30s）做一次完整
  /// 后台序列（分析排空+知识消化+网络同步+洞察 tick）。序列本身每次都是全量的
  /// （开 store、解密 vault、roster 校验），空闲期白烧——这是唯一的省法。
  static const int _idleWakeFullEvery = 6;
  int _idleWakes = 0;

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
          // Keep draining bounded Rust batches while pending work is
          // immediately available. Retry jobs are revisited by the timer once
          // available_at expires.
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
        } while (_analysisRerunRequested && !_disposed);
        // 分析排空后再做知识消化：同一 worker 串行，避免与分析并发调用 provider；
        // 消化依赖分析结果（可记录性），先分析后消化也最省重试。
        await _drainKnowledgeDigest();
        // 周期报告 + 偏差检测（R3）：同一 worker 串行（Q4：同一 timer）；
        // 内部自带幂等/空窗跳过/检测退避，仅在有事发生时记日志
        //（该 tick 在空闲期每 ≈30s 一次，逐拍打印会刷屏）。
        try {
          final reportTick = await api.tickPeriodicReports();
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
          knowledgeDigestBusy.value = true;
          final changed = await api.tickKnowledgeInsights();
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
        knowledgeDigestBusy.value = false;
        _analysisWorkerRunning = false;
        if (_analysisRerunRequested) _wakeAnalysisWorker();
      }
    });
  }

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
    while (!_disposed && !_analysisRerunRequested) {
      knowledgeDigestBusy.value = true;
      final result = await api.triggerKnowledgeDigest();
      knowledgeDigestBusy.value = false;
      // Idle / NoProvider / Failed 都停：失败已退避回队，由 timer 在到期后重试。
      if (result is! api.KnowledgeDigestTickResult_Processed) break;
      if (!_disposed) knowledgeRevision.value++;
    }
  }

  /// Stop lifecycle polling. Production keeps the repository for the process
  /// lifetime; isolated tests call this before deleting their temporary data.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _analysisTimer?.cancel();
    _analysisTimer = null;
    _chatQueueDispatcher.dispose();
    _observedQueueRuns.clear();
    localSync.dispose();
  }

  // Conversation methods

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

  Future<Conversation> createConversation({String? title, String? tag}) async {
    final dto = await api.createConversation(title: title, tag: tag);
    return _fromConversationDto(dto);
  }

  Future<List<Message>> listMessages(String conversationId) async {
    final dtos = await api.listMessages(conversationId: conversationId);
    return dtos
        .map(
          (dto) => Message(
            id: dto.id,
            conversationId: dto.conversationId,
            parentMessageId: dto.parentMessageId,
            role: MessageRole.fromString(dto.role),
            content: dto.content,
            createdAt: DateTime.parse(dto.createdAt).toLocal(),
            inputMessageId: dto.inputMessageId,
          ),
        )
        .toList();
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
          ),
        )
        .toList();
  }

  bool get supportsPageTasks => _initialized;

  Future<api.WikiChatMaterialDto> getWikiChatMaterial(String slug) =>
      api.getWikiChatMaterial(pageSlug: slug);

  Future<String?> getWikiChatTask(String conversationId) =>
      api.getWikiChatTask(conversationId: conversationId);

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
    );
  }

  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) async {
    final dto = await api.sendMessage(
      conversationId: conversationId,
      role: role,
      content: content,
      parentMessageId: parentMessageId,
    );
    return Message(
      id: dto.id,
      conversationId: dto.conversationId,
      parentMessageId: dto.parentMessageId,
      role: MessageRole.fromString(dto.role),
      content: dto.content,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
    );
  }

  /// Legacy in-memory test repositories can keep their synchronous reply path.
  /// A successfully initialized real bridge always uses durable generation.
  bool get supportsChatRuns => _initialized;
  bool get supportsChatQueue => _initialized;

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

  Future<List<api.ChatRunDto>> listChatRuns(
    String conversationId, {
    int offset = 0,
  }) => api.listChatRuns(
    conversationId: conversationId,
    offset: offset,
    limit: 20,
  );

  Future<int> enqueueChatInput(
    String conversationId,
    String inputMessageId,
  ) async {
    try {
      return await api.enqueueChatInput(
        conversationId: conversationId,
        inputMessageId: inputMessageId,
      );
    } finally {
      _watchChatQueue(conversationId);
    }
  }

  Future<int> enqueueChatSupplement(
    String conversationId,
    String inputMessageId,
    String runId, {
    String contextJson = '{}',
  }) => api.enqueueChatSupplement(
    conversationId: conversationId,
    inputMessageId: inputMessageId,
    runId: runId,
    contextJson: contextJson,
  );
  Future<api.ChatInputQueueDto> getChatInputQueue(
    String conversationId, {
    int afterId = 0,
  }) => api.getChatInputQueue(
    conversationId: conversationId,
    afterId: afterId,
    limit: 100,
  );
  Future<bool> withdrawChatInput(String conversationId, int queueId) =>
      api.withdrawChatInput(conversationId: conversationId, queueId: queueId);
  Future<void> resumeChatInputQueue(String conversationId) async {
    try {
      await api.resumeChatInputQueue(conversationId: conversationId);
    } finally {
      _watchChatQueue(conversationId);
    }
  }

  Future<void> pauseChatInputQueue(String conversationId) async {
    await api.pauseChatInputQueue(conversationId: conversationId);
    _chatQueueDispatcher.forget(conversationId);
    _observedQueueRuns.remove(conversationId);
  }

  Future<api.ChatRunDto?> dispatchChatInputQueue(String conversationId) =>
      api.dispatchChatInputQueue(conversationId: conversationId);

  Future<api.ChatRunDto> startChatRun(
    String conversationId,
    String inputMessageId,
    String idempotencyKey,
  ) => api.startChatRun(
    conversationId: conversationId,
    inputMessageId: inputMessageId,
    idempotencyKey: idempotencyKey,
  );

  Future<api.ChatRunDto?> getChatRun(String conversationId, String runId) =>
      api.getChatRun(conversationId: conversationId, runId: runId);

  Future<api.ChatRunDto?> getLatestChatRun(String conversationId) =>
      api.getLatestChatRun(conversationId: conversationId);

  Future<api.ChatRunEventPageDto> listChatRunEvents(
    String conversationId,
    String runId,
    int afterSequence, {
    int limit = 64,
  }) => api.listChatRunEvents(
    conversationId: conversationId,
    runId: runId,
    afterSequence: afterSequence,
    limit: limit,
  );

  /// Call only from an explicit application quit, never chat-view disposal.
  Future<int> shutdownChatRuns() => api.shutdownChatRuns();

  Future<api.ChatRunDto?> cancelChatRun(String conversationId, String runId) =>
      api.cancelChatRun(conversationId: conversationId, runId: runId);

  Future<String> generateReply(
    String conversationId, {
    String? providerType,
    String? memoryType,
    int? memoryWindowSize,
  }) async {
    return await api.generateReply(
      conversationId: conversationId,
      providerType: providerType,
      memoryType: memoryType,
      memoryWindowSize: memoryWindowSize,
    );
  }

  /// 最近 N 天的每日 token 用量统计（含当天，日期倒序）
  Future<List<DailyTokenUsage>> getDailyTokenUsage(int days) async {
    final dtos = await api.getDailyTokenUsage(days: days);
    return dtos.map(DailyTokenUsage.fromDto).toList();
  }

  // Wiki methods

  /// 列出知识库页面（主列表，不含派生产物）。
  /// kind/area 为空时列出全部；area 取值：imported（素材库）/ network（人物项目）/ insight（知识沉淀）。
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async {
    final dtos = await api.listWikiPages(kind: kind, area: area);
    return dtos.map(WikiPage.fromDto).toList();
  }

  /// 页面列表元数据（sidebar 专用）：无正文、无图片解析、SQL 级有界
  /// （默认 500 条，Rust 侧按 updated_at 倒序）。明细按 slug 单页读。
  Future<List<WikiPageMeta>> listWikiPageMetas({int? limit}) async {
    final dtos = await api.listWikiPageMetas(limit: limit);
    return dtos.map(WikiPageMeta.fromDto).toList();
  }

  /// 某页的派生产物列表（AI 加工成果：总结/提炼/文案…）
  Future<List<WikiPage>> listWikiPageDerivatives(String slug) async {
    final dtos = await api.listWikiPageDerivatives(slug: slug);
    return dtos.map(WikiPage.fromDto).toList();
  }

  /// 直接把一段内容保存为某知识页的派生产物（页内 AI 聊天「保存」按钮用，跳过 AI 草拟确认）。
  Future<WikiPage> createWikiDerivative({
    required String basedOnSlug,
    required String contentType,
    required String title,
    required String contentMd,
  }) async {
    final dto = await api.createWikiDerivative(
      basedOnSlug: basedOnSlug,
      contentType: contentType,
      title: title,
      contentMd: contentMd,
    );
    return WikiPage.fromDto(dto);
  }

  /// 按 slug 获取单个知识库页面
  Future<WikiPage?> getWikiPage(String slug) async {
    final dto = await api.getWikiPage(slug: slug);
    return dto == null ? null : WikiPage.fromDto(dto);
  }

  /// 更新知识页标签（应用内整理元数据；传空列表即清空）。返回更新后的页面。
  Future<WikiPage> updateWikiTags({
    required String slug,
    required List<String> tags,
  }) async {
    final dto = await api.updateWikiTags(slug: slug, tags: tags);
    return WikiPage.fromDto(dto);
  }

  /// 数据库体量统计（含 freelist 空闲占比，供维护面板展示）。
  Future<api.DatabaseStatsDto> databaseStats() => api.databaseStats();

  /// 用户技能（skills 目录 + SKILL.md frontmatter）。
  Future<List<api.UserSkillDto>> listUserSkills() => api.listUserSkills();

  /// MCP 服务器管理（用户自定义外部工具源，迁移 44）。
  Future<List<api.McpServerDto>> listMcpServers() => api.listMcpServers();

  Future<String> saveMcpServer(api.McpServerDto server) =>
      api.saveMcpServer(server: server);

  Future<void> deleteMcpServer({required String id}) =>
      api.deleteMcpServer(id: id);

  Future<void> setMcpServerEnabled({required String id, required bool enabled}) =>
      api.setMcpServerEnabled(id: id, enabled: enabled);

  /// 连通性测试：拉起进程走握手并取回工具清单。
  Future<List<api.McpToolDto>> testMcpServer(api.McpServerDto server) =>
      api.testMcpServer(server: server);

  /// 用户技能启停（写 skills_state.json，不影响技能文件）。
  Future<void> setUserSkillEnabled({required String id, required bool enabled}) =>
      api.setUserSkillEnabled(id: id, enabled: enabled);

  /// 技能目录绝对路径（供"打开技能目录"）。
  Future<String> userSkillsDirectoryPath() => api.userSkillsDirectoryPath();

  /// 安装：本地目录（含仓库外壳自动下钻）。
  Future<api.UserSkillDto> installUserSkillFromDirectory(String source) =>
      api.installSkillFromDirectory(source: source);

  /// 安装：zip（内含 zip-slip 防御）。
  Future<api.UserSkillDto> installUserSkillFromZip(String zipPath) =>
      api.installSkillFromZip(zipPath: zipPath);

  /// 安装：git 仓库（depth 1，https 或本地路径）。
  Future<api.UserSkillDto> installUserSkillFromGit(String url) =>
      api.installSkillFromGit(url: url);

  /// 卸载：删目录 + 记删除墓碑（其他设备同步后一并删除）。
  /// 删除的唯一合法入口——手动 rm 目录会被分发复活。
  Future<void> uninstallUserSkill(String id) => api.uninstallUserSkill(id: id);

  /// VACUUM 整理（独占写，被并发阻塞时抛错由前端提示重试）。
  Future<api.VacuumResultDto> vacuumDatabase() => api.vacuumDatabase();

  /// 删除知识页（同步库先捕获 deleted 变更再物理删除；不存在返回 false）。
  Future<bool> deleteWikiPage({required String slug}) =>
      api.deleteWikiPage(slug: slug);

  /// 导出知识页为 Markdown（用户选定目录；与主列表同口径，同名覆盖可重复）。
  Future<api.WikiExportResultDto> exportWikiMarkdown({required String outDir}) =>
      api.exportWikiMarkdown(outDir: outDir);

  /// 按项目页记录的本地目录重新扫描，并更新原知识页。
  Future<WikiPage> refreshProjectPage(String slug) async {
    final dto = await api.refreshProjectPage(slug: slug);
    return WikiPage.fromDto(dto);
  }

  /// 修改一张项目页关联的本地目录（目录搬家后纠正路径；新路径须存在且是目录）。
  Future<WikiPage> updateProjectPath({
    required String slug,
    required String newPath,
  }) async {
    final dto = await api.updateProjectPath(slug: slug, newPath: newPath);
    return WikiPage.fromDto(dto);
  }

  /// 人类编辑保存一页正文（M3 §6.2；限可编辑 kind，素材页只读拒绝）。
  /// 保存后 `human_edited_at` 置位：该页被 AI digest 视为人工持有，不再整篇覆盖。
  /// [expectedUpdatedAt]：乐观锁（§11 Q3）——传加载时的 updatedAt（rfc3339），
  /// 编辑期间页面被后台更新则报「编辑冲突」；null 跳过校验（强制覆盖路径）。
  Future<WikiPage> saveWikiPageContent({
    required String slug,
    required String contentMd,
    String reason = 'GUI 编辑',
    String? expectedUpdatedAt,
  }) async {
    final dto = await api.saveWikiPageContent(
      slug: slug,
      contentMd: contentMd,
      reason: reason,
      expectedUpdatedAt: expectedUpdatedAt,
    );
    if (!_disposed) knowledgeRevision.value++;
    return WikiPage.fromDto(dto);
  }

  /// 知识库页聚合视图（D4）：哪些事件的分析 guidance 引用了此页。
  Future<List<api.PageGuidanceDto>> listPageGuidance(String slug) =>
      api.listPageGuidance(slug: slug);

  /// guidance 采纳反馈（遗留收口切片 B）：只记不外推；feedback null = 清
  /// 除（toggle off）。与 rule_hit feedback 同纪律，未知值 fail-closed。
  Future<bool> setGuidanceFeedback({
    required String analysisId,
    required String slug,
    String? note,
    String? feedback,
  }) => api.setGuidanceFeedback(
    analysisId: analysisId,
    slug: slug,
    note: note,
    feedback: feedback,
  );

  /// 情绪标注列表（D5 用户模型面板）：分析产物派生，文本无数值。
  Future<List<api.EventEmotionDto>> listEventEmotions() =>
      api.listEventEmotions();

  /// 删除一条情绪标注：只编辑分析产物 emotion 字段，事件真源不动。
  Future<void> deleteEventEmotion({
    required String id,
    String? target,
    required String feeling,
  }) => api.deleteEventEmotion(id: id, target: target, feeling: feeling);

  // ── 人物关系 ──

  /// 与某页相关的人物关系（双向：作为人物方或作为事情/项目方）
  Future<List<Relation>> listRelationsForPage(String slug) async {
    final dtos = await api.listRelationsForPage(slug: slug);
    return dtos.map(Relation.fromDto).toList();
  }

  /// 全部人物关系（备用：未来人物视图）
  Future<List<Relation>> listRelations() async {
    final dtos = await api.listRelations();
    return dtos.map(Relation.fromDto).toList();
  }

  /// 手动添加/刷新一条人物关系（两侧页面必须已存在）。返回落库后的关系。
  Future<Relation> addRelation({
    required String fromSlug,
    required String toSlug,
    required String relation,
    String? note,
  }) async {
    final dto = await api.addRelation(
      fromSlug: fromSlug,
      toSlug: toSlug,
      relation: relation,
      note: note,
    );
    return Relation.fromDto(dto);
  }

  /// 删除一条人物关系（修正误识别时用）。返回是否真的删掉了。
  Future<bool> deleteRelation(String id) => api.deleteRelation(id: id);

  /// 从 x.com / twitter.com 推文链接抓取长文（解析 json，不写库）
  Future<TweetFetch> fetchTweet(String url) async {
    final dto = await api.fetchTweet(url: url);
    return TweetFetch.fromDto(dto);
  }

  /// URL 查重：知识库里是否已保存过该推文（slug = tweet-{id}）。
  /// 已存在则返回已保存页面（UI 直接打开、不再抓取）；否则 null。
  Future<WikiPage?> findTweetSourcePage(String url) async {
    final dto = await api.findTweetSourcePage(url: url);
    return dto == null ? null : WikiPage.fromDto(dto);
  }

  /// 把已抓取的推文内容保存为知识库页面（kind=source）。只有点「保存」才入库。
  Future<WikiPage> saveTweetPage({
    required String tweetId,
    required String text,
    String? title,
    String? authorName,
    String? screenName,
  }) async {
    final dto = await api.saveTweetPage(
      tweetId: tweetId,
      text: text,
      title: title,
      authorName: authorName,
      screenName: screenName,
    );
    return WikiPage.fromDto(dto);
  }

  /// 把用户粘贴的纯文本保存为知识库页面（kind=topic）。
  /// 文本会完整写入（content_md 不截断），标题/摘要自动生成；tags 可选。
  Future<WikiPage> saveTextPage({
    required String text,
    String? title,
    List<String> tags = const [],
  }) async {
    final dto = await api.saveTextPage(text: text, title: title, tags: tags);
    return WikiPage.fromDto(dto);
  }

  /// 把一段文本保存为「主题」（kind=topic）页——对话消息「存为主题」入口专用。
  /// 文本完整写入，标题留空时取正文首行前 60 字；tags 可选。
  Future<WikiPage> saveTopicPage({
    required String text,
    String? title,
    List<String> tags = const [],
  }) async {
    final dto = await api.saveTopicPage(text: text, title: title, tags: tags);
    return WikiPage.fromDto(dto);
  }

  /// 判断一个网址需要走哪种抓取方式（"tweet" → fxtwitter 专用 API，"web" → 普通网页提取）。
  /// 统一由 Rust 侧分类，避免前端各自猜测。
  Future<String> guessImportKind(String url) => api.guessImportKind(url: url);

  /// 针对一段抓取内容做一次性对话回复（不写库，供保存前与 AI 讨论内容）
  Future<String> generateContentChat({
    required String content,
    required List<ContentChatMessage> messages,
  }) {
    return api.generateContentChat(
      content: content,
      messages: messages.map((m) => m.toDto).toList(),
    );
  }

  /// 当前推文抓取服务（设置页读取；当前仅支持 fxtwitter）
  Future<String> getTweetFetchService() => api.getTweetFetchService();

  /// 更新推文抓取服务（设置页保存）
  Future<void> updateTweetFetchService(String service) =>
      api.updateTweetFetchService(service: service);

  /// 主题偏好（模式 + 预设 + 字体 + 正文字号 + 编辑器覆盖层，存 app_meta）
  Future<api.ThemePrefsDto> getThemePrefs() => api.getThemePrefs();

  Future<void> updateThemePrefs({
    required String mode,
    required String preset,
    String font = 'inter',
    double fontSize = 16.0,
    // 编辑器（内容区）覆盖层：null = 跟随全局（Rust 侧会清掉对应 meta 键）
    String? editorFont,
    double? editorFontSize,
    double? editorLineHeight,
  }) => api.updateThemePrefs(
    mode: mode,
    preset: preset,
    font: font,
    fontSize: fontSize,
    editorFont: editorFont,
    editorFontSize: editorFontSize,
    editorLineHeight: editorLineHeight,
  );

  /// AI provider 配置（设置页预填/保存用，直连 Rust DB 的 ai_provider_configs）
  Future<api.AiProviderConfigDto?> getAiProviderConfig() =>
      api.getAiProviderConfig();

  Future<void> updateAiProviderConfig({
    required String baseUrl,
    required String model,
    required String apiKey,
  }) => api.updateAiProviderConfig(
    baseUrl: baseUrl,
    model: model,
    apiKey: apiKey,
  );

  // ---- 多配置 + 单激活 ----

  /// 全部 AI provider 配置（含激活标记）
  Future<List<api.AiProviderConfigDto>> listAiProviderConfigs() =>
      api.listAiProviderConfigs();

  /// 新增 / 编辑配置；返回配置 id
  Future<String> saveAiProviderConfig(api.AiProviderConfigDto provider) =>
      api.saveAiProviderConfig(provider: provider);

  /// 设为激活（唯一激活项）
  Future<void> setActiveAiProviderConfig(String id, {String? revisionToken}) =>
      api.setActiveAiProviderConfig(id: id, revisionToken: revisionToken);

  /// 删除配置（删除激活项时剩余第一个自动接管激活）
  Future<void> deleteAiProviderConfig(String id, {String? revisionToken}) =>
      api.deleteAiProviderConfig(id: id, revisionToken: revisionToken);

  // ── 个人待办 ──

  /// 列出待办；status 为 null 时返回 open+done
  Future<List<Todo>> listTodos({String? status}) async {
    final dtos = await api.listTodos(status: status);
    return dtos.map(Todo.fromDto).toList();
  }

  /// 新建待办（手动创建，直接生效）
  Future<Todo> createTodo({
    required String title,
    String? dueAt,
    String? priority,
    String? relatedWikiSlug,
    String? note,
  }) async {
    final dto = await api.createTodo(
      title: title,
      dueAt: dueAt,
      priority: priority,
      relatedWikiSlug: relatedWikiSlug,
      note: note,
    );
    return Todo.fromDto(dto);
  }

  Future<void> updateTodoStatus(String id, String status) =>
      api.updateTodoStatus(id: id, status: status);

  /// 编辑待办（标题必填；note/dueAt 传 null 表示清除，priority 传 null 回落为 normal）
  Future<void> updateTodo({
    required String id,
    required String title,
    String? note,
    String? priority,
    String? dueAt,
  }) => api.updateTodo(
    id: id,
    title: title,
    note: note,
    priority: priority,
    dueAt: dueAt,
  );

  Future<bool> deleteTodo(String id) => api.deleteTodo(id: id);

  // ── 目标（FR-PES-005-01）──

  /// 列出全部活跃目标（最多 3 条）
  Future<List<Goal>> listActiveGoals() async {
    final dtos = await api.listActiveGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 列出已归档目标（历史）
  Future<List<Goal>> listArchivedGoals() async {
    final dtos = await api.listArchivedGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 新建目标。活跃目标已满 3 条时后端拒绝，错误信息为可展示的中文。
  Future<Goal> createGoal({
    required String content,
    required GoalPhase phase,
  }) async {
    final dto = await api.createGoal(content: content, phase: phase.wire);
    return Goal.fromDto(dto);
  }

  /// 编辑目标正文与阶段（状态不变，故不受上限限制）
  Future<void> updateGoal({
    required String id,
    required String content,
    required GoalPhase phase,
  }) => api.updateGoal(id: id, content: content, phase: phase.wire);

  /// 归档目标，让出活跃名额但保留为历史
  Future<void> archiveGoal(String id) => api.archiveGoal(id: id);

  /// 复活已归档目标。活跃目标已满 3 条时后端拒绝。
  Future<void> reactivateGoal(String id) => api.reactivateGoal(id: id);

  Future<WikiPage> openTodoWorkItem(String id) async {
    final dto = await api.openTodoWorkItem(id: id);
    return WikiPage.fromDto(dto);
  }

  // ── 任意 URL 导入 ──

  /// 抓取任意 URL 内容（推文走 fxtwitter，普通页面走 HTML 文本提取），只解析不入库
  Future<ImportFetch> fetchImportUrl(String url) async {
    final dto = await api.fetchImportUrl(url: url);
    return ImportFetch.fromDto(dto);
  }

  /// 把抓取到的 URL 内容保存为知识库页面（kind=source，带 source_url）
  Future<WikiPage> saveImportedPage({
    required String title,
    required String contentMd,
    required String sourceUrl,
    required String sourceKind,
    required List<String> tags,
  }) async {
    final dto = await api.saveImportedPage(
      title: title,
      contentMd: contentMd,
      sourceUrl: sourceUrl,
      sourceKind: sourceKind,
      tags: tags,
    );
    return WikiPage.fromDto(dto);
  }

  // ── 知识页内 AI 处理会话 ──

  /// 获取（不存在则创建）知识页处理会话；页内 AI 聊天走该会话
  Future<Conversation> ensureWikiPageChat(String pageSlug) async {
    final dto = await api.ensureWikiPageChat(pageSlug: pageSlug);
    return _fromConversationDto(dto);
  }

  /// 归档知识页处理会话（如页面删除时）
  Future<void> archiveWikiPageChat(String pageSlug) =>
      api.archiveWikiPageChat(pageSlug: pageSlug);
}

/// Storage repository provider
final storageRepositoryProvider = Provider<StorageRepository>((ref) {
  return RustBridgeRepository();
});

/// 后台知识消化是否进行中（标题栏 AI 活动指示）。非桥接仓库（mock）恒为 false。
final knowledgeDigestBusyProvider = Provider<bool>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return false;
  final busy = repo.knowledgeDigestBusy;
  void onChange() => ref.invalidateSelf();
  busy.addListener(onChange);
  ref.onDispose(() => busy.removeListener(onChange));
  return busy.value;
});

final knowledgeRevisionProvider = Provider<int>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return 0;
  final revision = repo.knowledgeRevision;
  void onChange() => ref.invalidateSelf();
  revision.addListener(onChange);
  ref.onDispose(() => revision.removeListener(onChange));
  return revision.value;
});

/// 同步到达修订号：仅对端数据真正拉到时递增（见 syncArrivalRevision）。
/// 会话列表/消息列表订阅它，而不是知识修订号。
final syncArrivalRevisionProvider = Provider<int>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return 0;
  final revision = repo.syncArrivalRevision;
  void onChange() => ref.invalidateSelf();
  revision.addListener(onChange);
  ref.onDispose(() => revision.removeListener(onChange));
  return revision.value;
});
