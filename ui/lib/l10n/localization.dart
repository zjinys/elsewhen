import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:intl/date_symbol_data_local.dart';

import 'app_localizations.dart';
import 'notice_messages.dart';
import '../utils/error_report.dart';
import '../utils/app_error.dart';

export 'app_localizations.dart';

// The local symbol bundle registers synchronously before returning its Future.
// Standalone previews/widget hosts need the same date resources as app startup.
final _dateSymbols = initializeDateFormatting();

extension AppLocalizationContext on BuildContext {
  /// Standalone widget hosts without the application delegate retain the
  /// historical Chinese default. The application supplies its resolved locale.
  AppLocalizations get l10n {
    unawaited(_dateSymbols);
    return Localizations.of<AppLocalizations>(this, AppLocalizations) ??
        lookupAppLocalizations(const Locale('zh'));
  }

  String notice(Object? message) =>
      resolveNotice(message, l10n) ?? l10n.operationFailed(l10n.l6Actionf3ea6);

  String errorMessage(String action, Object error) => reportUiError(
    action,
    error,
    userMessage:
        resolveNotice(error, l10n) ??
        AppError.decode(error)?.message(l10n) ??
        l10n.operationFailed(action),
  );

  String artifactTypeLabel(String type) => l10n.artifactType(switch (type) {
    '摘要' => 'summary',
    '观点' => 'opinion',
    '灵感' => 'idea',
    '脚本' => 'script',
    'AI 总结' => 'ai_summary',
    _ => type,
  });

  String pageKindLabel(String kind) => switch (kind) {
    'profile' => l10n.pageKindProfile,
    'person' => l10n.pageKindPerson,
    'recurring_cost' => l10n.pageKindRecurringCost,
    'capability' => l10n.pageKindCapability,
    'asset' => l10n.pageKindAsset,
    'project' => l10n.pageKindProject,
    'relationship' => l10n.pageKindRelationship,
    'decision' => l10n.pageKindDecision,
    'habit' => l10n.pageKindHabit,
    'constraint' => l10n.pageKindConstraint,
    'insight' => l10n.pageKindInsight,
    'topic' => l10n.pageKindTopic,
    'method' => l10n.pageKindMethod,
    'case' => l10n.pageKindCase,
    'principle' => l10n.pageKindPrinciple,
    'series' => l10n.pageKindSeries,
    'source' => l10n.pageKindSource,
    'note' => l10n.pageKindNote,
    _ => kind,
  };
}
