part of '../wiki_page_detail_view.dart';

//! 库浏览 tab 与沉浸内容容器。

class _WikiBrowseTab extends StatelessWidget {
  const _WikiBrowseTab();
  @override
  Widget build(BuildContext context) => const KnowledgeLibrary();
}

/// 沉浸阅读专用内容渲染：在 WikiContentEditor 基础上覆盖文字色为
/// textSecondary（降低对比度疲劳），并注入 ContentFontScope 保证字体链完整。
class _ImmersiveContent extends ConsumerWidget {
  const _ImmersiveContent({
    required this.slug,
    required this.contentMd,
    required this.onWikiLinkTap,
    required this.onSave,
    required this.onSaveError,
    required this.onDirtyChanged,
  });

  final String slug;
  final String contentMd;
  final void Function(String slug) onWikiLinkTap;
  final Future<void> Function(String markdown) onSave;
  final void Function(Object error) onSaveError;
  final void Function(bool dirty) onDirtyChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final fontFamily = ref.watch(
      settingsProvider.select((s) => s.contentFontName),
    );
    final fontSize = ref.watch(
      settingsProvider.select((s) => s.contentFontSize),
    );
    final lineHeight = ref.watch(
      settingsProvider.select((s) => s.contentLineHeight),
    );
    return WikiContentEditor(
      slug: slug,
      contentMd: contentMd,
      editable: false,
      onWikiLinkTap: onWikiLinkTap,
      onSave: onSave,
      onSaveError: onSaveError,
      onDirtyChanged: onDirtyChanged,
      fontFamily: fontFamily,
      fontSize: fontSize,
      lineHeight: lineHeight,
      // 沉浸阅读：文字色 softened（textSecondary 代替 textPrimary），
      // 降低长文阅读的对比度疲劳，保留足够可读性。
      textColor: AppTheme.textSecondary,
    );
  }
}



/// 相关页面行（知识关系网 P3，提案 2026-10-19）。
///
/// 数据来自知识详情的派生读（details.relations）；人工边可删，「+」从页列表
/// 拉取候选；衍生边（tags 命中标题）不在此删——去改 tags 让事实消失。
/// 独立成公开组件以便单测（详情页头部与 widget 测试共用）。
class RelatedPagesRow extends ConsumerStatefulWidget {
  const RelatedPagesRow({super.key, required this.slug});

  final String slug;

  @override
  ConsumerState<RelatedPagesRow> createState() => _RelatedPagesRowState();
}

class _RelatedPagesRowState extends ConsumerState<RelatedPagesRow> {
  @override
  Widget build(BuildContext context) {
    final detailsAsync = ref.watch(knowledgeDetailsProvider(widget.slug));
    final relations = detailsAsync.value?.relations ?? const [];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              '相关页面',
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w600,
                color: AppTheme.textTertiary,
                letterSpacing: 0.5,
              ),
            ),
            const SizedBox(width: 4),
            InkWell(
              onTap: () => _showAddRelatedDialog(context),
              borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              child: Padding(
                padding: const EdgeInsets.all(2),
                child: Icon(
                  Icons.add,
                  size: 14,
                  color: AppTheme.textTertiary,
                ),
              ),
            ),
          ],
        ),
        if (relations.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space2),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final r in relations)
                RelatedPageChip(
                  relation: r,
                  pageSlug: widget.slug,
                  onOpen: (slug, title) =>
                      openWikiTab(ref, PageTabEntry(slug: slug, title: title)),
                  onRemove: r.origin == 'human'
                      ? () => _removeRelatedPage(context, r.id)
                      : null,
                ),
            ],
          ),
        ],
      ],
    );
  }

  Future<void> _removeRelatedPage(BuildContext context, String relationId) async {
    try {
      final repo = ref.read(knowledgeRepositoryProvider);
      await repo.removeRelatedPage(widget.slug, relationId);
      ref.invalidate(knowledgeDetailsProvider(widget.slug));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('移除失败：${e.toString().replaceFirst('Exception: ', '')}'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }

  Future<void> _showAddRelatedDialog(BuildContext context) async {
    final details = ref.read(knowledgeDetailsProvider(widget.slug)).value;
    final existing = details?.relations.map((r) => r.otherSlug).toSet() ?? {};
    final pages = await ref.read(wikiPagesProvider.future);
    if (!context.mounted) return;
    final candidates = pages
        .where((p) =>
            p.slug != widget.slug &&
            p.status != 'archived' &&
            !existing.contains(p.slug))
        .toList();
    final picked = await showDialog<WikiPageMeta>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('添加相关页面'),
        content: SizedBox(
          width: 360,
          child: candidates.isEmpty
              ? const Padding(
                  padding: EdgeInsets.all(12),
                  child: Text('暂时没有可选的页面'),
                )
              : ListView.builder(
                  shrinkWrap: true,
                  itemCount: candidates.length,
                  itemBuilder: (context, index) => ListTile(
                    dense: true,
                    title: Text(
                      candidates[index].title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(
                      candidates[index].slug,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    onTap: () => Navigator.of(context).pop(candidates[index]),
                  ),
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('取消'),
          ),
        ],
      ),
    );
    if (picked == null || !context.mounted) return;
    try {
      final repo = ref.read(knowledgeRepositoryProvider);
      await repo.addRelatedPage(widget.slug, picked.slug);
      ref.invalidate(knowledgeDetailsProvider(widget.slug));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            '关联失败：${e.toString().replaceFirst('Exception: ', '')}',
          ),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }
}

/// 内容对话的气泡（用户右对齐高亮，AI 左侧带头像）
