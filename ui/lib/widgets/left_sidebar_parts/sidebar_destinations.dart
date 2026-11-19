part of '../left_sidebar.dart';

//! 导航抽屉的一级入口清单与底部设置入口

/// 一级入口的标准行高（Material 3 默认），触屏平台用。
const double _kDestinationHeight = 56;

/// 桌面平台的入口行高：标准行高按 compact 密度收一档（−2 档，每档 4）。
const double _kCompactDestinationHeight = 48;

/// 选中指示器的标准宽度（Material 3 默认）；抽屉比它窄时由抽屉宽度限制。
const double _kIndicatorWidth = 336;

/// 第一个入口上方的留白（Material 3 导航抽屉规范）。
const double _kDestinationsTopGap = 12;

/// 一级入口与上下文面板之间的分割：1px 线 + 上下各 8。
const double _kSectionDividerHeight = 17;

/// 上下文面板的保底高度：窗口再矮也留这么多，抽屉改为整体滚动。
/// 取值要放得下各面板里不滚动的部分（目前最高的是情报面板的标题、三个
/// 工作区入口、来源小标题和状态行，约 290）。
const double _kMinPanelHeight = 300;

/// 一级入口：对应一个 [SidebarTab]。
class _NavDestination {
  const _NavDestination({
    required this.tab,
    required this.icon,
    required this.selectedIcon,
    required this.label,
    required this.onSelected,
    this.showDot = false,
  });

  final SidebarTab tab;
  final IconData icon;
  final IconData selectedIcon;
  final String label;
  final VoidCallback onSelected;

  /// 未读点：只在该入口有新内容且用户没停在上面时显示。
  final bool showDot;
}

/// 一级入口清单，顺序即抽屉里的显示顺序。设置不是导航目的地（打开的是
/// 二级页面，不参与选中态），固定在抽屉底部，见 [_SettingsEntry]。
List<_NavDestination> _navigationDestinations(
  BuildContext context,
  WidgetRef ref,
) {
  final activeTab = ref.watch(sidebarTabProvider);
  final hasUnreadHabit = ref.watch(pendingHabitNudgesProvider) > 0;
  final selectedConversation = ref.watch(selectedConversationIdProvider);
  final mainConversation = ref.watch(mainConversationProvider).value;
  // 只有「正停在主对话流」时不算未读；停在别的会话/别的页都提示。
  final viewingMain =
      activeTab == SidebarTab.conversation &&
      mainConversation != null &&
      selectedConversation == mainConversation.id;
  void open(SidebarTab tab) => ref.read(sidebarTabProvider.notifier).set(tab);
  return [
    _NavDestination(
      tab: SidebarTab.conversation,
      icon: Icons.chat_bubble_outline,
      selectedIcon: Icons.chat_bubble,
      label: context.l10n.conversation,
      showDot: hasUnreadHabit && !viewingMain,
      onSelected: () {
        open(SidebarTab.conversation);
        if (hasUnreadHabit) {
          final main = ref.read(mainConversationProvider).value;
          if (main != null) {
            ref.read(selectedConversationIdProvider.notifier).set(main.id);
          }
        }
      },
    ),
    _NavDestination(
      tab: SidebarTab.wiki,
      icon: Icons.menu_book_outlined,
      selectedIcon: Icons.menu_book,
      label: context.l10n.knowledge,
      onSelected: () {
        ref.read(wikiActiveTabIdProvider.notifier).set('import');
        open(SidebarTab.wiki);
      },
    ),
    // 目标一级入口（Roadmap G3+）：上下文面板是完整工作台（清单/过滤/新建
    // 入口），主区是选中目标的详情与 AI 讨论。
    _NavDestination(
      tab: SidebarTab.goal,
      icon: Icons.flag_outlined,
      selectedIcon: Icons.flag,
      label: context.l10n.goals,
      onSelected: () => open(SidebarTab.goal),
    ),
    // 情报一级入口（提案 2026-11-15 §4）：scout 镜像读面。有未读时显示
    // 小圆点，与对话未读同一形态。紧随目标之后：同为「事务型」入口，
    // 与后面的回顾/同步（状态型）区分开。
    _NavDestination(
      tab: SidebarTab.scout,
      icon: Icons.travel_explore_outlined,
      selectedIcon: Icons.travel_explore,
      label: context.l10n.scoutTab,
      showDot: (ref.watch(scoutUnreadProvider).value ?? 0) > 0,
      onSelected: () => open(SidebarTab.scout),
    ),
    // 回顾入口（R1）：原「事件」域并入回顾——日视图承载当天
    // 事件明细，周/月为实时统计拼装。见
    // docs/roadmap/2026-10-15-review-and-goal-deviation-roadmap.md。
    _NavDestination(
      tab: SidebarTab.review,
      icon: Icons.auto_awesome_outlined,
      selectedIcon: Icons.auto_awesome,
      label: context.l10n.review,
      onSelected: () => open(SidebarTab.review),
    ),
    // 同步一级入口：配对完成后的持续状态仪表（设备/心跳），不该沉在设置里。
    // 单入口：主区放完整 LocalSyncSettings 工作区，设置页的同步 tab 已删。
    _NavDestination(
      tab: SidebarTab.sync,
      icon: Icons.devices_outlined,
      selectedIcon: Icons.devices,
      label: context.l10n.sync,
      onSelected: () => open(SidebarTab.sync),
    ),
  ];
}

