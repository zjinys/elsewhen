import '../bridge/api.dart' as api;

/// 抓取到的推文内容（从 fxtwitter 响应解析，未入库）
class TweetFetch {
  final String tweetId;
  final String url;

  /// 可读正文：普通推文为推文原文；文章型推文为 article 正文（不含标题）
  final String text;

  /// 文章型推文的标题（article.title），普通推文为 null
  final String? title;
  final String? authorName;
  final String? screenName;
  final String? inputRecordId;

  const TweetFetch({
    required this.tweetId,
    required this.url,
    required this.text,
    this.title,
    this.authorName,
    this.screenName,
    this.inputRecordId,
  });

  factory TweetFetch.fromDto(api.TweetFetchDto dto) {
    return TweetFetch(
      tweetId: dto.tweetId,
      url: dto.url,
      text: dto.text,
      title: dto.title,
      authorName: dto.authorName,
      screenName: dto.screenName,
    );
  }

  TweetFetch withInputRecord(String id) => TweetFetch(
    tweetId: tweetId,
    url: url,
    text: text,
    title: title,
    authorName: authorName,
    screenName: screenName,
    inputRecordId: id,
  );

  /// 给 AI 的完整内容（文章型推文带上标题，增强上下文）
  String get fullContent {
    final t = title?.trim() ?? '';
    return t.isEmpty ? text : '$t\n\n$text';
  }
}

/// 内容对话的一条临时消息（不入库，仅用于保存前与 AI 讨论抓取内容）
class ContentChatMessage {
  final String role; // 'user' | 'assistant'
  final String content;

  const ContentChatMessage({required this.role, required this.content});

  api.ContentChatMessageDto get toDto =>
      api.ContentChatMessageDto(role: role, content: content);
}
