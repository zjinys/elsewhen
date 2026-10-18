import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import 'app_pressable.dart';

import '../models/wiki_page.dart';
import '../providers/app_provider.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../providers/review_provider.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';
import 'sync_workspace.dart';
import '../bridge/rust_bridge_repository.dart';

/// 「最近使用」最多显示的页面数。
///
/// 数据源是 `wikiPagesProvider` → `listWikiPageMetas()`：Rust 侧已按
/// updated_at 倒序截断（默认 500，单查询无正文无图片解析），这里再取前 20。
/// Rust 的截断上限内的排序语义与本处一致，所以「500 条之外不可能进入
/// 最近 20」，不会出现截断导致的排序错漏。
const int _kRecentVisiblePages = 20;

/// AI 从素材**自动消化**出的知识页类型：不进左侧「最近」列表。
/// 这些是后台提炼产物（方法/案例/规律/洞察），不是用户主动要看的东西。
const Set<String> _kAiDigestedKinds = {
  'method',
  'case',
  'principle',
  'insight',
};

/// AI 知识整合**自动生成**的主题的 slug 前缀（哈希 slug，如
/// `knowledge-topic/1cac3d8b...`）。这类主题是后台跨素材整合产出的，不是用户
/// 主动存的；用户手动建的主题 slug 是 `kb-xxx` / `topic-xxx` / `topic/中文名`
/// 等形态。以此为唯一可靠区分（无专门来源字段）。
const String _kAiGeneratedTopicSlugPrefix = 'knowledge-topic/';

/// 左侧「最近」列表显示哪些页：排除两类 AI 产物——
/// 1. AI 自动提炼的类型（_kAiDigestedKinds：方法/案例/规律/洞察）；
/// 2. AI 自动整合生成的主题（slug 带 _kAiGeneratedTopicSlugPrefix 前缀）。
/// 其余都保留——imported 素材（抓取/导入）、network（联系人/项目）、以及用户
/// **主动存的主题（topic）**等人工意图产物。派生产物（derivative）后端
/// list_wiki_page_metas 已默认排除。
bool _isSidebarVisiblePage(WikiPageMeta page) {
  if (_kAiDigestedKinds.contains(page.kind)) return false;
  if (page.slug.startsWith(_kAiGeneratedTopicSlugPrefix)) return false;
  return true;
}

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

    // 平板 / 小桌面窗口（600–1024）：主区被挤，侧栏收成 64px 图标轨；
    // 图标按钮与抽屉 footer 用同一套 _FooterAction（iconOnly），
    // 三个域的切换语义完全一致，只是换了密度。
    // 注：抽屉形态（compact，<700）总是用宽侧栏，由 Drawer 约束决定。
    final width = MediaQuery.maybeSizeOf(context)?.width ?? 1024;
    final rail = width >= 700 && width < 1024;
    if (rail) return const _SidebarRail();

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
              // 回顾页左栏：近 7 天入口，点击切换日期与日粒度
              SidebarTab.review => const _ReviewNavSummary(),
              // 同步左栏：健康度仪表（成员/心跳），操作在右侧工作区
              SidebarTab.sync => SyncNavSummary(
                  repository: ref.watch(storageRepositoryProvider),
                ),
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
      // 启动期占位：静默空白——FFI 库装载 + DB 打开通常亚秒级，
      // spinner 转一圈反而制造「在等什么」的错觉；数据到位即浮现。
      loading: () => const SizedBox.expand(),
      error: (error, stack) => Center(child: Text('对话加载失败')),
      data: (conversation) {
        if (ref.read(selectedConversationIdProvider) != conversation.id) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (context.mounted) {
              ref
                  .read(selectedConversationIdProvider.notifier)
                  .set(conversation.id);
            }
          });
        }
        final pages = ref.watch(wikiPagesProvider).value ?? const [];
        final recentPages =
            pages
                .where(
                  (page) =>
                      page.status != 'archived' && _isSidebarVisiblePage(page),
                )
                .toList()
              ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
        final visiblePages = recentPages.take(_kRecentVisiblePages).toList();
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
                              ref
                                  .read(sidebarTabProvider.notifier)
                                  .set(SidebarTab.wiki);
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
    final activeTab = ref.watch(sidebarTabProvider);
    return Container(
      padding: const EdgeInsets.all(AppTheme.space2),
      decoration: BoxDecoration(
        color: AppTheme.surface0,
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        children: [
          for (final (i, action) in _footerActions(
            context,
            ref,
            activeTab,
          ).indexed) ...[if (i > 0) const SizedBox(height: 2), action],
        ],
      ),
    );
  }
}

