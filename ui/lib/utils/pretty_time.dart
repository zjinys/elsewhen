import 'package:intl/intl.dart';

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
String prettyTime(DateTime time, {DateTime? now}) {
  final reference = now ?? DateTime.now();
  final diff = reference.difference(time);
  if (diff.inMinutes < 1) return '刚刚';
  if (diff.inMinutes < 60) return '${diff.inMinutes} 分钟前';
  if (diff.inHours < 24) return '${diff.inHours} 小时前';
  if (diff.inDays < 7) return '${diff.inDays} 天前';
  final sameYear = time.year == reference.year;
  return DateFormat(sameYear ? 'MM-dd' : 'yyyy-MM-dd').format(time);
}

/// 便捷组合：解析 + prettyTime；无法解析时返回原样截断（兜底可读）。
String? prettyTimestamp(String? raw, {DateTime? now}) {
  final time = tryParseTimestamp(raw);
  if (time == null) return raw == null || raw.isEmpty ? null : raw;
  return prettyTime(time, now: now);
}
