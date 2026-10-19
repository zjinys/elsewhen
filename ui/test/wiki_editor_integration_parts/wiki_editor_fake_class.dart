part of '../wiki_editor_integration_test.dart';

class _FakeRepo extends RustBridgeRepository {
  _FakeRepo({required this.page, this.wikilinkTarget});

  final WikiPage page;
  final WikiPage? wikilinkTarget;
  int savedCount = 0;
  bool saveThrows = false;
  List<Message> chatMessages = const [];

  /// 模拟「编辑期间页面被后台更新」：携带乐观锁的保存一律报冲突
  bool conflictOnLockedSave = false;
  final List<(String, String, String)> savedContent = [];
  final List<String?> savedExpectedUpdatedAt = [];
  final List<String> wikilinkLookups = [];

  @override
  Future<WikiPage?> getWikiPage(String slug) async {
    wikilinkLookups.add(slug);
    if (slug == wikilinkTarget?.slug) return wikilinkTarget;
    return page;
  }

  @override
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async => [
    page,
  ];

  @override
  Future<List<WikiPage>> listWikiPageDerivatives(String slug) async => const [];

  @override
  Future<List<EntityFactDto>> listEntityFacts(
    String entityKind,
    String entitySlug,
  ) async => const [];

  @override
  Future<List<Relation>> listRelationsForPage(String slug) async => const [];

  @override
  Future<WikiPage> saveWikiPageContent({
    required String slug,
    required String contentMd,
    String reason = 'GUI 编辑',
    String? expectedUpdatedAt,
  }) async {
    savedCount++;
    savedContent.add((slug, contentMd, reason));
    savedExpectedUpdatedAt.add(expectedUpdatedAt);
    if (conflictOnLockedSave && expectedUpdatedAt != null) {
      throw Exception('编辑冲突：页面在你编辑期间已被更新（模拟）');
    }
    if (saveThrows) throw Exception('模拟保存失败');
    return page;
  }

  @override
  Future<Conversation> ensureWikiPageChat(String pageSlug) async {
    return Conversation(
      id: 'conv-page',
      createdAt: DateTime(2026, 1, 1),
      updatedAt: DateTime(2026, 1, 1),
      messageCount: 0,
      wikiPageSlug: pageSlug,
    );
  }

  @override
  Future<List<Message>> listMessages(String conversationId) async =>
      chatMessages;

  @override
  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) async {
    return Message(
      id: 'm-reply',
      conversationId: conversationId,
      role: MessageRole.fromString('assistant'),
      content: '模拟回复',
      createdAt: DateTime(2026, 1, 1),
    );
  }
}
