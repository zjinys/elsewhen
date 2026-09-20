import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

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
import 'generated.dart/api.dart' as api;
import 'generated.dart/frb_generated.dart';

/// Rust bridge implementation of storage repository
class RustBridgeRepository implements StorageRepository {
  final String? databasePath;
  bool _initialized = false;
  bool _analysisWorkerRunning = false;
  bool _analysisWorkerScheduled = false;
  bool _analysisRerunRequested = false;
  bool _disposed = false;
  Timer? _analysisTimer;

  RustBridgeRepository({this.databasePath});

  @override
  Future<void> initialize() async {
    if (_initialized) return;

    await RustLib.init();
    final initializedPath = await api.initBridge(databasePath: databasePath);
    if (initializedPath.startsWith('Error ')) {
      throw StateError(initializedPath);
    }
    _initialized = true;
    // Resume durable work after process restart. The timer also covers retry
    // backoff expiry and the Rust-side 50-job invocation bound.
    _analysisTimer = Timer.periodic(const Duration(seconds: 5), (_) {
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

  Future<String> generateDailyReview(String date) =>
      api.generateDailyReview(date: date);

  Future<List<api.EntityFactDto>> listEntityFacts(
    String entityKind,
    String entitySlug,
  ) => api.listEntityFacts(entityKind: entityKind, entitySlug: entitySlug);

  Future<bool> deleteEntityFact(String id) => api.deleteEntityFact(id: id);

  Future<api.EventAnalysisDetailDto?> getEventAnalysisDetail(String eventId) =>
      api.getEventAnalysisDetail(eventId: eventId);

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

  @override
  Future<String?> getAiProvider() async {
    return await api.getAiProvider();
  }

  @override
  Future<String> triggerAnalysis() async {
    return await api.triggerAnalysis();
  }

  /// Schedule analysis without making input persistence wait for the network.
  /// Multiple saves coalesce into one in-flight invocation.
  void _wakeAnalysisWorker() {
    if (!_initialized || _disposed) return;
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
            if (result == 'no_provider' || !result.startsWith('processed:')) {
              break;
            }
            final stats = await getAnalysisJobStats();
            if (stats.pending == 0) break;
          }
        } while (_analysisRerunRequested && !_disposed);
      } catch (_) {
        // Queue state remains durable; the next wake retries after startup or
        // the periodic timer without surfacing an error in the save path.
      } finally {
        _analysisWorkerRunning = false;
        if (_analysisRerunRequested) _wakeAnalysisWorker();
      }
    });
  }

  /// Stop lifecycle polling. Production keeps the repository for the process
  /// lifetime; isolated tests call this before deleting their temporary data.
  void dispose() {
    _disposed = true;
    _analysisTimer?.cancel();
    _analysisTimer = null;
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
          ),
        )
        .toList();
  }

  Future<String> addRule(String content) => api.addRule(content: content);

  Future<void> deleteRule(String ruleId) => api.deleteRule(ruleId: ruleId);

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
          ),
        )
        .toList();
  }

  Future<List<api.PendingActionDto>> listPendingActions(String conversationId) =>
      api.listPendingActions(conversationId: conversationId);

  Future<bool> updatePendingActionArgs(String actionId, String argsJson) =>
      api.updatePendingActionArgs(actionId: actionId, argsJson: argsJson);

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

  /// 某页的派生产物列表（AI 加工成果：总结/提炼/文案…）
  Future<List<WikiPage>> listWikiPageDerivatives(String slug) async {
    final dtos = await api.listWikiPageDerivatives(slug: slug);
    return dtos.map(WikiPage.fromDto).toList();
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

  /// 主题偏好（模式 + 预设，存 app_meta）
  Future<api.ThemePrefsDto> getThemePrefs() => api.getThemePrefs();

  Future<void> updateThemePrefs({
    required String mode,
    required String preset,
  }) => api.updateThemePrefs(mode: mode, preset: preset);

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
  Future<void> setActiveAiProviderConfig(String id) =>
      api.setActiveAiProviderConfig(id: id);

  /// 删除配置（删除激活项时剩余第一个自动接管激活）
  Future<void> deleteAiProviderConfig(String id) =>
      api.deleteAiProviderConfig(id: id);

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
