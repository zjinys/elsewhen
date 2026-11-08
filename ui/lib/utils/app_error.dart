import 'dart:convert';

import '../l10n/app_localizations.dart';

/// Explicit Rust presentation envelope. Unknown versions/codes use the normal
/// localized fallback; diagnostic prose is never treated as a translation key.
class AppError {
  const AppError(this.code, this.parameters);
  final String code;
  final Map<String, Object?> parameters;

  static AppError? decode(Object error) {
    var raw = error.toString().trim();
    if (raw.length > 65536) return null;
    if (raw.startsWith('AnyhowException(')) {
      raw = raw.substring('AnyhowException('.length);
      if (raw.endsWith(')')) raw = raw.substring(0, raw.length - 1);
    }
    if (raw.startsWith('Exception: ')) {
      raw = raw.substring('Exception: '.length);
    }
    final first = raw.split('\n').first;
    const prefix = 'elsewhen-error-v1:';
    if (!first.startsWith(prefix)) {
      // Compatibility with older Rust libraries that already emitted these
      // exact machine codes. Human and third-party prose never matches here.
      if (const {
        'object_conflict_local_changed',
        'object_conflict_not_pending',
        'goal_active_limit',
      }.contains(first)) {
        return AppError(first, const {});
      }
      return null;
    }
    if (first.length > 4096) return null;
    try {
      final json = jsonDecode(first.substring(prefix.length));
      if (json is! Map<String, dynamic> ||
          json['version'] != 1 ||
          json['code'] is! String ||
          json['params'] is! Map<String, dynamic>) {
        return null;
      }
      final params = json['params'] as Map<String, dynamic>;
      if (params.length > 16 ||
          params.values.any(
            (value) =>
                value is! String &&
                value is! num &&
                value is! bool &&
                value != null,
          )) {
        return null;
      }
      return AppError(json['code'] as String, params);
    } on FormatException {
      return null;
    }
  }

  String? message(AppLocalizations strings) => switch (code) {
    'field_required' when parameters['field'] == 'title' =>
      strings.errorFieldRequired(strings.title),
    'not_found' when parameters['resource'] == 'todo' => strings.errorNotFound(
      strings.todos,
    ),
    'not_found' when parameters['resource'] == 'wiki_page' =>
      strings.errorNotFound(strings.knowledgePage),
    'wiki_page_changed' => strings.errorWikiPageChanged,
    'knowledge_source_changed' => strings.errorKnowledgeSourceChanged,
    'knowledge_review_changed' => strings.errorKnowledgeReviewChanged,
    'wiki_material_readonly' => strings.errorWikiMaterialReadonly,
    'wiki_content_too_long' => strings.errorWikiContentTooLong,
    'field_required' when parameters['field'] == 'content' =>
      strings.errorWikiContentRequired,
    'settings_changed' => strings.errorSettingsChanged,
    'object_conflict_local_changed' => strings.errorConflictChanged,
    'object_conflict_not_pending' => strings.errorConflictResolved,
    'goal_active_limit' => strings.errorGoalLimit,
    'owner_required' => strings.knowledgeLanguageOwnerOnly,
    'sync_unlock_required' => strings.errorSyncUnlockRequired,
    'language_capability_required' =>
      strings.knowledgeLanguageCapabilityRequired,
    'language_unsupported' => strings.errorLanguageUnsupported,
    'reminder_time_past' => strings.errorReminderTimePast,
    'reminder_time_required' => strings.errorReminderTimeRequired,
    _ => null,
  };
}
