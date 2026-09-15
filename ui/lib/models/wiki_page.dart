import '../bridge/generated.dart/api.dart' as api;

/// Wiki page model（知识库页面，LLM wiki 的编译产物）
class WikiPage {
  final String id;
  final String slug;
  final String kind;
  final String title;
  final String summary;
  final String contentMd;
  final List<String> tags;
  final List<String> sourceEventIds;
  final int evidenceCount;
  final DateTime firstSeenAt;
  final DateTime lastSeenAt;
  final String status;
  final DateTime createdAt;
  final DateTime updatedAt;

  const WikiPage({
    required this.id,
    required this.slug,
    required this.kind,
    required this.title,
    required this.summary,
    required this.contentMd,
    required this.tags,
    required this.sourceEventIds,
    required this.evidenceCount,
    required this.firstSeenAt,
    required this.lastSeenAt,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
  });

  factory WikiPage.fromDto(api.WikiPageDto dto) {
    return WikiPage(
      id: dto.id,
      slug: dto.slug,
      kind: dto.kind,
      title: dto.title,
      summary: dto.summary,
      contentMd: dto.contentMd,
      tags: dto.tags,
      sourceEventIds: dto.sourceEventIds,
      evidenceCount: dto.evidenceCount.toInt(),
      firstSeenAt: DateTime.parse(dto.firstSeenAt),
      lastSeenAt: DateTime.parse(dto.lastSeenAt),
      status: dto.status,
      createdAt: DateTime.parse(dto.createdAt),
      updatedAt: DateTime.parse(dto.updatedAt),
    );
  }

  /// kind 的中文展示名
  String get kindLabel {
    const labels = {
      'profile': '档案',
      'recurring_cost': '固定成本',
      'capability': '能力',
      'asset': '资产',
      'project': '项目',
      'relationship': '关系',
      'decision': '决策',
      'habit': '习惯',
      'constraint': '约束',
      'insight': '洞察',
      'topic': '主题',
      'method': '方法',
      'case': '案例',
      'principle': '规律',
      'series': '系列',
      'source': '来源',
    };
    return labels[kind] ?? kind;
  }
}