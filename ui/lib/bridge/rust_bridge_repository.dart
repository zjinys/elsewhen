import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../data/storage_repository.dart';
import '../models/event.dart';
import '../models/analysis.dart';
import '../models/conversation.dart';
import '../models/wiki_page.dart';
import '../models/token_usage.dart';
import '../models/tweet_fetch.dart';
import 'generated.dart/api.dart' as api;
import 'generated.dart/frb_generated.dart';

/// Rust bridge implementation of storage repository
class RustBridgeRepository implements StorageRepository {
  bool _initialized = false;

  @override
  Future<void> initialize() async {
    if (_initialized) return;

    await RustLib.init();
    _initialized = true;
  }

  @override
  Future<Event> recordEvent(String rawText) async {
    final dto = await api.recordEvent(rawText: rawText);
    return Event(
      id: dto.id,
      rawText: dto.rawText,
      recordedAt: DateTime.parse(dto.recordedAt),
      source: dto.source,
      status: dto.status,
    );
  }

  @override
  Future<List<Event>> listEvents() async {
    final dtos = await api.listEvents();
    return dtos.map((dto) => Event(
      id: dto.id,
      rawText: dto.rawText,
      recordedAt: DateTime.parse(dto.recordedAt),
      source: dto.source,
      status: dto.status,
    )).toList();
  }

  @override
  Future<List<Analysis>> listAnalyses() async {
    final dtos = await api.listAnalyses();
    return dtos.map((dto) => Analysis(
      eventType: dto.eventType,
      confidence: dto.confidence,
      summary: dto.summary,
      clarifications: dto.clarifications,
    )).toList();
  }

  @override
  Future<String?> getAiProvider() async {
    return await api.getAiProvider();
  }

  @override
  Future<String> triggerAnalysis() async {
    return await api.triggerAnalysis();
  }

  // Conversation methods

  Future<List<Conversation>> listConversations() async {
    final dtos = await api.listConversations();
    return dtos.map((dto) => Conversation(
      id: dto.id,
      title: dto.title,
      tag: dto.tag,
      createdAt: DateTime.parse(dto.createdAt),
      updatedAt: DateTime.parse(dto.updatedAt),
      messageCount: dto.messageCount,
      lastMessagePreview: dto.lastMessagePreview,
      archived: dto.archived,
    )).toList();
  }

  Future<List<Conversation>> listArchivedConversations() async {
    final dtos = await api.listArchivedConversations();
    return dtos.map((dto) => Conversation(
      id: dto.id,
      title: dto.title,
      tag: dto.tag,
      createdAt: DateTime.parse(dto.createdAt),
      updatedAt: DateTime.parse(dto.updatedAt),
      messageCount: dto.messageCount,
      lastMessagePreview: dto.lastMessagePreview,
      archived: dto.archived,
    )).toList();
  }

  Future<void> renameConversation(String conversationId, String title) =>
      api.renameConversation(conversationId: conversationId, title: title);

  Future<void> setConversationArchived(String conversationId, bool archived) =>
      api.setConversationArchived(conversationId: conversationId, archived: archived);

  Future<Conversation> createConversation({String? title, String? tag}) async {
    final dto = await api.createConversation(title: title, tag: tag);
    return Conversation(
      id: dto.id,
      title: dto.title,
      tag: dto.tag,
      createdAt: DateTime.parse(dto.createdAt),
      updatedAt: DateTime.parse(dto.updatedAt),
      messageCount: dto.messageCount,
      lastMessagePreview: dto.lastMessagePreview,
      archived: dto.archived,
    );
  }

  Future<List<Message>> listMessages(String conversationId) async {
    final dtos = await api.listMessages(conversationId: conversationId);
    return dtos.map((dto) => Message(
      id: dto.id,
      conversationId: dto.conversationId,
      parentMessageId: dto.parentMessageId,
      role: MessageRole.fromString(dto.role),
      content: dto.content,
      createdAt: DateTime.parse(dto.createdAt),
    )).toList();
  }

  Future<List<Message>> getChildMessages(String parentId) async {
    final dtos = await api.getChildMessages(parentId: parentId);
    return dtos.map((dto) => Message(
      id: dto.id,
      conversationId: dto.conversationId,
      parentMessageId: dto.parentMessageId,
      role: MessageRole.fromString(dto.role),
      content: dto.content,
      createdAt: DateTime.parse(dto.createdAt),
    )).toList();
  }

  Future<List<Message>> getMessageChain(String messageId) async {
    final dtos = await api.getMessageChain(messageId: messageId);
    return dtos.map((dto) => Message(
      id: dto.id,
      conversationId: dto.conversationId,
      parentMessageId: dto.parentMessageId,
      role: MessageRole.fromString(dto.role),
      content: dto.content,
      createdAt: DateTime.parse(dto.createdAt),
    )).toList();
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
      createdAt: DateTime.parse(dto.createdAt),
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

  /// 列出知识库页面（kind 为空时列出全部）
  Future<List<WikiPage>> listWikiPages({String? kind}) async {
    final dtos = await api.listWikiPages(kind: kind);
    return dtos.map(WikiPage.fromDto).toList();
  }

  /// 按 slug 获取单个知识库页面
  Future<WikiPage?> getWikiPage(String slug) async {
    final dto = await api.getWikiPage(slug: slug);
    return dto == null ? null : WikiPage.fromDto(dto);
  }

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
  }) =>
      api.updateThemePrefs(mode: mode, preset: preset);

  /// AI provider 配置（设置页预填/保存用，直连 Rust DB 的 ai_provider_configs）
  Future<api.AiProviderConfigDto?> getAiProviderConfig() => api.getAiProviderConfig();

  Future<void> updateAiProviderConfig({
    required String baseUrl,
    required String model,
    required String apiKey,
  }) =>
      api.updateAiProviderConfig(
        baseUrl: baseUrl,
        model: model,
        apiKey: apiKey,
      );
}

/// Storage repository provider
final storageRepositoryProvider = Provider<StorageRepository>((ref) {
  return RustBridgeRepository();
});
