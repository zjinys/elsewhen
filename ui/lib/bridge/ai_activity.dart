import '../l10n/app_localizations_zh.dart';

/// 后台 AI 活动类型：标题栏指示器按调用方显示不同文案。
enum AiActivity {
  chat,
  analysis,
  digest,
  insights,
  periodReport;

  /// 标题栏胶囊里的短文案。
  String get label => switch (this) {
    chat => AppLocalizationsZh().aiReplying,
    analysis => AppLocalizationsZh().aiAnalyzing,
    digest => AppLocalizationsZh().aiDigesting,
    insights => AppLocalizationsZh().aiOrganizing,
    periodReport => AppLocalizationsZh().aiChecking,
  };

  /// 悬浮提示里的完整说明。
  String get tooltip => switch (this) {
    chat => AppLocalizationsZh().aiReplyingHint,
    analysis => AppLocalizationsZh().aiAnalyzingHint,
    digest => AppLocalizationsZh().aiDigestingHint,
    insights => AppLocalizationsZh().aiOrganizingHint,
    periodReport => AppLocalizationsZh().aiCheckingHint,
  };
}
