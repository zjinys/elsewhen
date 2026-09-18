import '../bridge/generated.dart/api.dart' as api;

/// 任意 URL 抓取结果（推文或普通网页；尚未入库）
class ImportFetch {
  final String sourceUrl;

  /// "tweet" | "webpage"
  final String sourceKind;
  final String? title;
  final String contentMd;
  final String? authorName;
  final String? screenName;
  final String? inputRecordId;

  const ImportFetch({
    required this.sourceUrl,
    required this.sourceKind,
    this.title,
    required this.contentMd,
    this.authorName,
    this.screenName,
    this.inputRecordId,
  });

  factory ImportFetch.fromDto(api.ImportUrlDto dto) {
    return ImportFetch(
      sourceUrl: dto.sourceUrl,
      sourceKind: dto.sourceKind,
      title: dto.title,
      contentMd: dto.contentMd,
      authorName: dto.authorName,
      screenName: dto.screenName,
    );
  }

  ImportFetch withInputRecord(String id) => ImportFetch(
    sourceUrl: sourceUrl,
    sourceKind: sourceKind,
    title: title,
    contentMd: contentMd,
    authorName: authorName,
    screenName: screenName,
    inputRecordId: id,
  );

  bool get isTweet => sourceKind == 'tweet';

  /// 展示标题：优先网页/文章标题，其次推文作者或链接
  String get displayTitle {
    final t = title?.trim() ?? '';
    if (t.isNotEmpty) return t;
    if (authorName != null && authorName!.trim().isNotEmpty) {
      return authLabel;
    }
    return sourceUrl;
  }

  String get authLabel {
    if (screenName != null && screenName!.trim().isNotEmpty) {
      return '@$screenName';
    }
    return authorName ?? '未知作者';
  }
}
