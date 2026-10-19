//! 翻篇（P2 手动 / P3 自动档）预览编辑与撤销。
//!
//! 流程：选边界消息 → createPageMark（AI 生成章节摘要，真实往返）
//! → 预览编辑（SummaryBody 五组条目，仍过摘要校验）→ 固化。
//! 撤销只把标记置 inactive：原文回到装配，已写 wiki 不回滚。
//! auto 标记由生成链路预算触发线自动产生，此处只展示与撤销。

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../bridge/api.dart' as bridge;
import '../theme/app_theme.dart';
import '../utils/error_report.dart';

const _groupLabels = {
  'goals': '目标',
  'constraints': '约束',
  'facts': '事实',
  'decisions': '决定',
  'unfinished': '未完成',
};

/// 选边界消息后的入口：生成 + 预览编辑一条龙。
Future<void> openPageMarkCreateDialog(
  BuildContext context,
  WidgetRef ref,
  String conversationId,
  String boundaryMessageId,
) {
  return showDialog<void>(
    context: context,
    builder: (_) => _PageMarkCreateDialog(
      conversationId: conversationId,
      boundaryMessageId: boundaryMessageId,
    ),
  );
}

/// 已有翻篇的查看 / 撤销列表。
Future<void> openPageMarkListDialog(
  BuildContext context,
  WidgetRef ref,
  String conversationId,
) {
  return showDialog<void>(
    context: context,
    builder: (_) => _PageMarkListDialog(conversationId: conversationId),
  );
}

/// SummaryBody JSON 的宽松解析（编辑卡没有校验外设，错误留给保存时的
/// Rust 端 SummaryBody::validate——那里的信息如「条目超长」更能定位）。
Map<String, List<String>> parseSummaryBody(String bodyJson) {
  final decoded = jsonDecode(bodyJson);
  if (decoded is! Map<String, dynamic>) return {};
  final result = <String, List<String>>{};
  for (final key in _groupLabels.keys) {
    final value = decoded[key];
    result[key] =
        (value as List<dynamic>?)?.map((e) => e.toString()).toList() ??
        <String>[];
  }
  return result;
}


/// 消息工具条上的「翻篇到此」按钮（桌面常驻；移动端走长按菜单）。
class PageMarkToolButton extends ConsumerWidget {
  const PageMarkToolButton({
    super.key,
    required this.conversationId,
    required this.boundaryMessageId,
  });

  final String conversationId;
  final String boundaryMessageId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return IconButton(
      icon: const Icon(Icons.auto_stories, size: 13),
      tooltip: '翻篇到此——此前历史收进章节摘要',
      visualDensity: VisualDensity.compact,
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints(minWidth: 24, minHeight: 20),
      onPressed: () =>
          openPageMarkCreateDialog(context, ref, conversationId, boundaryMessageId),
    );
  }
}

/// 会话标题栏的「翻篇记录」入口：查看全部标记（含 auto）与撤销/恢复。
class PageMarkListButton extends ConsumerWidget {
  const PageMarkListButton({super.key, required this.conversationId});

  final String conversationId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return IconButton(
      icon: const Icon(Icons.auto_stories, size: 18),
      tooltip: '翻篇记录',
      onPressed: () => openPageMarkListDialog(context, ref, conversationId),
    );
  }
}

class _PageMarkCreateDialog extends ConsumerStatefulWidget {
  const _PageMarkCreateDialog({
    required this.conversationId,
    required this.boundaryMessageId,
  });

  final String conversationId;
  final String boundaryMessageId;

  @override
  ConsumerState<_PageMarkCreateDialog> createState() =>
      _PageMarkCreateDialogState();
}

class _PageMarkCreateDialogState extends ConsumerState<_PageMarkCreateDialog> {
  Map<String, List<String>>? body;
  String summaryNodeId = '';
  String? error;
  bool generating = true;
  bool saving = false;

  @override
  void initState() {
    super.initState();
    _generate();
  }

