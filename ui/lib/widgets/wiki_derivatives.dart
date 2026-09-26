import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/wiki_provider.dart';
import '../models/wiki_page.dart';
import 'markdown_view.dart';

/// 原文下的独立加工成果；在此展开即可阅读完整正文，不另开页面。
class WikiDerivatives extends ConsumerWidget {
  final String slug;
  const WikiDerivatives({super.key, required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pages = ref.watch(wikiDerivativesProvider(slug));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const Expanded(child: Text('派生产物')),
            IconButton(
              tooltip: '刷新派生产物',
              onPressed: () => ref.invalidate(wikiDerivativesProvider(slug)),
              icon: const Icon(Icons.refresh, size: 18),
            ),
          ],
        ),
        pages.when(
          loading: () => const LinearProgressIndicator(),
          error: (error, _) => const Text('加载失败，请点击刷新重试'),
          data: (items) => items.isEmpty
              ? const Text('暂无产物。让 AI 加工本页，确认保存后将在这里展示；原文不变。')
              : Column(
                  children: [
                    for (final page in items) _DerivativeTile(page: page),
                  ],
                ),
        ),
      ],
    );
  }
}

class WikiSourceLink extends ConsumerWidget {
  final String slug;
  const WikiSourceLink({super.key, required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final source = ref.watch(wikiPageProvider(slug));
    return source.when(
      loading: () => const Text('正在读取来源…'),
      error: (_, _) => TextButton(
        onPressed: () => ref.invalidate(wikiPageProvider(slug)),
        child: const Text('来源读取失败，点击重试'),
      ),
      data: (page) => page == null
          ? const Text('来源页已不可用')
          : TextButton.icon(
              onPressed: () => openWikiPageTab(ref, page),
              icon: const Icon(Icons.subdirectory_arrow_left),
              label: Text('基于：${page.title}'),
            ),
    );
  }
}

/// 单个派生产物卡片：展开即读完整正文，支持一键复制。
class _DerivativeTile extends StatelessWidget {
  final WikiPage page;
  const _DerivativeTile({required this.page});

  String get _timeLabel {
    final t = page.createdAt;
    String two(int n) => n.toString().padLeft(2, '0');
    return '${t.year}-${two(t.month)}-${two(t.day)} ${two(t.hour)}:${two(t.minute)}';
  }

  @override
  Widget build(BuildContext context) {
    return ExpansionTile(
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: 8),
      title: Text(
        page.title,
        style: const TextStyle(fontWeight: FontWeight.w600),
      ),
      subtitle: Text('${page.contentType ?? 'AI 加工'} · $_timeLabel'),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: MarkdownView(markdown: page.contentMd),
        ),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton.icon(
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: page.contentMd));
              if (context.mounted) {
                ScaffoldMessenger.of(context)
                    .showSnackBar(const SnackBar(content: Text('已复制完整正文')));
              }
            },
            icon: const Icon(Icons.copy_outlined, size: 16),
            label: const Text('复制全文'),
          ),
        ),
      ],
    );
  }
}
