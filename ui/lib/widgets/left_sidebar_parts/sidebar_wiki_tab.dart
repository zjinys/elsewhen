part of '../left_sidebar.dart';

//! 知识库最近页面 tab（State+条目）

/// 知识库最近页面列表；完整搜索、过滤和排序在右侧工作区完成。
class _WikiTab extends ConsumerStatefulWidget {
  const _WikiTab();

  @override
  ConsumerState<_WikiTab> createState() => _WikiTabState();
}

class _WikiTabState extends ConsumerState<_WikiTab> {
  @override
  Widget build(BuildContext context) {
    final pagesAsync = ref.watch(wikiPagesProvider);
    final selectedSlug = ref.watch(selectedWikiSlugProvider);

    return Column(
      children: [
        Expanded(
          child: pagesAsync.when(
            data: (pages) {
              if (pages.isEmpty) {
                return const _EmptyState(
                  icon: Icons.menu_book_outlined,
                  message: '知识库还没有页面\n\n去「导入」tab 粘贴网址或文本，\n或在对话里说「导入 xxx 到知识库」',
                );
              }

              // 只显示素材/人工/联系人项目，排除 AI 提炼的 insight 区（见
              // _isSidebarVisiblePage）；与对话 tab 统一取最近 20 条。
              final filtered =
                  pages
                      .where(
                        (page) =>
                            page.status != 'archived' &&
                            _isSidebarVisiblePage(page),
                      )
                      .toList()
                    ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
              final recent = filtered.take(_kRecentVisiblePages).toList();

              // 孵化中（摘录闭环，提案 2026-10-19）独立置顶分组：导入后
              // 不入编不召回，必须有一个「待定论」库位，否则闭环走不动。
              // 纳入/丢弃（详见页动作）后随 opinion 非 null 自然离开此组。
              final incubating =
                  filtered
                      .where(
                        (page) => isIncubatingPage(page.kind, page.opinion),
                      )
                      .toList();
              // 平铺列表：只按最近更新排序，不再按主题/来源分组（用户反馈分组
              // 没必要，反而割裂浏览）。
              return Column(
                children: [
                  // 结果数 + 排序
                  Padding(
                    padding: const EdgeInsets.fromLTRB(
                      AppTheme.space3,
                      0,
                      AppTheme.space2,
                      0,
                    ),
                    child: Row(
                      children: [
                        Text(
                          '最近 ${recent.length} 条',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                        const Spacer(),
                        Text(
                          '按最近更新',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                      ],
                    ),
                  ),
                  Expanded(
                    child: ListView(
                      padding: const EdgeInsets.only(bottom: AppTheme.space1),
                      children: [
                        if (incubating.isNotEmpty) ...[
                          Padding(
                            padding: const EdgeInsets.fromLTRB(
                              AppTheme.space3,
                              AppTheme.space2,
                              AppTheme.space2,
                              AppTheme.space1,
                            ),
                            child: Row(
                              children: [
                                Icon(
                                  Icons.egg_outlined,
                                  size: 13,
                                  color: AppTheme.accentMuted,
                                ),
                                const SizedBox(width: 6),
                                Text(
                                  '孵化中 ${incubating.length} 条 · 待定论',
                                  style: TextStyle(
                                    fontSize: 11,
                                    fontWeight: FontWeight.w600,
                                    color: AppTheme.textSecondary,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          for (final page in incubating)
                            _WikiPageItem(
                              page: page,
                              isSelected: page.slug == selectedSlug,
                              onTap: () {
                                openWikiPageTab(ref, page);
                              },
                            ),
                          const _RejectedSourcesTile(),
                          const Divider(height: AppTheme.space3),
                        ],
                        for (final page in recent)
                          _WikiPageItem(
                            page: page,
                            isSelected: page.slug == selectedSlug,
                            onTap: () {
                              openWikiPageTab(ref, page);
                            },
                          ),
                      ],
                    ),
                  ),
                ],
              );
            },
            // 加载通常亚秒级：静默占位，spinner 一闪反而显拖沓。
            loading: () => const SizedBox.expand(),
            error: (error, stack) => Center(
              child: Text(
                '加载失败',
                style: TextStyle(color: AppTheme.textSecondary),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// 相对时间（列表元数据用）
String _formatRelative(DateTime time) {
  final diff = DateTime.now().difference(time);
  if (diff.inMinutes < 60) return '${diff.inMinutes}分钟前';
  if (diff.inHours < 24) return '${diff.inHours}小时前';
  if (diff.inDays < 7) return '${diff.inDays}天前';
  return DateFormat('MM-dd').format(time);
}

/// 每种 kind 一个主题色：让「分组 + 条目」的颜色形成双重视觉索引
Color _kindColor(String kind) {
  const map = <String, Color>{
    'profile': Color(0xFF7AA2F7), // 蓝
    'person': Color(0xFFF7768E), // 人物用暖红，与「关系」区分
    'recurring_cost': Color(0xFFF7768E), // 红
    'capability': Color(0xFF9ECE6A), // 绿
    'asset': Color(0xFFE0AF68), // 黄
    'project': Color(0xFFBB9AF7), // 紫
    'relationship': Color(0xFFF7768E),
    'decision': Color(0xFF7DCFFF), // 青
    'habit': Color(0xFF9ECE6A),
    'constraint': Color(0xFFA9B1D6),
    'insight': Color(0xFFE0AF68),
    'topic': Color(0xFFA9B1D6), // 灰蓝
    'method': Color(0xFF9ECE6A),
    'case': Color(0xFF7DCFFF),
    'principle': Color(0xFFBB9AF7),
    'series': Color(0xFFE0AF68),
    'source': Color(0xFF7AA2F7),
  };
  return map[kind] ?? AppTheme.accentMuted;
}

/// kind 色点 + 首字徽标：与分组标题同色的条目级视觉锚点
class _KindBadge extends StatelessWidget {
  final String kind;
  final String label;

  const _KindBadge({required this.kind, required this.label});

  @override
  Widget build(BuildContext context) {
    final color = _kindColor(kind);
    return Container(
      width: 20,
      height: 20,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.16),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: color.withValues(alpha: 0.4)),
      ),
      child: Text(
        label.characters.first,
        style: TextStyle(
          fontSize: 10,
          fontWeight: FontWeight.w700,
          color: color,
        ),
      ),
    );
  }
}

class _WikiPageItem extends StatelessWidget {
  final WikiPageMeta page;
  final bool isSelected;
  final VoidCallback onTap;

  const _WikiPageItem({
    required this.page,
    required this.isSelected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return AppPressable(
      scaleOnPress: 0.985,
      duration: const Duration(milliseconds: 120),
      child: Material(
        color: isSelected ? AppTheme.surface2 : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.all(AppTheme.space3),
            decoration: BoxDecoration(
              border: Border(
                left: BorderSide(
                  color: isSelected
                      ? AppTheme.accentPrimary
                      : Colors.transparent,
                  width: 3,
                ),
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    // kind 色点徽标：与分组标题同色，形成双重视觉索引
                    Padding(
                      padding: const EdgeInsets.only(right: AppTheme.space2),
                      child: _KindBadge(kind: page.kind, label: page.kindLabel),
                    ),
                    Expanded(
                      child: Text(
                        page.title,
                        style: TextStyle(
                          color: AppTheme.textPrimary,
                          fontSize: 14,
                          fontWeight: isSelected
                              ? FontWeight.w600
                              : FontWeight.w500,
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    const SizedBox(width: AppTheme.space2),
                    if (page.sourceUrl != null)
                      Padding(
                        padding: const EdgeInsets.only(right: 4),
                        child: Icon(
                          page.isLocalPath ? Icons.folder_outlined : Icons.link,
                          size: 12,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 2,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusFull,
                        ),
                      ),
                      child: Text(
                        '证据 ${page.evidenceCount}',
                        style: TextStyle(
                          fontSize: 10,
                          color: AppTheme.accentPrimary,
                        ),
                      ),
                    ),
                  ],
                ),
                if (page.summary.isNotEmpty) ...[
                  const SizedBox(height: AppTheme.space1),
                  Text(
                    page.summary,
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 12,
                    ),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
                const SizedBox(height: AppTheme.space1),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(
                      child: Wrap(
                        spacing: 6,
                        runSpacing: 2,
                        children: [
                          for (final tag in page.tags.take(3))
                            Text(
                              '#$tag',
                              style: TextStyle(
                                fontSize: 10.5,
                                color: AppTheme.accentMuted,
                              ),
                            ),
                          if (page.tags.length > 3)
                            Text(
                              '+${page.tags.length - 3}',
                              style: TextStyle(
                                fontSize: 10.5,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                        ],
                      ),
                    ),
                    const SizedBox(width: AppTheme.space2),
                    Text(
                      _formatRelative(page.updatedAt),
                      style: TextStyle(
                        fontSize: 10.5,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
