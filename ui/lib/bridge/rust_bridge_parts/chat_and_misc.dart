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


  /// 归档知识页处理会话（如页面删除时）
  Future<void> archiveWikiPageChat(String pageSlug) =>
      api.archiveWikiPageChat(pageSlug: pageSlug);
}
