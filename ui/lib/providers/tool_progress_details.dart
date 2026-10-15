import 'dart:convert';

String _excerpt(Object? value, int limit) {
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
    return '含协议标记的内容未在进度中展开，请查看原始记录。';
  }
  return visible + (chars.length > limit ? '…（片段）' : '');
}

/// Project only known business-result fields, never dump arbitrary tool JSON.
/// These are historical retrieval results, not final answer citations or a fresh
/// page revision. No live lookup may attach newer identities to these excerpts.
String toolProgressDetails(String name, String outcome, Object? content) {
  if (outcome == 'failed') return '此步骤未成功；请查看最终状态和审阅回执，不会自动重复执行。';
  if (outcome == 'uncertain') return '没有可核实的完成回执；请检查实际结果，不会自动重做。';
  if (outcome == 'proposal_ready') return '具体内容、目标版本和接受/拒绝操作见下方审阅卡。';
  if (content is! String) return '';
  if (const {
    'list_rules',
    'list_wiki_pages',
    'get_daily_token_usage',
    'list_todos',
    'list_events_by_date',
    'record_event',
  }.contains(name)) {
    return _excerpt(content, 1200);
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
      lines.add('检索时的证据片段 · ${_excerpt(item['title'], 80)}');
      lines.add(_excerpt(item['excerpt'], 400));
      final sources = item['sources'];
      if (sources is List) {
        for (final source in sources.take(3)) {
          if (source is! Map<String, dynamic>) continue;
          lines.add(
            '来源：${_excerpt(source['title'], 80)} · 版本 ${source['version'] is num ? source['version'] : '未知'} · 快照 ${_excerpt(source['snapshot_id'], 80)}',
          );
        }
      }
      final hash = _excerpt(item['content_hash'], 80);
      if (hash.isNotEmpty) lines.add('检索时页面依据：$hash');
    }
  }
  final events = decoded['events'];
  if (events is List) {
    for (final event in events.take(5)) {
      if (event is! Map<String, dynamic>) continue;
      lines.add(
        '用户记录 ${_excerpt(event['event_id'], 80)}：${_excerpt(event['excerpt'], 350)}',
      );
    }
  }
  final unverified = decoded['unverified_page'];
  if (unverified is Map<String, dynamic>) {
    lines.add('未核验页面 · ${_excerpt(unverified['title'], 80)}');
    lines.add(_excerpt(unverified['notice'], 300));
    lines.add(_excerpt(unverified['content_md'], 400));
  }
  if (lines.isEmpty) return '没有返回可展开的证据片段。';
  lines.add('以上是本次查询返回的数据；来源可能随后变化，最终回答引用另行核验。');
  return _excerpt(lines.join('\n'), 3000);
}
