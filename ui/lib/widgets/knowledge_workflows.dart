import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';
import '../l10n/localization.dart';
import 'app_error_notice.dart';

part 'knowledge_workflows_parts/knowledge_library.dart';

// ── 知识工作流按阅读控制/库列表拆分（纯类段平移，零行为变化）──

Map<String, String> readingLabels(BuildContext context) => {
  'unread': context.l10n.readingUnread,
  'read': context.l10n.readingRead,
  'valuable': context.l10n.readingValuable,
  'adopted': context.l10n.readingAdopted,
  'archived': context.l10n.readingArchived,
};
String libraryKindLabel(BuildContext context, String kind) => switch (kind) {
  'source' => context.l10n.librarySourceKind,
  'note' => context.l10n.pageKindNote,
  _ => context.pageKindLabel(kind),
};

class KnowledgeReadingControl extends ConsumerStatefulWidget {
  final String slug;
  const KnowledgeReadingControl({super.key, required this.slug});
  @override
  ConsumerState<KnowledgeReadingControl> createState() => _ReadingState();
}

class _ReadingState extends ConsumerState<KnowledgeReadingControl> {
  String? _value;
  Object? _error;
  bool _busy = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final value = await ref
          .read(knowledgeRepositoryProvider)
          .reading(widget.slug);
      if (mounted) setState(() => _value = value);
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _save(String value) async {
    setState(() => _busy = true);
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .setReading(widget.slug, value);
      if (mounted) {
        setState(() {
          _value = value;
          _error = null;
        });
        notifyKnowledgeChanged(ref);
      }
    } catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
    decoration: BoxDecoration(
      color: AppTheme.surface1,
      borderRadius: BorderRadius.circular(10),
      border: Border.all(color: AppTheme.surface3),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              Icons.bookmark_outline,
              size: 16,
              color: AppTheme.accentPrimary,
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                context.l10n.knowledgeReadingTitle,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
            ),
            if (_busy) ...[
              const SizedBox(
                width: 14,
                height: 14,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
              const SizedBox(width: 8),
            ],
            if (_value != null)
              Container(
                height: 32,
                padding: const EdgeInsets.symmetric(horizontal: 10),
                decoration: BoxDecoration(
                  color: AppTheme.surface2,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: AppTheme.surface3),
                ),
                child: DropdownButtonHideUnderline(
                  child: DropdownButton<String>(
                    value: _value,
                    isDense: true,
                    icon: Icon(
                      Icons.keyboard_arrow_down_rounded,
                      size: 18,
                      color: AppTheme.textSecondary,
                    ),
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: FontWeight.w500,
                      color: AppTheme.textPrimary,
                    ),
                    dropdownColor: AppTheme.surface2,
                    items: readingLabels(context).entries
                        .map(
                          (e) => DropdownMenuItem(
                            value: e.key,
                            child: Text(e.value),
                          ),
                        )
                        .toList(),
                    onChanged: _busy
                        ? null
                        : (v) {
                            if (v != null) _save(v);
                          },
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: 6),
        Text(
          context.l10n.knowledgeReadingHelp,
          style: TextStyle(
            fontSize: 11.5,
            color: AppTheme.textTertiary,
            height: 1.3,
          ),
        ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: AppErrorMessage(
              action: _value == null ? context.l10n.load : context.l10n.save,
              error: _error!,
            ),
          ),
      ],
    ),
  );
}

/// Database pagination; no original bodies cross the bridge for a list.
