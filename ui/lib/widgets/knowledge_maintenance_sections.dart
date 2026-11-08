import '../l10n/notice_messages.dart';
import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'markdown_view.dart';

part 'knowledge_maintenance_parts/maintenance_repair.dart';
part 'knowledge_maintenance_parts/maintenance_review_history.dart';
part 'knowledge_maintenance_parts/maintenance_issue_resolution.dart';
part 'knowledge_maintenance_parts/maintenance_work_queue.dart';

String _failure(BuildContext context, Object error, String action) =>
    appFailure(error);

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

String _repairFailure(BuildContext context, Object error) => appFailure(error);
