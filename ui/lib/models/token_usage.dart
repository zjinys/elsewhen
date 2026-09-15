import '../bridge/generated.dart/api.dart' as api;

/// 每日 token 用量统计（按天聚合，日期倒序）
class DailyTokenUsage {
  final String date; // YYYY-MM-DD
  final int promptTokens;
  final int completionTokens;
  final int totalTokens;
  final int callCount;

  const DailyTokenUsage({
    required this.date,
    required this.promptTokens,
    required this.completionTokens,
    required this.totalTokens,
    required this.callCount,
  });

  factory DailyTokenUsage.fromDto(api.DailyTokenUsageDto dto) {
    return DailyTokenUsage(
      date: dto.date,
      promptTokens: dto.promptTokens.toInt(),
      completionTokens: dto.completionTokens.toInt(),
      totalTokens: dto.totalTokens.toInt(),
      callCount: dto.callCount.toInt(),
    );
  }
}