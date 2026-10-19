import '../bridge/api.dart' as api;

/// 知识页的元数据字段面：全页对象（WikiPage）与列表元数据（WikiPageMeta）
/// 共同实现。openWikiPageTab 两态通用；展示 getter（kindLabel 等）在
/// WikiPageRefX extension 上，两态共享一份实现。
abstract class WikiPageRef {
  String get id;
  String get slug;
  String get kind;
  String get title;
  String get summary;
  List<String> get tags;
  int get evidenceCount;
  DateTime get firstSeenAt;
  DateTime get lastSeenAt;
  String get status;
  DateTime get createdAt;
  DateTime get updatedAt;
  String? get sourceUrl;
  String get area;
}

/// Wiki page model（知识库页面，LLM wiki 的编译产物）
class WikiPage implements WikiPageRef {
  @override
  final String id;
  @override
  final String slug;
  @override
  final String kind;
  @override
  final String title;
  @override
  final String summary;
  final String contentMd;
  @override
  final List<String> tags;
  final List<String> sourceEventIds;
  @override
  final int evidenceCount;
  @override
  final DateTime firstSeenAt;
  @override
  final DateTime lastSeenAt;
  @override
  final String status;
  @override
  final DateTime createdAt;
  @override
  final DateTime updatedAt;
  @override
  final String? sourceUrl;
  @override
  final String area;
  final String? basedOn;
  final String? contentType;
  /// 孵化定论（提案 2026-10-19）：null=孵化中（source/note 待定论），
  /// 'endorse'/'reject' 已定。非素材 kind 恒 null。
  final String? opinion;

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
    this.opinion,
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
      opinion: dto.opinion,
    );
  }
}

/// 列表元数据视图（无正文）：左栏/列表场景专用。
///
/// 刻意与 [WikiPage] 分开而不是给它塞个空 contentMd——空正文对象一旦被当成
/// 全页用（详情/保存路径）就是一致性炸弹。明细一律按 slug 经
/// `wikiPageProvider(slug)` 单页读取。openWikiPageTab 只需要 slug+title，
/// 由共同接口 [WikiPageRef] 承载。
class WikiPageMeta implements WikiPageRef {
  @override
  final String id;
  @override
  final String slug;
  @override
  final String kind;
  @override
  final String title;
  @override
  final String summary;
  @override
  final List<String> tags;
  @override
  final int evidenceCount;
  @override
  final DateTime firstSeenAt;
  @override
  final DateTime lastSeenAt;
  @override
  final String status;
  @override
  final DateTime createdAt;
  @override
  final DateTime updatedAt;
  @override
  final String? sourceUrl;
  @override
  final String area;
  /// 孵化定论：null=孵化中；'endorse'/'reject' 已定（非素材 kind 恒 null）。
  final String? opinion;

  const WikiPageMeta({
    required this.id,
    required this.slug,
    required this.kind,
    required this.title,
    required this.summary,
    required this.tags,
    required this.evidenceCount,
    required this.firstSeenAt,
    required this.lastSeenAt,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
    this.sourceUrl,
    required this.area,
    this.opinion,
  });

  factory WikiPageMeta.fromDto(api.WikiPageMetaDto dto) {
    return WikiPageMeta(
      id: dto.id,
      slug: dto.slug,
      kind: dto.kind,
      title: dto.title,
      summary: dto.summary,
      tags: dto.tags,
      evidenceCount: dto.evidenceCount.toInt(),
      firstSeenAt: DateTime.parse(dto.firstSeenAt).toLocal(),
      lastSeenAt: DateTime.parse(dto.lastSeenAt).toLocal(),
      status: dto.status,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
      sourceUrl: dto.sourceUrl,
      area: dto.area,
      opinion: dto.opinion,
    );
  }

  @override
  bool operator ==(Object other) => other is WikiPageMeta && other.slug == slug;

  @override
  int get hashCode => slug.hashCode;
}

/// 孵化判据（摘录闭环，提案 2026-10-19）的单一口径：素材 kind（source/
/// note）待定论（opinion 未定）。侧栏孵化分组与详情页动作按钮共用；
/// 非素材 kind（insight/派生稿）恒 false。
bool isIncubatingPage(String kind, String? opinion) {
  return (kind == 'source' || kind == 'note') && opinion == null;
}

/// 展示 helper（两态共用）：只依赖元数据字段面。
extension WikiPageRefX on WikiPageRef {  /// kind 的中文展示名
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
