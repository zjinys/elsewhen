part of '../rust_bridge_repository.dart';

//! RustBridgeRepository 便捷薄委托（chat_and_misc 域）。
extension RepoChatAndMisc on RustBridgeRepository {
  Future<bool> deleteTodo(String id) => api.deleteTodo(id: id);

  // ── 目标（FR-PES-005-01）──

  /// 列出全部活跃目标（最多 3 条）
  Future<List<Goal>> listActiveGoals() async {
    final dtos = await api.listActiveGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 列出已归档目标（历史所有非活跃，含暂停与达成）
  Future<List<Goal>> listArchivedGoals() async {
    final dtos = await api.listArchivedGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 列出暂停搁置的目标（历史）
  Future<List<Goal>> listPausedGoals() async {
    final dtos = await api.listPausedGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 列出已达成目标（成就）
  Future<List<Goal>> listCompletedGoals() async {
    final dtos = await api.listCompletedGoals();
    return dtos.map(Goal.fromDto).toList();
  }

  /// 获取单个目标
  Future<Goal?> getGoal(String id) async {
    final dto = await api.getGoal(id: id);
    return dto == null ? null : Goal.fromDto(dto);
  }

  // createGoal 已移至类内（虚分派：被测试 Fake 覆写，见主文件的 parts 规则注释）。

  /// 编辑目标正文与阶段（状态不变，故不受上限限制）
  Future<void> updateGoal({
    required String id,
    required String content,
    required GoalPhase phase,
  }) => api.updateGoal(id: id, content: content, phase: phase.wire);

  /// 更新目标的所有认知切片字段
  Future<void> updateGoalFacets({
    required String id,
    String? why,
    String? currentFocus,
    String? blocker,
    String? doneCriteria,
  }) => api.updateGoalFacets(
    id: id,
    why: why,
    currentFocus: currentFocus,
    blocker: blocker,
    doneCriteria: doneCriteria,
  );

  /// 完整更新目标的所有正文与切片字段
  Future<void> updateGoalFull({
    required String id,
    required String content,
    required GoalPhase phase,
    String? why,
    String? currentFocus,
    String? blocker,
    String? doneCriteria,
  }) => api.updateGoalFull(
    id: id,
    content: content,
    phase: phase.wire,
    why: why,
    currentFocus: currentFocus,
    blocker: blocker,
    doneCriteria: doneCriteria,
  );

  /// 暂停搁置目标，让出活跃名额但保留切片上下文作为历史以备随时复活
  Future<void> pauseGoal(String id) => api.pauseGoal(id: id);

  /// 达成目标，让出活跃名额并记录为成就勋章
  Future<void> completeGoal(String id) => api.completeGoal(id: id);

  /// 归档目标（向后兼容，等价于 pauseGoal）
  Future<void> archiveGoal(String id) => api.archiveGoal(id: id);

  /// 复活已归档目标。活跃目标已满 3 条时后端拒绝。支持传入新抓手与卡点。
  Future<void> reactivateGoal(
    String id, {
    String? currentFocus,
    String? blocker,
  }) =>
      api.reactivateGoal(id: id, currentFocus: currentFocus, blocker: blocker);

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

  /// 归档知识页处理会话（如页面删除时）
  Future<void> archiveWikiPageChat(String pageSlug) =>
      api.archiveWikiPageChat(pageSlug: pageSlug);
}