  Future<void> _generate() async {
    setState(() {
      generating = true;
      error = null;
    });
    try {
      final markId = await bridge.createPageMark(
        conversationId: widget.conversationId,
        boundaryMessageId: widget.boundaryMessageId,
      );
      // 从列表里取回刚建的标记拿编辑数据面。
      final marks = await bridge.listPageMarks(
        conversationId: widget.conversationId,
      );
      final mark = marks.firstWhere((m) => m.id == markId);
      if (!mounted) return;
      setState(() {
        body = parseSummaryBody(mark.bodyJson);
        summaryNodeId = mark.summaryNodeId;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        error = _readableError(e);
      });
    } finally {
      if (mounted) setState(() => generating = false);
    }
  }

  String _readableError(Object e) => stripErrorShell(e);

  Future<void> _saveOverride() async {
    final current = body;
    if (current == null) return;
    setState(() => saving = true);
    try {
      // 通过重新生的检索拿 id：create 返回的 id 在生成失败重试时才更新。
      final marks = await bridge.listPageMarks(
        conversationId: widget.conversationId,
      );
      final mark = marks
          .where((m) => m.summaryNodeId == summaryNodeId && m.active)
          .toList();
      if (mark.isEmpty) {
        throw Exception('翻篇标记已不存在，可能已被撤销');
      }
      final bodyJson = jsonEncode({
        'goals': current['goals'] ?? <String>[],
        'constraints': current['constraints'] ?? <String>[],
        'facts': current['facts'] ?? <String>[],
        'decisions': current['decisions'] ?? <String>[],
        'unfinished': current['unfinished'] ?? <String>[],
      });
      await bridge.savePageMarkOverride(
        id: mark.first.id,
        bodyJson: bodyJson,
      );
    } catch (e) {
      if (mounted) {
        setState(() => error = _readableError(e));
      }
    } finally {
      if (mounted) setState(() => saving = false);
    }
    if (mounted && error == null) {
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 640, maxHeight: 620),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.all(AppTheme.space3),
              child: Row(
                children: [
                  Icon(Icons.auto_stories, color: AppTheme.accentPrimary),
                  const SizedBox(width: AppTheme.space2),
                  const Expanded(
                    child: Text(
                      '翻篇 · 章节摘要',
                      style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: generating
                  ? const Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          CircularProgressIndicator(),
                          SizedBox(height: AppTheme.space3),
                          Text('正在生成章节摘要…'),
                        ],
                      ),
                    )
                  : error != null && body == null
                  ? Center(
                      child: Padding(
                        padding: const EdgeInsets.all(AppTheme.space3),
                        child: Text(error!, style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary)),
                      ),
                    )
                  : _BodyEditor(
                      body: body!,
                      onChanged: (next) => setState(() => body = next),
                    ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.all(AppTheme.space3),
              child: Row(
                children: [
                  if (error != null && body != null)
                    Expanded(
                      child: Text(
                        error!,
                        style: TextStyle(color: AppTheme.error, fontSize: 12),
                      ),
                    ),
                  if (error == null) const Spacer(),
                  TextButton(
                    onPressed:
                        saving || generating || body == null
                            ? null
                            : () => Navigator.of(context).pop(),
                    child: const Text('放弃修改'),
                  ),
                  const SizedBox(width: AppTheme.space2),
                  FilledButton(
                    onPressed: saving || generating || body == null
                        ? null
                        : _saveOverride,
                    child: Text(saving ? '保存中…' : '确认翻篇'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _BodyEditor extends StatelessWidget {
  const _BodyEditor({required this.body, required this.onChanged});

  final Map<String, List<String>> body;
  final ValueChanged<Map<String, List<String>>> onChanged;

  void _editEntry(String group, int index, String value) {
    final next = <String, List<String>>{
      for (final entry in body.entries) entry.key: List.of(entry.value),
    };
    next[group]![index] = value;
    onChanged(next);
  }

  void _removeEntry(String group, int index) {
    final next = <String, List<String>>{
      for (final entry in body.entries) entry.key: List.of(entry.value),
    };
    next[group]!.removeAt(index);
    onChanged(next);
  }

  void _addEntry(String group) {
    final next = <String, List<String>>{
      for (final entry in body.entries) entry.key: List.of(entry.value),
    };
    next[group]!.add('');
    onChanged(next);
  }

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(AppTheme.space3),
      children: [
        for (final entry in _groupLabels.entries)
          if ((body[entry.key] ?? const []).isNotEmpty) ...[
            Padding(
              padding: const EdgeInsets.only(
                top: AppTheme.space2,
                bottom: AppTheme.space1,
              ),
              child: Text(
                entry.value,
                style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary),
              ),
            ),
            for (var i = 0; i < (body[entry.key] ?? const []).length; i++)
              Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Expanded(
                    child: TextFormField(
                      initialValue: body[entry.key]![i],
                      maxLines: 2,
                      style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary),
                      onChanged: (value) => _editEntry(entry.key, i, value),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.remove_circle_outline, size: 18),
                    tooltip: '删除这一条',
                    color: AppTheme.textTertiary,
                    onPressed: () => _removeEntry(entry.key, i),
                  ),
                ],
              ),
          ],
        // 补充入口：空组也能加条目。
        Wrap(
          spacing: AppTheme.space2,
          children: [
            for (final entry in _groupLabels.entries)
              if ((body[entry.key] ?? const []).isEmpty)
                ActionChip(
                  label: Text('加一条${entry.value}'),
                  onPressed: () => _addEntry(entry.key),
                ),
          ],
        ),
        const SizedBox(height: AppTheme.space2),
        Text(
          '翻篇后这段历史不再原文进入请求，仅以本摘要常驻；需要精确数值或原话时 '
          'AI 会回读原始消息。可编辑或删减条目后确认。',
          style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
        ),
      ],
    );
  }
}

class _PageMarkListDialog extends ConsumerStatefulWidget {
  const _PageMarkListDialog({required this.conversationId});

