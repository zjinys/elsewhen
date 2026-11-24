import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../utils/pretty_time.dart';

import 'adaptive.dart';
import 'app_pressable.dart';

import '../models/wiki_page.dart';
import '../bridge/api.dart' as api;
import '../providers/app_provider.dart';
import '../providers/conversation_provider.dart';
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../providers/scout_provider.dart';
import '../providers/scout_research_provider.dart';
import 'scout_source_identity.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';
import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';

import '../theme/design_tokens.dart';
part 'left_sidebar_parts/sidebar_destinations.dart';
part 'left_sidebar_parts/sidebar_wiki_tab.dart';
part 'left_sidebar_parts/sidebar_scout_tab.dart';
part 'left_sidebar_parts/sidebar_misc.dart';

// ── 左侧导航按 一级入口/最近页面/杂项 拆分 ──

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

/// 主界面左侧导航：标准的 Material 3 导航抽屉（[NavigationDrawer]）。
///
/// 自上而下：一级入口（[NavigationDrawerDestination]，当前入口由标准指示器
/// 高亮）→ 当前入口的上下文面板（仅对话/知识：最近页面 / 知识库导航；
/// 目标/回顾/同步/情报的内容与主工作区重复，已整体搬到右侧主区，
/// 抽屉里不再渲染）→ 底部固定的设置入口。
///
/// 同一个组件有两种挂法，由 MainScreen 决定：放进 `Scaffold.drawer` 做模态
/// 抽屉（窄屏与桌面小窗），或停靠在内容左侧（空间够时）。宽度由外层约束
/// 给出，收起抽屉的调用在停靠时是空操作。
class LeftSidebar extends ConsumerWidget {
  const LeftSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);
    final destinations = _navigationDestinations(context, ref);
    final insets = MediaQuery.paddingOf(context);
    final navigationTheme = NavigationDrawerTheme.of(context);
    // 入口行高：触屏平台用 Material 3 的默认值；桌面按框架的平台密度惯例
    // （VisualDensity.adaptivePlatformDensity 在桌面是 compact）收一档。
    // 抽屉下半部分还要放上下文面板，桌面窗口不高时每一行都要省。
    final tileHeight =
        navigationTheme.tileHeight ??
        (AdaptiveContext.isDesktopPlatform
            ? _kCompactDestinationHeight
            : _kDestinationHeight);

    return LayoutBuilder(
      builder: (context, constraints) {
        // 上下文面板各自带滚动区，必须拿到一个确定的高度。窗口够高时它正好
        // 填满一级入口与设置之间的空间；不够高时保底 [_kMinPanelHeight]，
        // 抽屉像标准抽屉那样整体滚动，入口不会被挤出屏幕。
        // 上下文面板服务对话/知识/情报三个入口：目标/回顾/同步的面板
        // 内容与主工作区重复，仍由主区直接承载。情报面板是来源筛选
        // 列表（来源数量持续增长，平铺 chip 放不下，收进侧栏滚动）。
        final showPanel =
            tab == SidebarTab.conversation ||
            tab == SidebarTab.wiki ||
            tab == SidebarTab.scout;
        final fixed =
            insets.top +
            _kDestinationsTopGap +
            destinations.length * tileHeight +
            _kSectionDividerHeight +
            _SettingsEntry.heightFor(tileHeight) +
            insets.bottom;
        final panelHeight = !showPanel
            ? 0.0
            : constraints.hasBoundedHeight
            ? math.max(_kMinPanelHeight, constraints.maxHeight - fixed)
            : _kMinPanelHeight;
        // 底部安全区由设置入口自己垫：留在 MediaQuery 里的话，NavigationDrawer
        // 内部的列表会在末尾再垫一次，上面的高度就算不准了。
        return MediaQuery.removePadding(
          context: context,
          removeBottom: true,
          child: NavigationDrawerTheme(
            data: navigationTheme.copyWith(
              tileHeight: tileHeight,
              indicatorSize: Size(
                navigationTheme.indicatorSize?.width ?? _kIndicatorWidth,
                tileHeight,
              ),
            ),
            child: NavigationDrawer(
              selectedIndex: destinations.indexWhere((d) => d.tab == tab),
              onDestinationSelected: (index) {
                destinations[index].onSelected();
                _closeModalDrawer(context);
              },
              footer: _SettingsEntry(
                tileHeight: tileHeight,
                bottomInset: insets.bottom,
              ),
              children: [
                const SizedBox(height: _kDestinationsTopGap),
                for (final destination in destinations)
                  NavigationDrawerDestination(
                    icon: _DestinationIcon(
                      destination.icon,
                      showDot: destination.showDot,
                    ),
                    selectedIcon: _DestinationIcon(
                      destination.selectedIcon,
                      showDot: destination.showDot,
                    ),
                    label: Text(destination.label),
                  ),
                if (showPanel) ...[
                  const Divider(
                    height: _kSectionDividerHeight,
                    indent: 28,
                    endIndent: 28,
                  ),
                  SizedBox(
                    height: panelHeight,
                    child: _contextPanel(context, ref, tab),
                  ),
                ] else ...[
                  // 无上下文面板时（目标/回顾/同步/情报）：导航组收尾给一条
                  // 淡分割线，让入口列与下方的大片空白有视觉边界，不再像
                  // 入口清单意外断在半空。
                  const Divider(
                    height: _kSectionDividerHeight,
                    indent: 28,
                    endIndent: 28,
                  ),
                ],
              ],
            ),
          ),
        );
      },
    );
  }

  /// 当前入口的上下文面板（仅对话/知识；其余入口的内容已搬进主工作区）。
  Widget _contextPanel(BuildContext context, WidgetRef ref, SidebarTab tab) {
    return switch (tab) {
      SidebarTab.conversation => _buildConversationTab(context, ref),
      SidebarTab.wiki => const _WikiTab(),
      SidebarTab.scout => const _ScoutSourcePanel(),
      // 目标/回顾/同步：面板已移除，主区直接承载。不会走到这里
      // （showPanel 为 false 时面板整体不渲染），兜底空面板。
      _ => const SizedBox.shrink(),
    };
  }

  Widget _buildConversationTab(BuildContext context, WidgetRef ref) {
    final main = ref.watch(mainConversationProvider);
    return main.when(
      // 启动期占位：静默空白——FFI 库装载 + DB 打开通常亚秒级，
      // spinner 转一圈反而制造「在等什么」的错觉；数据到位即浮现。
      loading: () => const SizedBox.expand(),
      error: (error, stack) =>
          Center(child: Text(context.l10n.conversationLoadFailed)),
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
                context.l10n.recentlyUsed,
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
                        context.l10n.noRecentPages,
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
                            horizontalTitleGap: 10,
                            minLeadingWidth: 16,
                            shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(6),
                            ),
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: 16,
                              vertical: 1,
                            ),
                            leading: Icon(
                              page.kind == 'topic'
                                  ? Icons.tag_outlined
                                  : Icons.description_outlined,
                              size: 15,
                              color: AppTheme.textSecondary,
                            ),
                            title: Text(
                              page.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: TypographyTokens.secondary,
                                color: AppTheme.textPrimary,
                              ),
                            ),
                            subtitle: Text(
                              context.pageKindLabel(page.kind),
                              style: TextStyle(
                                fontSize: TypographyTokens.micro,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                            onTap: () {
                              ref
                                  .read(sidebarTabProvider.notifier)
                                  .set(SidebarTab.wiki);
                              openWikiPageTab(ref, page);
                              _closeModalDrawer(context);
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
}

/// 收起态的导航图标栏：停靠抽屉收起时不完全消失，留一列一级入口图标。
///
/// 视觉是 [LeftSidebar] 一级入口的纵向压缩版：只保留图标（含未读点）与
/// 选中指示胶囊，不放文字与上下文面板。点击图标切到对应入口并**保持收
/// 起**——展开/收起只由顶栏菜单按钮控制，图标栏不做双向开关。
class LeftSidebarRail extends ConsumerWidget {
  const LeftSidebarRail({super.key});

  /// 图标栏的固定宽度：图标 24 + 胶囊内边距，单列竖排够用。
  static const double width = 56;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);
    final destinations = _navigationDestinations(context, ref);
    return Material(
      color: AppTheme.surface0,
      child: Column(
        children: [
          // 与抽屉一致的第一入口上方留白。
          const SizedBox(height: _kDestinationsTopGap),
          for (final destination in destinations)
            _RailIcon(
              icon: destination.tab == tab
                  ? destination.selectedIcon
                  : destination.icon,
              tooltip: destination.label,
              selected: destination.tab == tab,
              showDot: destination.showDot,
              onTap: destination.onSelected,
            ),
          // 剩余空间全部让给顶部入口群，设置入口钉在底部——与抽屉里
          // 「设置固定在底部」的位置语义一致；入口群与下方空白之间给一条
          // 淡分割线，与抽屉的无面板形态对齐。
          const Spacer(),
          const Divider(height: 1, indent: 12, endIndent: 12),
          const SizedBox(height: 6),
          _RailIcon(
            icon: Icons.settings_outlined,
            tooltip: context.l10n.settings,
            selected: false,
            onTap: () => _openSettingsFromRail(context),
          ),
          const SizedBox(height: 4),
        ],
      ),
    );
  }

  /// 与抽屉底部 [_SettingsEntry] 相同的打开流程：关抽屉在这里是空操作
  /// （停靠态），从设置页返回后刷新首次运行探测。
  Future<void> _openSettingsFromRail(BuildContext context) async {
    final navigator = Navigator.of(context);
    final container = ProviderScope.containerOf(context, listen: false);
    await navigator.push(
      MaterialPageRoute<void>(builder: (_) => const SettingsScreen()),
    );
    if (!navigator.mounted) return;
    container.invalidate(aiProviderConfiguredProvider);
  }
}

/// 图标栏的单个入口：胶囊指示器 + 图标（可带未读点），无文字。
class _RailIcon extends StatelessWidget {
  const _RailIcon({
    required this.icon,
    required this.tooltip,
    required this.selected,
    required this.onTap,
    this.showDot = false,
  });

  final IconData icon;
  final String tooltip;
  final bool selected;
  final bool showDot;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Material(
        color: selected
            ? AppTheme.accentPrimary.withValues(alpha: 0.14)
            : Colors.transparent,
        shape: const StadiumBorder(),
        child: Tooltip(
          message: tooltip,
          child: InkWell(
            customBorder: const StadiumBorder(),
            onTap: onTap,
            child: SizedBox(
              width: 40,
              height: 40,
              child: Center(
                child: Badge(
                  isLabelVisible: showDot,
                  // 未读用主题强调色：Badge 默认的错误红在这里会被读成
                  // 「出错了」，与抽屉里 [_DestinationIcon] 同一形态。
                  backgroundColor: AppTheme.accentPrimary,
                  child: Icon(
                    icon,
                    size: 24,
                    color: selected
                        ? AppTheme.accentPrimary
                        : AppTheme.textSecondary,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
