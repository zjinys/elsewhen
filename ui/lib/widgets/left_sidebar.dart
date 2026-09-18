import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';
import '../models/conversation.dart';
import '../models/wiki_page.dart';
import '../providers/conversation_provider.dart';
import '../providers/todo_provider.dart';
import '../providers/wiki_provider.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';
import 'todo_view.dart';

/// 主界面左侧栏：对话 / 知识库 双 Tab + 底部设置入口
class LeftSidebar extends ConsumerWidget {
  const LeftSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);

    // 切到知识库 / 待办 tab 时强制刷新列表：
    // 对话里 AI 可能刚导入过页面 / 建过待办（含用户确认后落库），
    // 若沿用缓存，切换到对应 tab 会看不到最新数据。
    ref.listen(sidebarTabProvider, (prev, next) {
      if (prev == next) return;
      if (next == SidebarTab.wiki) {
        ref.invalidate(wikiPagesProvider);
      } else if (next == SidebarTab.todo) {
        ref.invalidate(todosProvider);
      }
    });

    return Container(
      width: 280,
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          right: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Column(
        children: [
          _buildTabBar(ref, tab),
          Expanded(
            child: switch (tab) {
              SidebarTab.conversation => _buildConversationTab(context, ref),
              SidebarTab.wiki => const _WikiTab(),
              SidebarTab.todo => const _TodoTab(),
            },
          ),
          _buildFooter(context, ref),
        ],
      ),
    );
  }

  Widget _buildTabBar(WidgetRef ref, SidebarTab tab) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space2),
      decoration: BoxDecoration(
        border: Border(
          bottom: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          _TabButton(
            label: '对话',
            icon: Icons.chat_bubble_outline,
            selected: tab == SidebarTab.conversation,
            onTap: () {
              ref.read(sidebarTabProvider.notifier).state = SidebarTab.conversation;
            },
          ),
          const SizedBox(width: AppTheme.space2),
          _TabButton(
            label: '知识库',
            icon: Icons.menu_book_outlined,
            selected: tab == SidebarTab.wiki,
            onTap: () {
              ref.read(sidebarTabProvider.notifier).state = SidebarTab.wiki;
            },
          ),
          const SizedBox(width: AppTheme.space2),
          _TabButton(
            label: '待办',
            icon: Icons.fact_check_outlined,
            selected: tab == SidebarTab.todo,
            onTap: () {
              ref.read(sidebarTabProvider.notifier).state = SidebarTab.todo;
            },
          ),
        ],
      ),
    );
  }

  Widget _buildConversationTab(BuildContext context, WidgetRef ref) {
    final conversationsAsync = ref.watch(conversationsProvider);
    final selectedId = ref.watch(selectedConversationIdProvider);
    final showArchived = ref.watch(showArchivedProvider);

    return Column(
      children: [
        // 工具行：新建对话 + 归档视图切换
        _buildConversationToolbar(ref, showArchived),
        Expanded(
          child: conversationsAsync.when(
            data: (conversations) {
              if (conversations.isEmpty) {
                return _EmptyState(
                  icon: showArchived ? Icons.archive_outlined : Icons.chat_bubble_outline,
                  message: showArchived ? '没有归档对话' : '还没有对话\n\n点上方「新建对话」开始',
                );
              }

              // Auto-select first conversation if none selected
              if (selectedId == null && conversations.isNotEmpty) {
                WidgetsBinding.instance.addPostFrameCallback((_) {
                  ref.read(selectedConversationIdProvider.notifier).state =
                      conversations.first.id;
                });
              }

              return ListView.builder(
                padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
                itemCount: conversations.length,
                itemBuilder: (context, index) {
                  final conversation = conversations[index];
                  final isSelected = conversation.id == selectedId;
                  return _ConversationItem(
                    conversation: conversation,
                    isSelected: isSelected,
                    onTap: () {
                      ref.read(selectedConversationIdProvider.notifier).state =
                          conversation.id;
                    },
                    onRename: () => _renameConversation(context, ref, conversation),
                    onArchive: () =>
                        _setConversationArchived(ref, conversation, !showArchived),
                  );
                },
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

  Widget _buildConversationToolbar(WidgetRef ref, bool showArchived) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space3,
        AppTheme.space2,
        AppTheme.space3,
        AppTheme.space1,
      ),
      child: Row(
        children: [
          _ToolbarAction(
            icon: Icons.add_comment_outlined,
            label: '新建对话',
            onTap: () async {
              final repo = ref.read(conversationRepositoryProvider);
              final newConv = await repo.createConversation();
              ref.read(showArchivedProvider.notifier).state = false;
              ref.invalidate(conversationsProvider);
              ref.read(selectedConversationIdProvider.notifier).state = newConv.id;
            },
          ),
          const Spacer(),
          _ToolbarAction(
            icon: showArchived ? Icons.chat_bubble_outline : Icons.archive_outlined,
            label: showArchived ? '活跃对话' : '已归档',
            onTap: () {
              ref.read(showArchivedProvider.notifier).state = !showArchived;
              ref.read(selectedConversationIdProvider.notifier).state = null;
            },
          ),
        ],
      ),
    );
  }

  Future<void> _renameConversation(
    BuildContext context,
    WidgetRef ref,
    Conversation conversation,
  ) async {
    final controller = TextEditingController(text: conversation.title ?? '');
    final newTitle = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppTheme.surface1,
        title: Text(
          '重命名对话',
          style: TextStyle(color: AppTheme.textPrimary, fontSize: 16),
        ),
        content: TextField(
          controller: controller,
          autofocus: true,
          style: TextStyle(color: AppTheme.textPrimary),
          cursorColor: AppTheme.accentPrimary,
          decoration: InputDecoration(
            hintText: '输入新标题',
            hintStyle: TextStyle(color: AppTheme.textTertiary),
            enabledBorder: OutlineInputBorder(
              borderSide: BorderSide(color: AppTheme.surface3),
            ),
            focusedBorder: OutlineInputBorder(
              borderSide: BorderSide(color: AppTheme.accentPrimary),
            ),
          ),
          onSubmitted: (value) =>
              Navigator.of(dialogContext).pop(value.trim()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: AppTheme.accentPrimary),
            onPressed: () =>
                Navigator.of(dialogContext).pop(controller.text.trim()),
            child: const Text('确定'),
          ),
        ],
      ),
    );

    if (newTitle != null && newTitle.isNotEmpty) {
      final repo = ref.read(conversationRepositoryProvider);
      await repo.renameConversation(conversation.id, newTitle);
      ref.invalidate(conversationsProvider);
    }
  }

  Future<void> _setConversationArchived(
    WidgetRef ref,
    Conversation conversation,
    bool archived,
  ) async {
    final repo = ref.read(conversationRepositoryProvider);
    await repo.setArchived(conversation.id, archived);
    if (archived && ref.read(selectedConversationIdProvider) == conversation.id) {
      ref.read(selectedConversationIdProvider.notifier).state = null;
    }
    ref.invalidate(conversationsProvider);
  }

  Widget _buildFooter(BuildContext context, WidgetRef ref) {
    return Container(
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(
            color: AppTheme.surface3,
            width: 1,
          ),
        ),
      ),
      child: InkWell(
        onTap: () {
          Navigator.of(context).push(
            MaterialPageRoute(builder: (_) => const SettingsScreen()),
          );
        },
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space4,
            vertical: AppTheme.space3,
          ),
          child: Row(
            children: [
              Icon(Icons.settings_outlined, size: 18, color: AppTheme.textSecondary),
              const SizedBox(width: AppTheme.space2),
              Text(
                '设置',
                style: TextStyle(
                  color: AppTheme.textSecondary,
                  fontSize: 13,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _TabButton extends StatelessWidget {
  final String label;
  final IconData icon;
  final bool selected;
  final VoidCallback onTap;

  const _TabButton({
    required this.label,
    required this.icon,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: Material(
        color: selected ? AppTheme.surface2 : Colors.transparent,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  icon,
                  size: 16,
                  color: selected ? AppTheme.accentPrimary : AppTheme.textTertiary,
                ),
                const SizedBox(width: AppTheme.space2),
                Text(
                  label,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                    color: selected ? AppTheme.textPrimary : AppTheme.textTertiary,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 对话 tab 顶部工具栏上的小操作按钮
class _ToolbarAction extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;

  const _ToolbarAction({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space2,
            vertical: 6,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 14, color: AppTheme.accentPrimary),
              const SizedBox(width: 6),
              Text(
                label,
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.accentPrimary,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// 知识库 Tab：搜索 + kind 过滤 + 按 kind 分组展示 wiki 页面
class _WikiTab extends ConsumerStatefulWidget {
  const _WikiTab();

  @override
  ConsumerState<_WikiTab> createState() => _WikiTabState();
}

/// 知识库列表排序方式
enum _WikiSort { updated, evidence, title }

/// 知识库按「来源/用途」分区
enum _WikiArea { all, imported, network, insight }

String _areaLabel(_WikiArea area) => switch (area) {
      _WikiArea.all => '全部',
      _WikiArea.imported => '素材库',
      _WikiArea.network => '人物/项目',
      _WikiArea.insight => '知识沉淀',
    };

class _WikiTabState extends ConsumerState<_WikiTab> {
  String _query = '';
  // null = 全部
  String? _kindFilter;
  // null = 全部标签
  String? _tagFilter;
  _WikiSort _sort = _WikiSort.updated;
  _WikiArea _area = _WikiArea.all;

  /// 按当前排序方式就地排序
  void _sortPages(List<WikiPage> pages) {
    pages.sort((a, b) => switch (_sort) {
          _WikiSort.updated => b.updatedAt.compareTo(a.updatedAt),
          _WikiSort.evidence => b.evidenceCount.compareTo(a.evidenceCount),
          _WikiSort.title => a.title.compareTo(b.title),
        });
  }

  @override
  Widget build(BuildContext context) {
    final pagesAsync = ref.watch(wikiPagesProvider);
    final selectedSlug = ref.watch(selectedWikiSlugProvider);

    return Column(
      children: [
        // 搜索框
        Padding(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space3,
            AppTheme.space2,
            AppTheme.space3,
            AppTheme.space1,
          ),
          child: TextField(
            onChanged: (value) => setState(() => _query = value.trim().toLowerCase()),
            decoration: InputDecoration(
              hintText: '搜索知识库…',
              hintStyle: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
              prefixIcon: const Icon(Icons.search, size: 16),
              prefixIconConstraints: const BoxConstraints(
                minWidth: 32,
                minHeight: 32,
              ),
              isDense: true,
              contentPadding: const EdgeInsets.symmetric(vertical: 8),
              filled: true,
              fillColor: AppTheme.surface2,
              border: OutlineInputBorder(
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                borderSide: BorderSide(color: AppTheme.surface3),
              ),
              enabledBorder: OutlineInputBorder(
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                borderSide: BorderSide(color: AppTheme.surface3),
              ),
              focusedBorder: OutlineInputBorder(
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                borderSide: BorderSide(
                  color: AppTheme.accentPrimary,
                  width: 1.5,
                ),
              ),
            ),
          ),
        ),
        Expanded(
          child: pagesAsync.when(
            data: (pages) {
              if (pages.isEmpty) {
                return const _EmptyState(
                  icon: Icons.menu_book_outlined,
                  message: '知识库还没有页面\n\n去「导入」tab 粘贴网址或文本，\n或在对话里说「导入 xxx 到知识库」',
                );
              }

              // 汇总 kind 与标签，供过滤 chip 使用
              final kinds = <String>[];
              final tagCounts = <String, int>{};
              for (final p in pages) {
                if (!kinds.contains(p.kindLabel)) kinds.add(p.kindLabel);
                for (final t in p.tags) {
                  tagCounts[t] = (tagCounts[t] ?? 0) + 1;
                }
              }
              final allTags = tagCounts.keys.toList()..sort();

              // 分区 + kind + 标签 + 搜索
              final areaFilter =
                  _area == _WikiArea.all ? null : _area.name;
              final filtered = pages.where((page) {
                if (areaFilter != null && page.area != areaFilter) {
                  return false;
                }
                if (_kindFilter != null && page.kindLabel != _kindFilter) {
                  return false;
                }
                if (_tagFilter != null && !page.tags.contains(_tagFilter)) {
                  return false;
                }
                if (_query.isNotEmpty) {
                  final hay = (page.title + page.summary + page.contentMd)
                      .toLowerCase();
                  final tagHay = page.tags.join(' ').toLowerCase();
                  if (!hay.contains(_query) && !tagHay.contains(_query)) {
                    return false;
                  }
                }
                return true;
              }).toList();

              _sortPages(filtered);

              // 按 kind 分组（组的先后 = 排序后首次出现的顺序）
              final grouped = <String, List<WikiPage>>{};
              for (final page in filtered) {
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
                          '${filtered.length} 条',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                        const Spacer(),
                        _SortMenu(
                          value: _sort,
                          onChanged: (v) => setState(() => _sort = v),
                        ),
                      ],
                    ),
                  ),
                  // 分区过滤 chips（来源/用途：素材库 / 人物项目 / 知识沉淀）
                  SizedBox(
                    height: 34,
                    child: ListView(
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(
                        horizontal: AppTheme.space3,
                      ),
                      children: [
                        for (final area in _WikiArea.values)
                          _FilterChip(
                            label: _areaLabel(area),
                            count: area == _WikiArea.all
                                ? pages.length
                                : pages.where((p) => p.area == area.name).length,
                            selected: _area == area,
                            onTap: () => setState(() {
                              _area = area;
                              _kindFilter = null;
                              _tagFilter = null;
                            }),
                          ),
                      ],
                    ),
                  ),
                  // kind 过滤 chips（水平滚动）
                  SizedBox(
                    height: 34,
                    child: ListView(
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(
                        horizontal: AppTheme.space3,
                      ),
                      children: [
                        _FilterChip(
                          label: '全部',
                          count: pages.length,
                          selected: _kindFilter == null,
                          onTap: () => setState(() => _kindFilter = null),
                        ),
                        for (final kind in kinds)
                          _FilterChip(
                            label: kind,
                            count: pages.where((p) => p.kindLabel == kind).length,
                            selected: _kindFilter == kind,
                            onTap: () => setState(() {
                              _kindFilter =
                                  _kindFilter == kind ? null : kind;
                            }),
                          ),
                      ],
                    ),
                  ),
                  // 标签过滤 chips（有标签才显示）
                  if (allTags.isNotEmpty)
                    SizedBox(
                      height: 34,
                      child: ListView(
                        scrollDirection: Axis.horizontal,
                        padding: const EdgeInsets.symmetric(
                          horizontal: AppTheme.space3,
                        ),
                        children: [
                          _TagChip(
                            label: '全部标签',
                            selected: _tagFilter == null,
                            onTap: () => setState(() => _tagFilter = null),
                          ),
                          for (final tag in allTags)
                            _TagChip(
                              label: '#$tag ${tagCounts[tag]}',
                              selected: _tagFilter == tag,
                              onTap: () => setState(() {
                                _tagFilter = _tagFilter == tag ? null : tag;
                              }),
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

/// kind 过滤 chip
class _FilterChip extends StatelessWidget {
  final String label;
  final int count;
  final bool selected;
  final VoidCallback onTap;

  const _FilterChip({
    required this.label,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: Material(
        color: selected ? AppTheme.accentPrimary : AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  '$label $count',
                  style: TextStyle(
                    fontSize: 11.5,
                    fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                    color: selected
                        ? Colors.black
                        : AppTheme.textSecondary,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 标签过滤 chip
class _TagChip extends StatelessWidget {
  final String label;
  final bool selected;
  final VoidCallback onTap;

  const _TagChip({
    required this.label,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: Material(
        color: selected ? AppTheme.accentMuted : AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            child: Text(
              label,
              style: TextStyle(
                fontSize: 11.5,
                fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                color: selected ? Colors.white : AppTheme.textSecondary,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// 排序选择器：最近更新 / 证据数 / 标题
class _SortMenu extends StatelessWidget {
  final _WikiSort value;
  final ValueChanged<_WikiSort> onChanged;

  const _SortMenu({required this.value, required this.onChanged});

  static const _labels = {
    _WikiSort.updated: '最近更新',
    _WikiSort.evidence: '证据数',
    _WikiSort.title: '标题',
  };

  @override
  Widget build(BuildContext context) {
    return PopupMenuButton<_WikiSort>(
      tooltip: '排序',
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints(),
      color: AppTheme.surface2,
      onSelected: onChanged,
      itemBuilder: (context) => [
        for (final entry in _labels.entries)
          PopupMenuItem(
            value: entry.key,
            child: Row(
              children: [
                Icon(
                  entry.key == value ? Icons.check : Icons.sort,
                  size: 15,
                  color: entry.key == value
                      ? AppTheme.accentPrimary
                      : AppTheme.textTertiary,
                ),
                const SizedBox(width: 8),
                Text(
                  entry.value,
                  style: TextStyle(
                    color: entry.key == value
                        ? AppTheme.textPrimary
                        : AppTheme.textSecondary,
                  ),
                ),
              ],
            ),
          ),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.swap_vert, size: 14, color: AppTheme.textTertiary),
            const SizedBox(width: 3),
            Text(
              _labels[value]!,
              style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
            ),
          ],
        ),
      ),
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
                        fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
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
                        Icons.link,
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
    );
  }
}

class _ConversationItem extends StatelessWidget {
  final Conversation conversation;
  final bool isSelected;
  final VoidCallback onTap;
  final VoidCallback onRename;
  final VoidCallback onArchive;

  const _ConversationItem({
    required this.conversation,
    required this.isSelected,
    required this.onTap,
    required this.onRename,
    required this.onArchive,
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
              // Title and timestamp
              Row(
                children: [
                  Expanded(
                    child: Text(
                      conversation.displayTitle,
                      style: TextStyle(
                        color: isSelected ? AppTheme.textPrimary : AppTheme.textSecondary,
                        fontSize: 14,
                        fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: AppTheme.space2),
                  Text(
                    _formatTime(conversation.updatedAt),
                    style: TextStyle(
                      color: AppTheme.textTertiary,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),

              // Preview and count
              if (conversation.lastMessagePreview != null) ...[
                const SizedBox(height: AppTheme.space1),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        conversation.lastMessagePreview!,
                        style: TextStyle(
                          color: AppTheme.textSecondary,
                          fontSize: 12,
                        ),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ],
                ),
              ],

              const SizedBox(height: AppTheme.space1),
              Row(
                children: [
                  Expanded(
                    child: Text(
                      '${conversation.messageCount} 条消息',
                      style: TextStyle(
                        color: AppTheme.textTertiary,
                        fontSize: 11,
                      ),
                    ),
                  ),
                  // 更多操作：改名 / 归档
                  PopupMenuButton<String>(
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                    splashRadius: 16,
                    icon: Icon(
                      Icons.more_horiz,
                      size: 16,
                      color: AppTheme.textTertiary,
                    ),
                    color: AppTheme.surface2,
                    onSelected: (action) {
                      switch (action) {
                        case 'rename':
                          onRename();
                        case 'archive':
                          onArchive();
                      }
                    },
                    itemBuilder: (context) => [
                      PopupMenuItem(
                        value: 'rename',
                        child: Row(
                          children: [
                            Icon(Icons.edit_outlined,
                                size: 16, color: AppTheme.textSecondary),
                            const SizedBox(width: AppTheme.space2),
                            Text('改名',
                                style: TextStyle(color: AppTheme.textPrimary)),
                          ],
                        ),
                      ),
                      PopupMenuItem(
                        value: 'archive',
                        child: Row(
                          children: [
                            Icon(
                              conversation.archived
                                  ? Icons.unarchive_outlined
                                  : Icons.archive_outlined,
                              size: 16,
                              color: AppTheme.textSecondary,
                            ),
                            const SizedBox(width: AppTheme.space2),
                            Text(
                              conversation.archived ? '取消归档' : '归档',
                              style: TextStyle(color: AppTheme.textPrimary),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);

    if (diff.inMinutes < 60) {
      return '${diff.inMinutes}分钟前';
    } else if (diff.inHours < 24) {
      return '${diff.inHours}小时前';
    } else if (diff.inDays < 7) {
      return '${diff.inDays}天前';
    } else {
      return DateFormat('MM-dd').format(time);
    }
  }
}

class _EmptyState extends StatelessWidget {
  final IconData icon;
  final String message;

  const _EmptyState({
    required this.icon,
    required this.message,
  });

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space6),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              icon,
              size: 48,
              color: AppTheme.textTertiary,
            ),
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
class _TodoTab extends StatelessWidget {
  const _TodoTab();

  @override
  Widget build(BuildContext context) {
    return const TodoListView();
  }
}