  final String conversationId;

  @override
  ConsumerState<_PageMarkListDialog> createState() =>
      _PageMarkListDialogState();
}

class _PageMarkListDialogState extends ConsumerState<_PageMarkListDialog> {
  List<bridge.PageMarkDto>? marks;
  String? error;

  bool _busy = true;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() => _busy = true);
    try {
      final list = await bridge.listPageMarks(
        conversationId: widget.conversationId,
      );
      if (!mounted) return;
      setState(() {
        marks = list;
        error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _toggle(bridge.PageMarkDto mark) async {
    await bridge.setPageMarkActive(id: mark.id, active: !mark.active);
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 620, maxHeight: 560),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.all(AppTheme.space3),
              child: Row(
                children: [
                  Icon(
                    Icons.auto_stories,
                    color: AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: AppTheme.space2),
                  Expanded(
                    child: const Text(
                      '翻篇记录',
                      style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: _busy
                  ? const Center(child: CircularProgressIndicator())
                  : error != null
                  ? Center(child: Text(error!))
                  : (marks ?? const []).isEmpty
                  ? const Center(child: Text('本会话还没有翻篇记录'))
                  : ListView(
                      padding: const EdgeInsets.all(AppTheme.space3),
                      children: [
                        for (final mark in marks!)
                          Card(
                            margin: const EdgeInsets.only(
                              bottom: AppTheme.space2,
                            ),
                            child: ListTile(
                              leading: Icon(
                                mark.active
                                    ? Icons.auto_stories
                                    : Icons.auto_stories_outlined,
                                color: mark.active
                                    ? AppTheme.accentPrimary
                                    : AppTheme.textTertiary,
                              ),
                              title: Text(
                                '${mark.kind == "manual" ? "手动" : "自动"} · '
                                '${DateFormat('yyyy-MM-dd HH:mm').format(DateTime.parse(mark.createdAt))}',
                                style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary),
                              ),
                              subtitle: Text(
                                mark.summaryPreview,
                                maxLines: 3,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                              ),
                              trailing: TextButton(
                                onPressed: () => _toggle(mark),
                                child: Text(mark.active ? '撤销' : '恢复'),
                              ),
                            ),
                          ),
                      ],
                    ),
            ),
          ],
        ),
      ),
    );
  }
}
