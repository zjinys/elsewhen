part of '../rust_bridge_repository.dart';

//! RustBridgeRepository 便捷薄委托（conversations 域）。
extension RepoConversations on RustBridgeRepository {

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


  /// 最近 N 天的每日 token 用量统计（含当天，日期倒序）
  Future<List<DailyTokenUsage>> getDailyTokenUsage(int days) async {
    final dtos = await api.getDailyTokenUsage(days: days);
    return dtos.map(DailyTokenUsage.fromDto).toList();
  }

  // Wiki methods


  /// 页面列表元数据（sidebar 专用）：无正文、无图片解析、SQL 级有界
  /// （默认 500 条，Rust 侧按 updated_at 倒序）。明细按 slug 单页读。
  Future<List<WikiPageMeta>> listWikiPageMetas({int? limit}) async {
    final dtos = await api.listWikiPageMetas(limit: limit);
    return dtos.map(WikiPageMeta.fromDto).toList();
  }


  /// 创建记事本（kind=memo 唯一建档入口）
  Future<WikiPage> createWikiPage({
    required String title,
    String contentMd = '',
    List<String> tags = const [],
  }) async {
    final dto = await api.createWikiPage(
        title: title, contentMd: contentMd, tags: tags);
    return WikiPage.fromDto(dto);
  }

  /// 追加记事本正文（快记框唯一写入口；单事务）
  Future<WikiPage> appendWikiPageContent({
    required String slug,
    required String appendMd,
    String reason = '快记',
  }) async {
    final dto = await api.appendWikiPageContent(
        slug: slug, appendMd: appendMd, reason: reason);
    return WikiPage.fromDto(dto);
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


  Future<String> saveMcpServer(api.McpServerDto server) =>
      api.saveMcpServer(server: server);

  Future<void> deleteMcpServer({required String id}) =>
      api.deleteMcpServer(id: id);

  Future<void> setMcpServerEnabled({required String id, required bool enabled}) =>
      api.setMcpServerEnabled(id: id, enabled: enabled);

  /// 免审批开关（迁移 47）：trusted=true 后该服务器工具调用直接执行。
  Future<void> setMcpServerTrusted({required String id, required bool trusted}) =>
      api.setMcpServerTrusted(id: id, trusted: trusted);


  /// 连通性测试：拉起进程走握手并取回工具清单。
  Future<List<api.McpToolDto>> testMcpServer(api.McpServerDto server) =>
      api.testMcpServer(server: server);

  /// 用户技能启停（写 skills_state.json，不影响技能文件）。
  Future<void> setUserSkillEnabled({required String id, required bool enabled}) =>
      api.setUserSkillEnabled(id: id, enabled: enabled);

  /// 免审批执行开关（与 MCP trusted 同模型）。
  Future<void> setUserSkillTrusted({required String id, required bool trusted}) =>
      api.setUserSkillTrusted(id: id, trusted: trusted);

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
    bool reminder = false,
  }) async {
    final dto = await api.createTodo(
      title: title,
      dueAt: dueAt,
      priority: priority,
      relatedWikiSlug: relatedWikiSlug,
      note: note,
      reminder: reminder,
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
}
