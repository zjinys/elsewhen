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
import '../sync/background_failure_policy.dart';
import '../sync/sync_setup_controller.dart';
import 'ai_activity.dart';
import 'api.dart' as api;
import 'chat_queue_dispatcher.dart';
import 'generated.dart/frb_generated.dart';

// ── 仓储无多态需求的便捷薄委托按功能域拆为 part extension（被测试 Fake 覆写
//  及其调用链的方法保留在类内保证虚分派语义，行为零变化）──
part 'rust_bridge_parts/worker_reminders.dart';
part 'rust_bridge_parts/conversations.dart';
part 'rust_bridge_parts/chat_and_misc.dart';

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
  bool _housekeepingRunning = false;
  Timer? _analysisTimer;

  /// 后台 worker 的失败分类与退避（终态不重试，见 [BackgroundFailurePolicy]）。
  final BackgroundFailurePolicy _failurePolicy = BackgroundFailurePolicy();

  /// 后台 AI 活动（标题栏指示器订阅，见 [backgroundAiActivityProvider]）。
  final ValueNotifier<AiActivity?> backgroundAiActivity = ValueNotifier(null);

  /// 后台活动栈：分析、消化、报告、整理可能前后/嵌套发生，用栈保证
  /// 内层结束时恢复到外层文案，而不是直接清空。
  final List<AiActivity> _backgroundAiActivityStack = [];

  /// 最近一次网络同步 tick 的结果（状态页展示范围/待同步/最近成功/错误）。
  /// `ran == false` 表示本轮按防线跳过（锁定/他 worker 占用/快照导入中），不是错误。
  final ValueNotifier<api.SyncTickResultDto?> lastSyncTick = ValueNotifier(
    null,
  );

  /// 最近一次网络同步 tick 的错误（成功或跳过时清空）。
  final ValueNotifier<String?> lastSyncTickError = ValueNotifier(null);

  /// 每个对端的投递状态（R6-1/R6-2 的 `deliver` 状态机视图）。由 Rust 从
  /// outbox 现有列**推导**得出，Dart 侧不再自造一份进度：状态只有一个真相，
  /// 面板与设置页都读它。每次 tick 后刷新；出错时保留上一次的值而不是清空
  /// ——「刚才还好、现在空」会让用户以为同步被重置了。
  final ValueNotifier<List<api.SyncDeliveryStatusDto>> syncDeliveryStatus =
      ValueNotifier(const []);

  /// Changes produced by the background worker invalidate visible knowledge views.
  final ValueNotifier<int> knowledgeRevision = ValueNotifier(0);

  /// 仅在同步 tick 真正拉到对端数据（pulledAccepted>0）时递增。会话/消息
  /// 列表只挂这个信号——分析/消化产出的 bump 与会话视图无关，挂到
  /// knowledgeRevision 上会让聊天页被无关重查反复拆装（列表闪跳、滚动
  /// 位置丢失）。
  final ValueNotifier<int> syncArrivalRevision = ValueNotifier(0);

  /// 习惯问候插入/回答后递增，驱动主对话消息重查。
  final ValueNotifier<int> habitNudgeRevision = ValueNotifier(0);

  /// 未处理习惯问候数（侧边栏「对话」未读点）。
  final ValueNotifier<int> pendingHabitNudges = ValueNotifier(0);

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
    localSync.onReady = () {
      // 用户/自愈刚把本机同步身份准备好（含解锁）：解除终态退避，立刻跑一拍。
      _failurePolicy.reset();
      _wakeAnalysisWorker();
    };
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
    _analysisTimer = Timer.periodic(const Duration(seconds: 5), (_) async {
      // 提醒/习惯问候必须每 5s 真跑，不能跟空闲退避一起降到 30s；它们只做
      // 本地 SQL 判断，成本低。失败各自隔离，不影响后面的后台序列。
      if (!_disposed && !_housekeepingRunning) {
        _housekeepingRunning = true;
        try {
          await _tickDueReminders();
          await _tickHabitNudges();
        } finally {
          _housekeepingRunning = false;
        }
      }
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
    await _tickDueReminders();
    await _tickHabitNudges();
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

  /// 到期提醒广播（UI 订阅弹出）。只进不出：tick 只负责发现+记录 fire+广播，
  /// 确认/推迟由 UI 动作走独立 API（见 confirmReminder/snoozeReminder）。
  final ValueNotifier<List<api.DueReminderDto>> dueReminders =
      ValueNotifier<List<api.DueReminderDto>>(const []);

  static const String _reminderDeviceId = 'local';

  bool get supportsPageTasks => _initialized;
  bool get supportsChatRuns => _initialized;
  bool get supportsChatQueue => _initialized;

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
            kind: dto.kind,
            messageRef: dto.messageRef,
          ),
        )
        .toList();
  }

  Future<api.WikiChatMaterialDto> getWikiChatMaterial(String slug) =>
      api.getWikiChatMaterial(pageSlug: slug);

  Future<String?> getWikiChatTask(String conversationId) =>
      api.getWikiChatTask(conversationId: conversationId);

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
      kind: dto.kind,
      messageRef: dto.messageRef,
    );
  }

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

  /// 列出知识库页面（主列表，不含派生产物）。
  /// kind/area 为空时列出全部；area 取值：imported（素材库）/ network（人物项目）/ insight（知识沉淀）。
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async {
    final dtos = await api.listWikiPages(kind: kind, area: area);
    return dtos.map(WikiPage.fromDto).toList();
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

  /// MCP 服务器管理（用户自定义外部工具源，迁移 44）。
  Future<List<api.McpServerDto>> listMcpServers() => api.listMcpServers();

  /// 只读免审批白名单（迁移 53）：仅对该服务器下标了 readOnlyHint 的
  /// 工具免逐次审批；不盲信上游标注，开关由用户按服务器显式打开。
  Future<void> setMcpServerAllowReadonlyAuto({
    required String id,
    required bool allow,
  }) => api.setMcpServerAllowReadonlyAuto(id: id, allow: allow);

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

  /// 与某页相关的人物关系（双向：作为人物方或作为事情/项目方）
  Future<List<Relation>> listRelationsForPage(String slug) async {
    final dtos = await api.listRelationsForPage(slug: slug);
    return dtos.map(Relation.fromDto).toList();
  }

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

  /// 获取（不存在则创建）知识页处理会话；页内 AI 聊天走该会话
  Future<Conversation> ensureWikiPageChat(String pageSlug) async {
    final dto = await api.ensureWikiPageChat(pageSlug: pageSlug);
    return _fromConversationDto(dto);
  }
}

