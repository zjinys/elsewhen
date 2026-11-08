import 'dart:convert';

import 'app_localizations.dart';

/// Status receipts are independent of UI and knowledge language. Historical
/// prose remains available as a diagnostic and never becomes a translation key.
class ProcessingResult {
  const ProcessingResult(this.code, this.count, this.detail);
  final String code;
  final int? count;
  final String detail;
  static ProcessingResult? decode(String raw) {
    const prefix = 'elsewhen-result-v1:';
    if (!raw.startsWith(prefix) || raw.length > 65536) return null;
    try {
      final data = jsonDecode(raw.substring(prefix.length));
      if (data is! Map<String, dynamic> ||
          data['version'] != 1 ||
          data['code'] is! String ||
          data['detail'] is! String ||
          data['count'] != null && data['count'] is! int) {
        return null;
      }
      return ProcessingResult(
        data['code'] as String,
        data['count'] as int?,
        data['detail'] as String,
      );
    } on FormatException {
      return null;
    }
  }

  String? message(AppLocalizations strings) => switch (code) {
    'page_reprocessed' when count != null && count! >= 0 =>
      strings.pageReprocessed(count!),
    'page_human_protected' => strings.pageHumanProtected,
    _ => null,
  };
}
