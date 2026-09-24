import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../models/wiki_page.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';

/// 主界面左侧栏：主对话 + 最近页面 + 工作区快捷入口。
class LeftSidebar extends ConsumerWidget {
  const LeftSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);

    // 切到知识库 / 待办时强制刷新列表：
    // 对话里 AI 可能刚导入过页面 / 建过待办（含用户确认后落库），
    // 若沿用缓存，切换到对应 tab 会看不到最新数据。
    ref.listen(sidebarTabProvider, (prev, next) {
      if (prev == next) return;
      if (next == SidebarTab.wiki) {
        ref.invalidate(wikiPagesProvider);
      }
    });

    return Container(
      width: 280,
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(right: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        children: [
          Expanded(
            child: switch (tab) {
              SidebarTab.conversation => _buildConversationTab(context, ref),
              SidebarTab.wiki => const _WikiTab(),
            },
          ),
          _buildFooter(context, ref),
        ],
      ),
    );
  }

  Widget _buildConversationTab(BuildContext context, WidgetRef ref) {
    final main = ref.watch(mainConversationProvider);
    return main.when(
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (error, stack) => Center(child: Text('对话加载失败')),
      data: (conversation) {
        if (ref.read(selectedConversationIdProvider) != conversation.id) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (context.mounted) {
              ref.read(selectedConversationIdProvider.notifier).state =
                  conversation.id;
            }
          });
        }
        final pages = ref.watch(wikiPagesProvider).valueOrNull ?? const [];
        final recentPages =
            pages.where((page) => page.status != 'archived').toList()
              ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
        final visiblePages = recentPages.take(10).toList();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Text(
                '最近使用',
                style: TextStyle(
                  color: AppTheme.textTertiary,
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            Expanded(
              child: visiblePages.isEmpty
                  ? Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                      child: Text(
                        '还没有最近使用的页面',
                        style: TextStyle(
                          color: AppTheme.textTertiary,
                          fontSize: 12,
                        ),
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      itemCount: visiblePages.length,
                      itemBuilder: (context, index) {
                        final page = visiblePages[index];
                        return Material(
                          color: Colors.transparent,
                          child: ListTile(
                            dense: true,
                            leading: Icon(
                              page.kind == 'topic'
                                  ? Icons.tag_outlined
                                  : Icons.description_outlined,
                              size: 16,
                            ),
                            title: Text(
                              page.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            subtitle: Text(
                              page.kindLabel,
                              style: TextStyle(
                                fontSize: 11,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                            onTap: () {
                              ref.read(sidebarTabProvider.notifier).state =
                                  SidebarTab.wiki;
                              openWikiPageTab(ref, page);
                            },
                          ),
                        );
                      },
                    ),
            ),
          ],
        );
      },
    );
  }

  Widget _buildFooter(BuildContext context, WidgetRef ref) {
    return Container(
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        children: [
          _FooterAction(
            icon: Icons.chat_bubble_outline,
            label: '对话',
            onTap: () {
              ref.read(sidebarTabProvider.notifier).state =
                  SidebarTab.conversation;
            },
          ),
          _FooterAction(
            icon: Icons.menu_book_outlined,
            label: '知识库',
            onTap: () {
              ref.read(wikiActiveTabIdProvider.notifier).state = 'import';
              ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
            },
          ),
        ],
      ),
    );
  }
}

class _FooterAction extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;

  const _FooterAction({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onTap,
    child: Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space4,
        vertical: AppTheme.space2,
      ),
      child: Row(
        children: [
          Icon(icon, size: 18, color: AppTheme.textSecondary),
          const SizedBox(width: AppTheme.space2),
          Text(
            label,
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          ),
        ],
      ),
    ),
  );
}

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

              final filtered =
                  pages.where((page) => page.status != 'archived').toList()
                    ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
              final recent = filtered.take(10).toList();

              // 按 kind 分组（组的先后 = 排序后首次出现的顺序）
              final grouped = <String, List<WikiPage>>{};
              for (final page in recent) {
                grouped.putIfAbsent(page.kindLabel, () => []).add(page);
              }

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
                        for (final entry in grouped.entries) ...[
                          Padding(
                            padding: const EdgeInsets.fromLTRB(
                              AppTheme.space3,
                              AppTheme.space3,
                              AppTheme.space3,
                              AppTheme.space1,
                            ),
                            child: Row(
                              children: [
                                Container(
                                  width: 8,
                                  height: 8,
                                  decoration: BoxDecoration(
                                    color: _kindColor(entry.value.first.kind),
                                    shape: BoxShape.circle,
                                  ),
                                ),
                                const SizedBox(width: 6),
                                Text(
                                  '${entry.key}（${entry.value.length}）',
                                  style: TextStyle(
                                    fontSize: 11,
                                    fontWeight: FontWeight.w600,
                                    letterSpacing: 0.5,
                                    color: _kindColor(entry.value.first.kind),
                                  ),
                                ),
                              ],
                            ),
                          ),
                          for (final page in entry.value)
                            _WikiPageItem(
                              page: page,
                              isSelected: page.slug == selectedSlug,
                              onTap: () {
                                openWikiPageTab(ref, page);
                              },
                            ),
                        ],
                      ],
                    ),
                  ),
                ],
              );
            },
            loading: () => const Center(child: CircularProgressIndicator()),
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
  final WikiPage page;
  final bool isSelected;
  final VoidCallback onTap;

  const _WikiPageItem({
    required this.page,
    required this.isSelected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Material(
      color: isSelected ? AppTheme.surface2 : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.all(AppTheme.space3),
          decoration: BoxDecoration(
            border: Border(
              left: BorderSide(
                color: isSelected ? AppTheme.accentPrimary : Colors.transparent,
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
                        page.isLocalPath
                            ? Icons.folder_outlined
                            : Icons.link,
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
                      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
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
                  style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
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
    );
  }
}

class _EmptyState extends StatelessWidget {
  final IconData icon;
  final String message;

  const _EmptyState({required this.icon, required this.message});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space6),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 48, color: AppTheme.textTertiary),
            const SizedBox(height: AppTheme.space3),
            Text(
              message,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 13,
                height: 1.6,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 待办 tab：手写待办视图（列表按进行中/已完成分组）