/// Storage repository provider
final storageRepositoryProvider = Provider<StorageRepository>((ref) {
  return RustBridgeRepository();
});

/// 后台 AI 活动（标题栏指示器）。非桥接仓库（mock）恒为 null。
final backgroundAiActivityProvider = Provider<AiActivity?>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return null;
  final activity = repo.backgroundAiActivity;
  void onChange() => ref.invalidateSelf();
  activity.addListener(onChange);
  ref.onDispose(() => activity.removeListener(onChange));
  return activity.value;
});

/// 后台知识消化/整理是否进行中（知识运行记录刷新用）。
final knowledgeDigestBusyProvider = Provider<bool>((ref) {
  final activity = ref.watch(backgroundAiActivityProvider);
  return activity == AiActivity.digest || activity == AiActivity.insights;
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

/// 习惯问候插入/回答修订号：消息列表与卡片状态订阅它。
final habitNudgeRevisionProvider = Provider<int>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return 0;
  final revision = repo.habitNudgeRevision;
  void onChange() => ref.invalidateSelf();
  revision.addListener(onChange);
  ref.onDispose(() => revision.removeListener(onChange));
  return revision.value;
});

/// 侧边栏未读点：未处理习惯问候数。
final pendingHabitNudgesProvider = Provider<int>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return 0;
  final pending = repo.pendingHabitNudges;
  void onChange() => ref.invalidateSelf();
  pending.addListener(onChange);
  ref.onDispose(() => pending.removeListener(onChange));
  return pending.value;
});

/// 今日问候状态（卡片显示已记录/已推迟）。按需重查。
final habitNudgeStatesProvider = FutureProvider<List<api.HabitNudgeStateDto>>((
  ref,
) async {
  ref.watch(habitNudgeRevisionProvider);
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return const [];
  return repo.listHabitNudgeStates();
});

/// 单条问候计划（卡片按钮文案）。按 nudge id 缓存。
final habitNudgePlanProvider =
    FutureProvider.family<api.HabitNudgeDto?, String>((ref, id) async {
      ref.watch(habitNudgeRevisionProvider);
      final repo = ref.watch(storageRepositoryProvider);
      if (repo is! RustBridgeRepository) return null;
      return repo.getHabitNudge(id);
    });

/// 某个习惯页的问候计划（详情页入口文案）。
final habitNudgeForPageProvider =
    FutureProvider.family<api.HabitNudgeDto?, String>((ref, slug) async {
      ref.watch(habitNudgeRevisionProvider);
      final repo = ref.watch(storageRepositoryProvider);
      if (repo is! RustBridgeRepository) return null;
      return repo.getHabitNudgeForPage(slug);
    });
