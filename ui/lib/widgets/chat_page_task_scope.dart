import '../l10n/app_localizations_zh.dart';

import 'dart:convert';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart';
import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';

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
        throw ChatPageTaskMaterialChanged();
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
  static String localizedLabel(BuildContext context, String kind) =>
      switch (kind) {
        'local_question' => context.l10n.taskLocalQuestion,
        'full_summary' => context.l10n.taskFullSummary,
        'compare' => context.l10n.taskCompare,
        'local_edit' => context.l10n.taskLocalEdit,
        'full_revision' => context.l10n.taskFullRevision,
        'none' => context.l10n.taskIndependent,
        _ => kind,
      };
  static final labels = {
    'local_question': AppLocalizationsZh().taskLocalQuestion,
    'full_summary': AppLocalizationsZh().taskFullSummary,
    'compare': AppLocalizationsZh().taskCompare,
    'local_edit': AppLocalizationsZh().taskLocalEdit,
    'full_revision': AppLocalizationsZh().taskFullRevision,
    'none': AppLocalizationsZh().taskIndependent,
  };
}

class ChatPageTaskMaterialChanged extends StateError {
  ChatPageTaskMaterialChanged() : super('page_task_material_changed');
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
            context.l10n.materialScope(
              ChatPageTask.localizedLabel(context, task.kind),
              task.startChar == null
                  ? ''
                  : context.l10n.materialRange(task.startChar!, task.endChar!),
            ),
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ),
        TextButton(
          onPressed: enabled ? () => _choose(context) : null,
          child: Text(context.l10n.chooseMaterial),
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
  bool selectionTooLong = false;
  bool loadFailed = false;
  bool loading = false;
  @override
  void dispose() {
    slug.dispose();
    super.dispose();
  }

  Future<void> _load(String value, {bool refresh = false}) async {
    setState(() {
      loading = true;
      selectionTooLong = false;
      loadFailed = false;
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
      if (mounted) {
        context.errorMessage(context.l10n.load, e);
        setState(() => loadFailed = true);
      }
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
      title: Text(context.l10n.taskDisplayedRevision),
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
                    (e) => DropdownMenuItem(
                      value: e.key,
                      child: Text(ChatPageTask.localizedLabel(context, e.key)),
                    ),
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
            Text(context.l10n.taskScopeHint, style: TextStyle(fontSize: 12)),
            if (main != null)
              Row(
                children: [
                  Expanded(
                    child: Text(
                      context.l10n.materialTitleCount(
                        main.title,
                        main.contentMd.runes.length,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  TextButton(
                    onPressed: loading
                        ? null
                        : () => _load(main.slug, refresh: true),
                    child: Text(context.l10n.refreshMaterialRevision),
                  ),
                ],
              ),
            if (kind == 'compare')
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: slug,
                      decoration: InputDecoration(
                        labelText: context.l10n.comparisonSlug,
                      ),
                    ),
                  ),
                  TextButton(
                    onPressed: loading || materials.length >= 4
                        ? null
                        : () => _load(slug.text),
                    child: Text(context.l10n.readAndAdd),
                  ),
                ],
              ),
            if (selectionTooLong || loadFailed)
              Text(
                selectionTooLong
                    ? context.l10n.selectionTooLong
                    : context.l10n.operationFailed(context.l10n.load),
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            if (kind == 'local_question' || kind == 'local_edit')
              Text(
                context.l10n.selectionHint(
                  start == null
                      ? context.l10n.selectionAutomatic
                      : '$start–$end',
                ),
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
                              if (!selection.isValid || selection.isCollapsed) {
                                return;
                              }
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
                                selectionTooLong = b - a > 6000;
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
          child: Text(context.l10n.cancel),
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
          child: Text(context.l10n.useDisplayedRevision),
        ),
      ],
    );
  }
}