/// 入口图标；有未读时右上角带标准的小圆点徽标（[Badge]）。
class _DestinationIcon extends StatelessWidget {
  const _DestinationIcon(this.icon, {required this.showDot});

  final IconData icon;
  final bool showDot;

  @override
  Widget build(BuildContext context) => Badge(
    isLabelVisible: showDot,
    // 未读用主题强调色：Badge 默认的错误红在这里会被读成「出错了」。
    backgroundColor: AppTheme.accentPrimary,
    child: Icon(icon),
  );
}

/// 收起模态抽屉。抽屉停靠在内容左侧（不是模态）时什么都不做，所以抽屉里
/// 任何「选完就该让出主区」的操作都可以无条件调用。
void _closeModalDrawer(BuildContext context) {
  Scaffold.maybeOf(context)?.closeDrawer();
}

/// 抽屉底部固定的设置入口（移动端唯一的设置入口；桌面端与标题栏右上角的
/// 按钮等价）。排版量与标准入口行一致：同样的行高、图标与文字样式。
class _SettingsEntry extends StatelessWidget {
  const _SettingsEntry({required this.tileHeight, required this.bottomInset});

  final double tileHeight;

  /// 底部系统手势条的安全区。
  final double bottomInset;

  /// 入口占用的总高度（含顶部分割线，不含安全区）。
  static double heightFor(double tileHeight) => tileHeight + 1;

  Future<void> _open(BuildContext context) async {
    // 从设置页返回后要刷新首次运行探测（横幅是否该消失）。模态抽屉此时
    // 已经收起、本组件已卸载，所以提前取容器，不依赖 context 仍然有效。
    final container = ProviderScope.containerOf(context, listen: false);
    final navigator = Navigator.of(context);
    _closeModalDrawer(context);
    await navigator.push(
      MaterialPageRoute<void>(builder: (_) => const SettingsScreen()),
    );
    // 导航器还在，它上面的 ProviderScope 就还在。
    if (!navigator.mounted) return;
    container.invalidate(aiProviderConfiguredProvider);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final navigationTheme = NavigationDrawerTheme.of(context);
    const unselected = <WidgetState>{};
    final labelStyle =
        navigationTheme.labelTextStyle?.resolve(unselected) ??
        theme.textTheme.labelLarge?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        );
    final iconTheme =
        navigationTheme.iconTheme?.resolve(unselected) ??
        IconThemeData(size: 24, color: theme.colorScheme.onSurfaceVariant);
    return Padding(
      padding: EdgeInsets.only(bottom: bottomInset),
      child: SizedBox(
        height: heightFor(tileHeight),
        child: Column(
          children: [
            const Divider(height: 1),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: Semantics(
                  button: true,
                  child: InkWell(
                    customBorder: const StadiumBorder(),
                    onTap: () => _open(context),
                    child: Row(
                      children: [
                        const SizedBox(width: 16),
                        IconTheme.merge(
                          data: iconTheme,
                          child: const Icon(Icons.settings_outlined),
                        ),
                        const SizedBox(width: 12),
                        Text(context.l10n.settings, style: labelStyle),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