/// 底部导航动作清单：宽侧栏（带文字）与窄轨（纯图标）共用，
/// 保证两种密度下的切换语义、设置入口、抽屉关闭逻辑一致。
List<_FooterAction> _footerActions(
  BuildContext context,
  WidgetRef ref,
  SidebarTab activeTab, {
  bool iconOnly = false,
}) {
  return [
    _FooterAction(
      icon: Icons.chat_bubble_outline,
      label: '对话',
      isActive: activeTab == SidebarTab.conversation,
      iconOnly: iconOnly,
      onTap: () {
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.conversation);
      },
    ),
    _FooterAction(
      icon: Icons.menu_book_outlined,
      label: '知识',
      isActive: activeTab == SidebarTab.wiki,
      iconOnly: iconOnly,
      onTap: () {
        ref.read(wikiActiveTabIdProvider.notifier).set('import');
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
      },
    ),
    // 回顾入口（R1）：原「事件」域并入回顾——日视图承载当天
    // 事件明细，周/月为实时统计拼装。见
    // docs/roadmap/2026-10-15-review-and-goal-deviation-roadmap.md。
    _FooterAction(
      icon: Icons.auto_awesome_outlined,
      label: '回顾',
      isActive: activeTab == SidebarTab.review,
      iconOnly: iconOnly,
      onTap: () {
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.review);
      },
    ),
    // 同步一级入口：配对完成后的持续状态仪表（设备/心跳），不该沉在设置里。
    // 单入口：主区放完整 LocalSyncSettings 工作区，设置页的同步 tab 已删。
    _FooterAction(
      icon: Icons.devices_outlined,
      label: '同步',
      isActive: activeTab == SidebarTab.sync,
      iconOnly: iconOnly,
      onTap: () {
        ref.read(sidebarTabProvider.notifier).set(SidebarTab.sync);
      },
    ),
    // 移动端唯一设置入口（标题栏不再放设置图标）；桌面端点开与
    // 标题栏右上角按钮等价。抽屉里打开时先关抽屉再进设置页。
    _FooterAction(
      icon: Icons.settings_outlined,
      label: '设置',
      isActive: false,
      iconOnly: iconOnly,
      onTap: () async {
        final scaffold = Scaffold.maybeOf(context);
        if (scaffold?.isDrawerOpen ?? false) {
          Navigator.of(context).pop();
        }
        await Navigator.of(context)
            .push(MaterialPageRoute(builder: (_) => const SettingsScreen()));
        // 从设置页返回后刷新首次运行探测（横幅是否该消失）
        ref.invalidate(aiProviderConfiguredProvider);
      },
    ),
  ];
}

/// 窄轨侧栏（64px）：平板 / 小桌面窗口的导航形态。只放域切换 + 设置，
/// 列表内容让位给主区；点域图标即切 tab，需要列表时拖宽窗口即可。
class _SidebarRail extends ConsumerWidget {
  const _SidebarRail();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final activeTab = ref.watch(sidebarTabProvider);
    final actions = _footerActions(context, ref, activeTab, iconOnly: true);
    return Container(
      width: 64,
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(right: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        children: [
          const SizedBox(height: AppTheme.space2),
          for (final (i, action) in actions.indexed) ...[
            if (i > 0) const SizedBox(height: 2),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: AppTheme.space2),
              child: action,
            ),
          ],
        ],
      ),
    );
  }
}

class _FooterAction extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;
  final bool isActive;

  /// 窄轨模式：只显示图标（居中），文字进 tooltip。
  final bool iconOnly;

  const _FooterAction({
    required this.icon,
    required this.label,
    required this.onTap,
    this.isActive = false,
    this.iconOnly = false,
  });

  @override
  Widget build(BuildContext context) => AppPressable(
    scaleOnPress: 0.98,
    duration: const Duration(milliseconds: 120),
    child: Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        hoverColor: isActive ? null : AppTheme.surface2.withValues(alpha: 0.6),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 150),
          curve: Curves.easeOut,
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space3,
            vertical: AppTheme.space2,
          ),
          decoration: BoxDecoration(
            color: isActive
                ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                : Colors.transparent,
            borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
            border: Border.all(
              color: isActive
                  ? AppTheme.accentPrimary.withValues(alpha: 0.28)
                  : Colors.transparent,
              width: 1,
            ),
          ),
          child: iconOnly
              ? Tooltip(
                  message: label,
                  child: Center(
                    child: Icon(
                      icon,
                      size: 18,
                      color: isActive
                          ? AppTheme.accentPrimary
                          : AppTheme.textSecondary,
                    ),
                  ),
                )
              : Row(
                  children: [
                    Icon(
                      icon,
                      size: 18,
                      color: isActive
                          ? AppTheme.accentPrimary
                          : AppTheme.textSecondary,
                    ),
                    const SizedBox(width: AppTheme.space2),
                    Text(
                      label,
                      style: TextStyle(
                        color: isActive
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                        fontSize: 13,
                        fontWeight: isActive
                            ? FontWeight.w600
                            : FontWeight.w400,
                      ),
                    ),
                  ],
                ),
        ),
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

/// 回顾页左栏：近 7 天日期入口（含今天）。点击即切换锚点日期到该日并
/// 落到日粒度——是与日视图最自然的联动，周/月切换在主工作区顶部完成。
class _ReviewNavSummary extends ConsumerWidget {
  const _ReviewNavSummary();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final anchor = ref.watch(reviewAnchorDateProvider);
    final today = DateTime.now();
    final days = List<DateTime>.generate(
      7,
      (i) => today.subtract(Duration(days: i)),
    );
    return ListView(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space2),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
          child: Text(
            '近 7 天',
            style: TextStyle(
              fontSize: 11.5,
              fontWeight: FontWeight.w600,
              letterSpacing: 0.4,
              color: AppTheme.textTertiary,
            ),
          ),
        ),
        for (final day in days)
          Builder(
            builder: (context) {
              final key =
                  '${day.year}-${day.month.toString().padLeft(2, '0')}-${day.day.toString().padLeft(2, '0')}';
              final selected = key == anchor;
              return ListTile(
                dense: true,
                selected: selected,
                selectedTileColor: AppTheme.surface2,
                title: Text(
                  key ==
                          '${today.year}-${today.month.toString().padLeft(2, '0')}-${today.day.toString().padLeft(2, '0')}'
                      ? '今天'
                      : key,
                  style: TextStyle(
                    fontSize: 13,
                    color: selected
                        ? AppTheme.textPrimary
                        : AppTheme.textSecondary,
                    fontFeatures: const [FontFeature.tabularFigures()],
                  ),
                ),
                onTap: () {
                  ref.read(reviewAnchorDateProvider.notifier).set(key);
                  ref
                      .read(reviewGranularityProvider.notifier)
                      .set(ReviewGranularity.day);
                },
              );
            },
          ),
      ],
    );
  }
}
