import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'glass.dart';
import 'markdown_view.dart';
import '../l10n/localization.dart';

/// 知识库单页内容快速预览对话框。
///
/// 允许用户在不离开当前工作上下文（如主会话、Wiki 会话、洞察回顾）的前提下，
/// 直接点击消息或正文中的知识页引用（slug 或 wikilink）弹窗查阅。
/// 弹窗内支持完整的 Markdown 渲染与文本复制，并提供「在知识库中打开」的直达出口。
class KnowledgePageDialog extends ConsumerWidget {
  /// 目标页面 slug（或规范化前的引用标识符）
  final String slug;

  /// 加载或未找到时的备选标题提示
  final String? fallbackTitle;

  const KnowledgePageDialog({
    super.key,
    required this.slug,
    this.fallbackTitle,
  });

  /// 弹出知识页预览对话框
  static Future<void> show(
    BuildContext context, {
    required String slug,
    String? fallbackTitle,
  }) {
    return showDialog<void>(
      context: context,
      builder: (_) =>
          KnowledgePageDialog(slug: slug, fallbackTitle: fallbackTitle),
    );
  }

  /// 规范化 slug，剥离可能的 namespace 前缀（如 `kb:`、`topic:`、`wiki:`）
  static String normalizeSlug(String raw) {
    return raw.trim().replaceFirst(RegExp(r'^(?:kb|topic|wiki):'), '');
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final cleanSlug = normalizeSlug(slug);
    final pageAsync = ref.watch(wikiPageProvider(cleanSlug));

    return GlassDialog(
      maxWidth: 720,
      title: _buildTitle(context, cleanSlug, pageAsync.value),
      content: _buildContent(context, ref, cleanSlug, pageAsync),
      actions: _buildActions(context, ref, pageAsync.value),
    );
  }

  Widget _buildTitle(BuildContext context, String cleanSlug, WikiPage? page) {
    if (page == null) {
      return Text(
        fallbackTitle ?? cleanSlug,
        style: TextStyle(
          fontSize: 16,
          fontWeight: FontWeight.w600,
          color: AppTheme.textPrimary,
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          children: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                border: Border.all(
                  color: AppTheme.accentPrimary.withValues(alpha: 0.3),
                  width: 1,
                ),
              ),
              child: Text(
                context.pageKindLabel(page.kind),
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.accentPrimary,
                ),
              ),
            ),
            const SizedBox(width: AppTheme.space2),
            Expanded(
              child: Text(
                page.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 17,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
            ),
            Tooltip(
              message: context.l10n.copyPageTitle,
              child: IconButton(
                icon: const Icon(Icons.copy_rounded, size: 14),
                color: AppTheme.textTertiary,
                visualDensity: VisualDensity.compact,
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: page.title));
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text(context.l10n.pageTitleCopied),
                      behavior: SnackBarBehavior.floating,
                    ),
                  );
                },
              ),
            ),
          ],
        ),
        const SizedBox(height: 2),
        Text(
          cleanSlug,
          style: TextStyle(
            fontSize: 11,
            color: AppTheme.textTertiary,
            fontFamily: 'monospace',
          ),
        ),
      ],
    );
  }

  Widget _buildContent(
    BuildContext context,
    WidgetRef ref,
    String cleanSlug,
    AsyncValue<WikiPage?> pageAsync,
  ) {
    return pageAsync.when(
      loading: () => const Padding(
        padding: EdgeInsets.symmetric(vertical: 40),
        child: Center(
          child: SizedBox(
            width: 28,
            height: 28,
            child: CircularProgressIndicator(strokeWidth: 2.5),
          ),
        ),
      ),
      error: (err, _) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.error_outline_rounded,
              color: AppTheme.warning,
              size: 36,
            ),
            const SizedBox(height: AppTheme.space2),
            Text(
              context.l10n.readPageFailed,
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w600,
                color: AppTheme.textPrimary,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              err.toString(),
              style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
      data: (page) {
        if (page == null) {
          return _buildNotFound(context, cleanSlug);
        }

        final content = page.contentMd.trim();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (page.tags.isNotEmpty) ...[
              Wrap(
                spacing: 6,
                runSpacing: 4,
                children: [
                  for (final tag in page.tags)
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 1.5,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.surface2,
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: AppTheme.surface3),
                      ),
                      child: Text(
                        '#$tag',
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                    ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
            ],
            SelectionArea(
              child: MarkdownView(
                markdown: content.isEmpty
                    ? context.l10n.pageBodyEmpty
                    : content,
                baseStyle: TextStyle(
                  fontSize: 14,
                  color: AppTheme.textPrimary,
                  height: 1.7,
                ),
              ),
            ),
          ],
        );
      },
    );
  }

  Widget _buildNotFound(BuildContext context, String cleanSlug) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 24),
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.find_in_page_outlined,
              size: 40,
              color: AppTheme.textTertiary,
            ),
            const SizedBox(height: AppTheme.space3),
            Text(
              context.l10n.knowledgePageMissing,
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w600,
                color: AppTheme.textPrimary,
              ),
            ),
            const SizedBox(height: AppTheme.space1),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
              ),
              child: Text(
                cleanSlug,
                style: TextStyle(
                  fontSize: 12,
                  fontFamily: 'monospace',
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
            const SizedBox(height: AppTheme.space3),
            Text(
              context.l10n.knowledgePageMissingHelp,
              style: TextStyle(
                fontSize: 12.5,
                color: AppTheme.textTertiary,
                height: 1.5,
              ),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }

  List<Widget> _buildActions(
    BuildContext context,
    WidgetRef ref,
    WikiPage? page,
  ) {
    return [
      if (page != null)
        TextButton.icon(
          icon: const Icon(Icons.open_in_new_rounded, size: 15),
          label: Text(context.l10n.openInKnowledgeBase),
          style: TextButton.styleFrom(
            foregroundColor: AppTheme.accentPrimary,
            visualDensity: VisualDensity.compact,
          ),
          onPressed: () {
            openWikiPageTab(ref, page);
            ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
            Navigator.of(context).pop();
          },
        ),
      FilledButton(
        style: FilledButton.styleFrom(
          visualDensity: VisualDensity.compact,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        ),
        onPressed: () => Navigator.of(context).pop(),
        child: Text(context.l10n.close),
      ),
    ];
  }
}

/// 全局便捷入口：直接根据 slug 弹出知识页预览窗口
Future<void> showKnowledgePageDialog(
  BuildContext context, {
  required String slug,
  String? fallbackTitle,
}) {
  return KnowledgePageDialog.show(
    context,
    slug: slug,
    fallbackTitle: fallbackTitle,
  );
}
