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
  // 导入同源命中提示（负知识读面之二，提案 2026-10-19）：identity 命中
  // 已否定来源时，确认前亮出「你否定过这条来源 + 原因」。重新导入是
  // 重新纳入的唯一路径（reject → endorse 不可达），所以这里只知情
  // 不拦截——用户明确选择「仍要保存」才继续，否定不被静默绕过。
  if (preview.priorRejection != null) {
    final proceed = await showDialog<bool>(
      context: context,
      builder: (context) => PriorRejectionDialog(reason: preview.priorRejection!),
    );
    if (proceed != true || !context.mounted) return null;
  }
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

/// 导入命中负知识的知情确认：展示当时的否定原因，由用户决定是否重建。
class PriorRejectionDialog extends StatelessWidget {
  final String reason;
  const PriorRejectionDialog({super.key, required this.reason});

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('此前否定过这条来源'),
    content: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('你曾把这条来源丢弃为负知识，当时的原因：'),
        const SizedBox(height: 8),
        SelectableText(reason, style: const TextStyle(fontStyle: FontStyle.italic)),
        const SizedBox(height: 8),
        const Text('重新导入会重新入册并从孵化中重新开始；要继续吗？'),
      ],
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(false),
        child: const Text('取消'),
      ),
      FilledButton(
        onPressed: () => Navigator.of(context).pop(true),
        child: const Text('仍要保存'),
      ),
    ],
  );
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
