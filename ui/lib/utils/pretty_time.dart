import '../l10n/app_localizations_zh.dart';

import 'package:intl/intl.dart';

import '../l10n/app_localizations.dart';

/// RFC3339 字符串 → DateTime；解析失败返回 null（旧数据/异常数据兜底）。
DateTime? tryParseTimestamp(String? raw) {
  if (raw == null || raw.isEmpty) return null;
  return DateTime.tryParse(raw)?.toLocal();
}

/// 相对时间的「漂亮打印」，列表元数据用：
/// - <1 分钟 → 刚刚
/// - <60 分钟 → n 分钟前
/// - <24 小时 → n 小时前
/// - <7 天 → n 天前
/// - 今年 → MM-dd
/// - 跨年 → yyyy-MM-dd
String prettyTime(DateTime time, {DateTime? now, AppLocalizations? strings}) {
  final reference = now ?? DateTime.now();
  final diff = reference.difference(time);
  if (diff.inMinutes < 1) return (strings ?? AppLocalizationsZh()).justNow;
  if (diff.inMinutes < 60) {
    return (strings ?? AppLocalizationsZh()).minutesAgo(diff.inMinutes);
  }
  if (diff.inHours < 24) {
    return (strings ?? AppLocalizationsZh()).hoursAgo(diff.inHours);
  }
  if (diff.inDays < 7) {
    return (strings ?? AppLocalizationsZh()).daysAgo(diff.inDays);
  }
  final sameYear = time.year == reference.year;
  if (strings != null) {
    return (sameYear
            ? DateFormat.Md(strings.localeName)
            : DateFormat.yMd(strings.localeName))
        .format(time);
  }
  return DateFormat(sameYear ? 'MM-dd' : 'yyyy-MM-dd').format(time);
}

/// 便捷组合：解析 + prettyTime；无法解析时返回原样截断（兜底可读）。
String? prettyTimestamp(
  String? raw, {
  DateTime? now,
  AppLocalizations? strings,
}) {
  final time = tryParseTimestamp(raw);
  if (time == null) return raw == null || raw.isEmpty ? null : raw;
  return prettyTime(time, now: now, strings: strings);
}
