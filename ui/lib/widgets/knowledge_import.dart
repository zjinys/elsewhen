import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/import_fetch.dart';
import '../models/wiki_page.dart';
import '../providers/knowledge_provider.dart';

/// Both import and reimport confirm the exact preview snapshot. A concurrent
/// update is rejected by Rust instead of overwriting the newly saved original.
Future<WikiPage?> saveKnowledgeImport(
  BuildContext context,
  WidgetRef ref,
  ImportFetch fetch,
  List<String> tags,
) async {
  final preview = await ref
      .read(knowledgeRepositoryProvider)
      .preview(fetch.sourceUrl, fetch.contentMd);
  if (!context.mounted) return null;
  if (preview.changed) {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => SourceUpdateDialog(
        previous: preview.previousContent ?? '',
        incoming: fetch.contentMd,
      ),
    );
    if (confirmed != true || !context.mounted) return null;
  }
  final saved = await ref
      .read(knowledgeRepositoryProvider)
      .confirmSource(
        title: fetch.displayTitle,
        contentMd: fetch.contentMd,
        sourceUrl: fetch.sourceUrl,
        sourceKind: fetch.sourceKind,
        tags: tags,
        expectedSnapshotId: preview.previousSnapshotId,
      );
  return WikiPage.fromDto(saved);
}

class SourceUpdateDialog extends StatelessWidget {
  final String previous;
  final String incoming;
  const SourceUpdateDialog({
    super.key,
    required this.previous,
    required this.incoming,
  });
  Widget _version(String title, String content) => Padding(
    padding: const EdgeInsets.all(12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: const TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 8),
        SelectableText(content),
      ],
    ),
  );
  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('来源内容有更新'),
    content: SizedBox(
      width: 900,
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('确认后保存为新版本，旧原文保留；引用它的知识页会提示复核。'),
            LayoutBuilder(
              builder: (context, constraints) => constraints.maxWidth < 600
                  ? Column(
                      children: [
                        _version('已保存版本', previous),
                        const Divider(),
                        _version('本次抓取', incoming),
                      ],
                    )
                  : Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(child: _version('已保存版本', previous)),
                        Expanded(child: _version('本次抓取', incoming)),
                      ],
                    ),
            ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context, false),
        child: const Text('保留旧版本'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, true),
        child: const Text('确认保存新版本'),
      ),
    ],
  );
}
