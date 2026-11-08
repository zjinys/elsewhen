import 'artifact_versions.dart';
import '../l10n/localization.dart';

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
    // 同 wiki_page_detail_view：知识号 bump 触发的 reload 不重回 loading。
    return source.when(
      skipLoadingOnReload: true,
      loading: () => Text(context.l10n.wikiSourceLoading),
      error: (_, _) => TextButton(
        onPressed: () => ref.invalidate(wikiPageProvider(slug)),
        child: Text(context.l10n.wikiSourceRetry),
      ),
      data: (page) => page == null
          ? Text(context.l10n.wikiSourceUnavailable)
          : TextButton.icon(
              onPressed: () => openWikiPageTab(ref, page),
              icon: const Icon(Icons.subdirectory_arrow_left),
              label: Text(context.l10n.wikiBasedOn(page.title)),
            ),
    );
  }
}
