import '../l10n/localization.dart';

import 'package:flutter/widgets.dart';

import 'dart:convert';

String _excerpt(AppLocalizations strings, Object? value, int limit) {
  if (value is! String) return '';
  final chars = value.runes.take(limit + 1).toList();
  final visible = String.fromCharCodes(chars.take(limit));
  final lower = visible.toLowerCase();
  if ([
    '<think',
    'reasoning_content',
    '<tool_call',
    '[工具调用]',
    '<|dsml|>',
  ].any(lower.contains)) {
    return strings.l6ContentContainingProtocolMarkersIsHidden50ad8;
  }
  return visible + (chars.length > limit ? strings.l6Excerptfba71 : '');
}

/// Project only known business-result fields, never dump arbitrary tool JSON.
/// These are historical retrieval results, not final answer citations or a fresh
/// page revision. No live lookup may attach newer identities to these excerpts.
String toolProgressDetails(
  String name,
  String outcome,
  Object? content, {
  AppLocalizations? localizations,
}) {
  final strings = localizations ?? lookupAppLocalizations(const Locale('zh'));
  if (outcome == 'failed') return strings.l6ThisStepFailedCheckTheFinal1ac2e;
  if (outcome == 'uncertain') {
    return strings.l6NoCompletionReceiptCouldBeVerifiede00f1;
  }
  if (outcome == 'proposal_ready') {
    return strings.l6SeeTheReviewCardBelowFord5a60;
  }
  if (content is! String) return '';
  if (const {
    'list_rules',
    'list_wiki_pages',
    'get_daily_token_usage',
    'list_todos',
    'list_events_by_date',
    'record_event',
  }.contains(name)) {
    return _excerpt(strings, content, 1200);
  }
  if (name != 'get_wiki_page' && name != 'search_knowledge_base') return '';
  Object? decoded;
  try {
    decoded = jsonDecode(content);
  } catch (_) {
    return '';
  }
  if (decoded is! Map<String, dynamic>) return '';
  final lines = <String>[];
  final candidates = decoded['knowledge_candidates'];
  if (candidates is List) {
    for (final item in candidates.take(5)) {
      if (item is! Map<String, dynamic>) continue;
      lines.add(
        strings.l6EvidenceAtRetrievalArg0cb747(
          (_excerpt(strings, item['title'], 80)).toString(),
        ),
      );
      lines.add(_excerpt(strings, item['excerpt'], 400));
      final sources = item['sources'];
      if (sources is List) {
        for (final source in sources.take(3)) {
          if (source is! Map<String, dynamic>) continue;
          lines.add(
            strings.l6SourceArg0VersionArg1SnapshotArg2d0032(
              (_excerpt(strings, source['title'], 80)).toString(),
              (source['version'] is num
                      ? source['version']
                      : strings.l6Unknownd9c32)
                  .toString(),
              (_excerpt(strings, source['snapshot_id'], 80)).toString(),
            ),
          );
        }
      }
      final hash = _excerpt(strings, item['content_hash'], 80);
      if (hash.isNotEmpty) {
        lines.add(strings.l6PageHashAtRetrievalArg05808a((hash).toString()));
      }
    }
  }
  final events = decoded['events'];
  if (events is List) {
    for (final event in events.take(5)) {
      if (event is! Map<String, dynamic>) continue;
      lines.add(
        strings.l6UserRecordArg0Arg1902d4(
          (_excerpt(strings, event['event_id'], 80)).toString(),
          (_excerpt(strings, event['excerpt'], 350)).toString(),
        ),
      );
    }
  }
  final unverified = decoded['unverified_page'];
  if (unverified is Map<String, dynamic>) {
    lines.add(
      strings.l6UnverifiedPageArg01a6ef(
        (_excerpt(strings, unverified['title'], 80)).toString(),
      ),
    );
    lines.add(_excerpt(strings, unverified['notice'], 300));
    lines.add(_excerpt(strings, unverified['content_md'], 400));
  }
  if (lines.isEmpty) return strings.l6NoEvidenceExcerptsWereReturned013a1;
  lines.add(strings.l6TheseAreTheResultsReturnedBy46276);
  return _excerpt(strings, lines.join('\n'), 3000);
}
