import 'artifact_versions.dart';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/wiki_provider.dart';

/// 原文下的独立加工成果；在此展开即可阅读完整正文，不另开页面。
class WikiDerivatives extends StatelessWidget {
  final String slug;
  const WikiDerivatives({super.key, required this.slug});
  @override
  Widget build(BuildContext context) => ArtifactVersions(slug: slug);
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
