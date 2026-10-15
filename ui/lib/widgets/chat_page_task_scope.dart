import 'dart:convert';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart';
import '../bridge/rust_bridge_repository.dart';

class ChatPageTask {
  final String kind;
  final List<WikiChatMaterialDto> materials;
  final int? startChar;
  final int? endChar;
  const ChatPageTask(this.kind, this.materials, {this.startChar, this.endChar});

  static Future<ChatPageTask> restore(
    String taskJson,
    Future<WikiChatMaterialDto> Function(String) loadMaterial,
  ) async {
    final saved = jsonDecode(taskJson) as Map<String, dynamic>;
    final materials = <WikiChatMaterialDto>[];
    for (final basis in saved['pages'] as List) {
      final material = await loadMaterial(basis['slug'] as String);
      if (!const DeepCollectionEquality().equals(
        jsonDecode(material.basisJson),
        basis,
      )) {
        throw StateError('原任务材料已变化，请明确重新选择任务与材料；不会自动沿用新版本。');
      }
      materials.add(material);
    }
    return ChatPageTask(
      saved['kind'] as String,
      materials,
      startChar: saved['start_char'] as int?,
      endChar: saved['end_char'] as int?,
    );
  }

  String get json => jsonEncode({
    'kind': kind,
    'pages': kind == 'none'
        ? []
        : materials.map((m) => jsonDecode(m.basisJson)).toList(),
    'start_char': startChar,
    'end_char': endChar,
  });

  String get label => labels[kind] ?? kind;
  static const labels = {
    'local_question': '局部问答',
    'full_summary': '全文总结',
    'compare': '跨来源对照',
    'local_edit': '局部编辑',
    'full_revision': '全页修订',
    'none': '独立新问题',
  };
}

/// Selection and revision tokens come from the same displayed material snapshot.
class ChatPageTaskScope extends StatelessWidget {
  final RustBridgeRepository repository;
  final ChatPageTask task;
  final ValueChanged<ChatPageTask> onChanged;
  final bool enabled;
  const ChatPageTaskScope({
    super.key,
    required this.repository,
    required this.task,
    required this.onChanged,
    this.enabled = true,
  });

  Future<void> _choose(BuildContext context) async {
    final result = await showDialog<ChatPageTask>(
      context: context,
      builder: (_) => _TaskDialog(repository: repository, initial: task),
    );
    if (result != null) onChanged(result);
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
    child: Row(
      children: [
        Expanded(
          child: Text(
            '材料范围：${task.label}${task.startChar == null ? '' : ' · ${task.startChar}–${task.endChar}字'}',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ),
        TextButton(
          onPressed: enabled ? () => _choose(context) : null,
          child: const Text('查看 / 选择材料'),
        ),
      ],
    ),
  );
}

class _TaskDialog extends StatefulWidget {
  final RustBridgeRepository repository;
  final ChatPageTask initial;
  const _TaskDialog({required this.repository, required this.initial});
  @override
  State<_TaskDialog> createState() => _TaskDialogState();
}

class _TaskDialogState extends State<_TaskDialog> {
  late String kind = widget.initial.kind;
  late List<WikiChatMaterialDto> materials = List.of(widget.initial.materials);
  late int? start = widget.initial.startChar;
  late int? end = widget.initial.endChar;
  final slug = TextEditingController();
  String? error;
  bool loading = false;
  @override
  void dispose() {
    slug.dispose();
    super.dispose();
  }

  Future<void> _load(String value, {bool refresh = false}) async {
    setState(() {
      loading = true;
      error = null;
    });
    try {
      final material = await widget.repository.getWikiChatMaterial(
        value.trim(),
      );
      if (!mounted) return;
      setState(() {
        if (refresh) {
          materials = [material];
          start = null;
          end = null;
        } else if (!materials.any((m) => m.slug == material.slug)) {
          materials.add(material);
        }
      });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final main = materials.firstOrNull;
    final valid =
        !loading &&
        (kind == 'none' || main != null) &&
        (kind != 'compare' || materials.length >= 2) &&
        (kind != 'local_edit' || start != null && end != null && end! > start!);
    return AlertDialog(
      title: const Text('任务与显示版本'),
      content: SizedBox(
        width: 640,
        height: 460,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DropdownButton<String>(
              value: kind,
              isExpanded: true,
              items: ChatPageTask.labels.entries
                  .map(
                    (e) => DropdownMenuItem(value: e.key, child: Text(e.value)),
                  )
                  .toList(),
              onChanged: loading
                  ? null
                  : (value) => setState(() {
                      kind = value!;
                      start = null;
                      end = null;
                    }),
            ),
            const Text(
              '后续问题沿用这里的任务和版本；新任务请重新选择。全页修订按连续分块处理，完成后仍需审阅。',
              style: TextStyle(fontSize: 12),
            ),
            if (main != null)
              Row(
                children: [
                  Expanded(
                    child: Text(
                      '${main.title} · ${main.contentMd.runes.length} 字',
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  TextButton(
                    onPressed: loading
                        ? null
                        : () => _load(main.slug, refresh: true),
                    child: const Text('明确刷新版本'),
                  ),
                ],
              ),
            if (kind == 'compare')
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: slug,
                      decoration: const InputDecoration(labelText: '对照材料 slug'),
                    ),
                  ),
                  TextButton(
                    onPressed: loading || materials.length >= 4
                        ? null
                        : () => _load(slug.text),
                    child: const Text('读取并加入'),
                  ),
                ],
              ),
            if (error != null)
              Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            if (kind == 'local_question' || kind == 'local_edit')
              Text(
                '在下方原文选择要处理的文字（最多6000字）。当前：${start == null ? '未选区；问答将自动选取相关片段' : '$start–$end'}',
                style: const TextStyle(fontSize: 12),
              ),
            const Divider(),
            Expanded(
              child: ListView(
                children: [
                  for (final material in materials) ...[
                    Text(
                      material.title,
                      style: const TextStyle(fontWeight: FontWeight.bold),
                    ),
                    SelectableText(
                      material.contentMd,
                      onSelectionChanged:
                          material != main ||
                              !{'local_question', 'local_edit'}.contains(kind)
                          ? null
                          : (selection, _) {
                              if (!selection.isValid || selection.isCollapsed)
                                return;
                              final a = material.contentMd
                                  .substring(0, selection.start)
                                  .runes
                                  .length;
                              final b = material.contentMd
                                  .substring(0, selection.end)
                                  .runes
                                  .length;
                              setState(() {
                                start = a;
                                end = b;
                                error = b - a > 6000 ? '选区超过6000字，请缩小' : null;
                              });
                            },
                    ),
                    const Divider(),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: valid && (start == null || end! - start! <= 6000)
              ? () => Navigator.pop(
                  context,
                  ChatPageTask(
                    kind,
                    kind == 'compare' ? materials : materials.take(1).toList(),
                    startChar: kind == 'local_question' || kind == 'local_edit'
                        ? start
                        : null,
                    endChar: kind == 'local_question' || kind == 'local_edit'
                        ? end
                        : null,
                  ),
                )
              : null,
          child: const Text('使用显示的版本'),
        ),
      ],
    );
  }
}
