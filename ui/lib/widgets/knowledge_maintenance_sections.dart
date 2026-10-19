import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../utils/error_report.dart';
import '../theme/app_theme.dart';
import 'markdown_view.dart';

part 'knowledge_maintenance_parts/maintenance_repair.dart';
part 'knowledge_maintenance_parts/maintenance_review_history.dart';
part 'knowledge_maintenance_parts/maintenance_issue_resolution.dart';
part 'knowledge_maintenance_parts/maintenance_work_queue.dart';

String _failure(Object error, String action) => reportUiError(action, error);

/// RFC3339 时间戳 → 简短可读格式（`MM-dd HH:mm`，当年省年份）。
/// 解析失败（非标准格式）时原样返回，不丟信息。
String _shortTime(String rfc3339) {
  final dt = DateTime.tryParse(rfc3339)?.toLocal();
  if (dt == null) return rfc3339;
  String two(int n) => n.toString().padLeft(2, '0');
  final now = DateTime.now();
  final date = dt.year == now.year
      ? '${two(dt.month)}-${two(dt.day)}'
      : '${dt.year}-${two(dt.month)}-${two(dt.day)}';
  return '$date ${two(dt.hour)}:${two(dt.minute)}';
}

String _repairFailure(Object error) {
  final message = _failure(error, '准备来源修复');
  final raw = error.toString();
  if (raw.contains('依据不足')) {
    return '剩余依据不足，请先补充有效原料。当前知识保留并等待补证据。';
  }
  if (raw.contains('后台阅读全文')) {
    return '所选原料正在后台阅读全文，请在阅读完成后准备修复建议。';
  }
  if (raw.contains('上游知识')) {
    return '上游知识仍待复核，请先打开上游页面检查更新。';
  }
  return '$message 可刷新来源后重试；当前正文未修改。';
}
