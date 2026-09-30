import '../bridge/api.dart' as api;

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
  final String? sourceUrl;
  final String area;
  final String? basedOn;
  final String? contentType;

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
    this.sourceUrl,
    required this.area,
    this.basedOn,
    this.contentType,
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
      firstSeenAt: DateTime.parse(dto.firstSeenAt).toLocal(),
      lastSeenAt: DateTime.parse(dto.lastSeenAt).toLocal(),
      status: dto.status,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
      sourceUrl: dto.sourceUrl,
      area: dto.area,
      basedOn: dto.basedOn,
      contentType: dto.contentType,
    );
  }

  /// kind 的中文展示名
  String get kindLabel {
    const labels = {
      'profile': '档案',
      'person': '联系人',
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

  /// 来源/用途分区的中文名
  String get areaLabel {
    const labels = {
      'imported': '素材库',
      'network': '联系人/项目',
      'insight': '知识沉淀',
      'derivative': '派生产物',
    };
    return labels[area] ?? area;
  }

  /// 素材来源标识：tweet- 前缀推文、带网址的网页、其余（粘贴文本）→ 素材
  String? get sourceKind {
    if (area != 'imported') return null;
    if (slug.startsWith('tweet-')) return 'tweet';
    if (sourceUrl != null) return 'web';
    return 'text';
  }

  /// 是否本地目录来源（sourceUrl 为 file://）：目录导入的项目页把路径记在这里。
  bool get isLocalPath => sourceUrl != null && sourceUrl!.startsWith('file://');

  /// 本地目录路径（仅 isLocalPath 时非空；file:// 解码为系统路径）。
  /// 与 Rust 侧 path_to_file_url 对应：空 host 或 localhost + 百分号解码。
  String? get localPath {
    final url = sourceUrl;
    if (url == null || !url.startsWith('file://')) return null;
    var rest = url.substring('file://'.length);
    if (rest.startsWith('localhost/'))
      rest = rest.substring('localhost'.length);
    return Uri.decodeComponent(rest);
  }
}
