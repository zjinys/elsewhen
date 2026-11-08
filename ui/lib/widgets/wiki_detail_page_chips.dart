part of 'wiki_page_detail_view.dart';

/// 本地路径 chip：目录导入的项目页（sourceUrl 为 file://）。
/// 显示解码后路径，点击复制；铅笔按钮弹出改路径对话框（目录搬家后纠正）。
class _LocalPathChip extends ConsumerWidget {
  final WikiPage page;

  const _LocalPathChip({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final path = page.localPath ?? page.sourceUrl!;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        InkWell(
          onTap: () {
            Clipboard.setData(ClipboardData(text: path));
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: Text(context.l10n.wikiProjectPathCopied),
                behavior: SnackBarBehavior.floating,
              ),
            );
          },
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              border: Border.all(color: AppTheme.surface3),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  Icons.folder_outlined,
                  size: 11,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 4),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 280),
                  child: Text(
                    path,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ),
                const SizedBox(width: 4),
                Icon(Icons.copy, size: 10, color: AppTheme.textTertiary),
              ],
            ),
          ),
        ),
        IconButton(
          tooltip: context.l10n.wikiEditProjectPath,
          visualDensity: VisualDensity.compact,
          iconSize: 14,
          onPressed: () => _showEditPathDialog(context, ref),
          icon: Icon(Icons.edit_outlined, color: AppTheme.textTertiary),
        ),
      ],
    );
  }

  /// 修改项目路径：系统目录选择器选目录 → 确认框保存。
  /// 不用 TextField 手输：避免路径输错，也避开 dialog 退场动画期间
  /// dispose controller 导致的已释放访问 crash。
  Future<void> _showEditPathDialog(BuildContext context, WidgetRef ref) async {
    final picked = await FilePicker.getDirectoryPath(
      dialogTitle: context.l10n.wikiChooseProjectDirectory,
      initialDirectory: page.localPath,
    );
    if (picked == null || picked.trim().isEmpty) return;
    final newPath = picked.trim();
    if (newPath == page.localPath) return;
    if (!context.mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.wikiEditProjectPath),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.l10n.wikiOldPath,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
            SelectableText(
              page.localPath ?? '',
              style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
            ),
            const SizedBox(height: 8),
            Text(
              context.l10n.wikiNewPath,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
            SelectableText(
              newPath,
              style: TextStyle(
                fontSize: 12,
                color: AppTheme.textPrimary,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.save),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.updateProjectPath(slug: page.slug, newPath: newPath);
      ref.invalidate(wikiPageProvider(page.slug));
      ref.invalidate(wikiPagesProvider);
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.wikiProjectPathUpdated)),
      );
    } catch (e) {
      if (!context.mounted) return;
      showAppError(context, context.l10n.save, e);
    }
  }
}

/// 来源链接 chip：点击复制
class _SourceChip extends StatelessWidget {
  final String url;

  const _SourceChip({required this.url});

  @override
  Widget build(BuildContext context) {
    final host = Uri.tryParse(url)?.host ?? url;
    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: url));
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.l10n.wikiSourceLinkCopied),
            behavior: SnackBarBehavior.floating,
          ),
        );
      },
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.link, size: 11, color: AppTheme.textTertiary),
            const SizedBox(width: 4),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 180),
              child: Text(
                host,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
              ),
            ),
            const SizedBox(width: 4),
            Icon(Icons.copy, size: 10, color: AppTheme.textTertiary),
          ],
        ),
      ),
    );
  }
}

/// 联系人关系 chip：以当前页为中心展示一条关系（出→ / 入←），点击跳到对端页面。
/// 对端标题从 wikiPageProvider 读取（避开仅 slug 的冷展示）。
class _RelationChip extends ConsumerWidget {
  final Relation relation;
  final String pageSlug;

  const _RelationChip({required this.relation, required this.pageSlug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final outbound = relation.fromSlug == pageSlug;
    final otherSlug = outbound ? relation.toSlug : relation.fromSlug;
    final otherAsync = ref.watch(wikiPageProvider(otherSlug));
    final other = otherAsync.value;
    final otherTitle = other?.title ?? otherSlug;
    final arrow = outbound ? '→' : '←';

    return InkWell(
      onTap: () {
        final slug = otherSlug;
        final title = other?.title ?? slug;
        openWikiTab(ref, PageTabEntry(slug: slug, title: title));
      },
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              arrow,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
            const SizedBox(width: 4),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: Text(
                otherTitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
            const SizedBox(width: 4),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
              child: Text(
                relation.relation,
                style: TextStyle(
                  fontSize: 11,
                  color: AppTheme.accentPrimary,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 相关页面 chip（知识关系网 P3）：点击跳对端页；人工边带删除 ×，
/// 派生边（origin='citation'）只读——改 tags 让事实消失，边才消失。
class RelatedPageChip extends StatelessWidget {
  final KnowledgeRelationDto relation;
  final String pageSlug;
  final void Function(String slug, String title) onOpen;
  final VoidCallback? onRemove;

  const RelatedPageChip({
    super.key,
    required this.relation,
    required this.pageSlug,
    required this.onOpen,
    required this.onRemove,
  });

  @override
  Widget build(BuildContext context) {
    final relationKey = switch (relation.relation) {
      '被mentions' => 'mentionedBy',
      '被part_of' => 'contains',
      '被supports' => 'supportedBy',
      '被contradicts' => 'contradictedBy',
      _ => relation.relation,
    };
    final label = context.l10n.wikiRelationLabel(relationKey);
    final chip = InkWell(
      onTap: () => onOpen(relation.otherSlug, relation.otherTitle),
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: Text(
                relation.otherTitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
            const SizedBox(width: 4),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
              child: Text(
                label,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.accentPrimary,
                ),
              ),
            ),
          ],
        ),
      ),
    );
    if (onRemove == null) return chip;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        chip,
        InkWell(
          onTap: onRemove,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.all(2),
            child: Icon(Icons.close, size: 12, color: AppTheme.textTertiary),
          ),
        ),
      ],
    );
  }
}
