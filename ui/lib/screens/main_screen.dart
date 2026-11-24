import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';
import '../providers/state_holder.dart';
import '../providers/wiki_provider.dart';
import '../providers/app_provider.dart';
import '../providers/conversation_provider.dart';
import '../providers/goal_provider.dart';
import '../providers/todo_provider.dart';
import '../screens/settings_screen.dart';
import '../widgets/left_sidebar.dart';
import '../widgets/message_area.dart';
import '../widgets/wiki_page_detail_view.dart';
import '../widgets/custom_title_bar.dart';
import '../widgets/ai_provider_setup_hint.dart';
import '../widgets/companion_revisit_hint.dart';
import '../widgets/sync_backlog_banner.dart';
import '../widgets/glass.dart';
import '../widgets/review_view.dart';
import '../widgets/goal_view.dart';
import '../widgets/chat_proposal_review.dart';
import '../widgets/sync_workspace.dart';
import '../widgets/scout_view.dart';
import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';

/// 导航抽屉停靠在内容左侧时是否展开；顶栏的菜单按钮切换它。只在本次运行内
/// 保留，重启后回到展开。模态抽屉（窄屏、桌面小窗）不看这个状态。
final _navigationDockedOpenProvider = NotifierProvider<StateHolder<bool>, bool>(
  () => StateHolder(true),
);

/// 导航抽屉停靠时的宽度。模态时用 Material 抽屉的标准宽度。
const double _kDockedNavigationWidth = 280;

class MainScreen extends ConsumerWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);
    // 切到知识库时强制刷新页面列表：对话里 AI 可能刚导入过页面（含用户确认后
    // 落库），沿用缓存会看不到最新数据。挂在这里而不是导航抽屉里：抽屉收起
    // 时不在树上，而入口也会被别处的代码直接切换。
    ref.listen(sidebarTabProvider, (prev, next) {
      if (prev != next && next == SidebarTab.wiki) {
        ref.invalidate(wikiPagesProvider);
      }
    });
    // 平台判定：移动端（Android/iOS）一律走 app 风格布局；桌面端按宽度。
    // 用平台而不是宽度断点决定，避免手机横屏/小窗误判回桌面布局。
    final isMobilePlatform = !kIsWeb && (Platform.isAndroid || Platform.isIOS);
    final isDesktopPlatform =
        !kIsWeb && (Platform.isLinux || Platform.isMacOS || Platform.isWindows);
    // 平板解锁双栏：此前按平台一刀切（Android/iOS 永远抽屉），pad 侧栏
    // 无从展现。以最短边 ≥600 判定平板（含横竖屏），pad 走桌面式双栏，
    // 分屏窄栏（<700）自动退回抽屉；手机（最短边小，含横屏/分屏/小窗）
    // 仍按平台走抽屉，保留原有的防误判语义。
    final size = MediaQuery.sizeOf(context);
    final isTablet = size.shortestSide >= 600;
    final compact = size.width < 700 || (isMobilePlatform && !isTablet);
    // 导航抽屉的两种形态。
    // 模态（盖在内容上，带遮罩）：窄屏，以及桌面的小窗（<1024）——主区本来
    // 就挤，不再让导航常占一栏，窗口拖宽即恢复停靠。桌面上用菜单按钮开，
    // Esc 或点遮罩关（框架在桌面端不启用边缘拖拽）。
    // 停靠（与内容并排）：其余情况，菜单按钮收起/展开。平板没法拖宽窗口，
    // 所以只要不是 compact 就停靠。
    final modalNavigation = compact || (isDesktopPlatform && size.width < 1024);
    final navigationDocked =
        !modalNavigation && ref.watch(_navigationDockedOpenProvider);
    // 移动端：状态栏被隐藏时（main.dart 只留 bottom overlay）不需要再补
    // 安全区；仍显示时（如系统强制显示）把 padding 加进标题栏，
    // 否则内容会被状态栏裁掉。
    final topInset = isMobilePlatform
        ? MediaQuery.viewPaddingOf(context).top
        : 0.0;
    // 顶栏标题：桌面端与移动端都用应用名（移动端 mobile 分支不显示）。
    final String title = 'Elsewhen';

    return Scaffold(
      backgroundColor: AppTheme.surface0,
      // 模态抽屉一直挂着，只在模态形态下能打开（菜单按钮，或触屏的边缘拖拽）。
      // 不按形态切成 null：抽屉开着时把窗口拖宽，Scaffold 会一直记着「抽屉
      // 开着」，窗口再缩回来它就自己弹出来。关着的抽屉不构建内容。
      drawer: const LeftSidebar(),
      drawerEnableOpenDragGesture: modalNavigation,
      // 氛围渐变底：为玻璃层提供可透出的层次，纯色底上玻璃看不出模糊。
      body: AmbientBackground(
        child: Column(
          children: [
            // 最左侧是导航抽屉的菜单按钮（所有形态都有）。
            // 移动端 app 风格：「今天」操作行居右，设置收进导航抽屉底部，
            // 标题栏不再出现设置图标。
            // 桌面端保留窗口 chrome 风格：标题在左，设置图标 + 窗口按钮在右。
            CustomTitleBar(
              title: title,
              topInset: topInset,
              leading: _NavigationMenuButton(
                modal: modalNavigation,
                dockedOpen: navigationDocked,
                desktop: isDesktopPlatform,
                onToggleDocked: () => ref
                    .read(_navigationDockedOpenProvider.notifier)
                    .update((open) => !open),
              ),
              middleActions: compact
                  ? (tab == SidebarTab.conversation
                        // 对话：高频「今天」操作行（目标/待办/创建主题/搜索）。
                        ? const [TodayTitleBarActions()]
                        // 知识库/回顾/同步/目标：顶栏放「对话/知识」快速导航按钮
                        //（当前 tab 高亮），免开抽屉切回主 tab；抽屉保留，
                        // 里面的最近页面/回顾日期/同步仪表/目标工作台导航不变。
                        : tab == SidebarTab.wiki
                        ? const [_WikiTabTitleBarActions()]
                        : const [_TabNavTitleBarActions()])
                  : null,
              actions: [
                if (!compact)
                  IconButton(
                    icon: const Icon(Icons.settings_outlined),
                    iconSize: DesignTokens.icon,
                    color: const Color(0xFF6C7A89),
                    tooltip: context.l10n.settings,
                    onPressed: () async {
                      // await 前先取全局 ProviderContainer：返回后 MainScreen
                      // 可能已被卸载（此时 ref 已失效），container 不依赖
                      // widget 生命周期，可以安全 invalidate。
                      final container = ProviderScope.containerOf(
                        context,
                        listen: false,
                      );
                      await Navigator.of(context).push(
                        MaterialPageRoute(
                          builder: (_) => const SettingsScreen(),
                        ),
                      );
                      // 从设置页返回后刷新首次运行探测（横幅是否该消失）
                      container.invalidate(aiProviderConfiguredProvider);
                    },
                  ),
              ],
            ),
            // 首次运行引导：未配置 AI Provider 时提示去设置页
            const AiProviderSetupHint(),
            // 情感回访（D6）：打开应用时最多一条温和的记忆回引，可关（设置→规则库）
            const CompanionRevisitHint(),
            // 全局同步状态位（D4）：同步停摆/积压时主界面必须看得见，且要分清
            // 「待解锁积压」与「等对端确认」—— 前者点一下就通，后者只能等。
            // 只在移动端主界面挂：桌面端的同步信息住在同步工作区（同屏可见）。
            if (isMobilePlatform)
              Builder(
                builder: (context) {
                  final repository = ref.watch(storageRepositoryProvider);
                  return SyncBacklogBanner(
                    controller: repository is RustBridgeRepository
                        ? repository.localSync
                        : null,
                    onOpenSync: () => ref
                        .read(sidebarTabProvider.notifier)
                        .set(SidebarTab.sync),
                  );
                },
              ),
            Expanded(
              child: Row(
                children: [
                  // 左：停靠的导航抽屉（一级入口 + 当前入口的上下文面板 + 设置）。
                  // 收起/展开时抽屉保持自身宽度滑入滑出，内容区跟着让位。
                  // 收起后不完全消失：降级成同宽度的图标栏（一级入口图标），
                  // 点图标切入口并重新展开抽屉；模态形态（窄屏/小窗）不占位，
                  // 仍是空操作。
                  AnimatedSwitcher(
                    duration: MotionTokens.effective(
                      context,
                      MotionTokens.panel,
                    ),
                    switchInCurve: MotionTokens.standard,
                    switchOutCurve: MotionTokens.standard,
                    transitionBuilder: (child, animation) => SizeTransition(
                      sizeFactor: animation,
                      axis: Axis.horizontal,
                      // 抽屉贴着内容一侧：宽度从 0 展开时像抽屉一样从左滑入。
                      alignment: AlignmentDirectional.topEnd,
                      child: child,
                    ),
                    child: navigationDocked
                        ? SizedBox(
                            key: const ValueKey('docked-navigation'),
                            width: _kDockedNavigationWidth,
                            // 顶部安全区已经由标题栏垫过，抽屉里不再垫一次。
                            child: MediaQuery.removePadding(
                              context: context,
                              removeTop: true,
                              child: const LeftSidebar(),
                            ),
                          )
                        : modalNavigation
                        ? const SizedBox.shrink()
                        : SizedBox(
                            key: const ValueKey('collapsed-navigation'),
                            width: LeftSidebarRail.width,
                            // 顶部安全区同上：标题栏已垫过。
                            child: MediaQuery.removePadding(
                              context: context,
                              removeTop: true,
                              // 收起态点击图标只切入口，不重新展开：
                              // 展开/收起只由顶栏菜单按钮控制。
                              child: const LeftSidebarRail(),
                            ),
                          ),
                  ),

                  // Right: 按当前 Tab 切换内容区
                  // 背景色放在 Material 上而非外层 Container：右侧内容区里的裸
                  // ListTile（ExpansionTile / CheckboxListTile 等）因此有最近的
                  // Material 祖先兜底，ink 波纹不会被 DecoratedBox 背景盖掉。
                  Expanded(
                    child: Container(
                      decoration: BoxDecoration(
                        border: Border(
                          left: BorderSide(color: AppTheme.boundaryColor),
                        ),
                      ),
                      child: Material(
                        color: AppTheme.surface0,
                        child: switch (tab) {
                          SidebarTab.conversation => const MessageArea(),
                          SidebarTab.wiki => const WikiPageDetailView(),
                          // 回顾页（R1）：日/周/月三粒度，事件明细并入日视图
                          SidebarTab.review => const ReviewView(),
                          // 同步工作区（一级入口）：LocalSyncSettings 整包，
                          // 非桌面后端（repository 非 RustBridge）维持占位空态。
                          // 同步工作区没有主区级滚动容器（内容短），Row 的
                          // 交叉轴默认居中会让卡片浮在竖直中间；用 Align
                          // 撑满后靠上，卡片从内容区顶部开始。
                          SidebarTab.sync => Align(
                            alignment: Alignment.topCenter,
                            child: Builder(
                              builder: (context) {
                                final repository = ref.watch(
                                  storageRepositoryProvider,
                                );
                                if (repository is! RustBridgeRepository) {
                                  return Center(
                                    child: Text(
                                      context.l10n.syncPlatformUnavailable,
                                      style: TextStyle(
                                        color: AppTheme.textTertiary,
                                        fontSize: TypographyTokens.secondary,
                                      ),
                                    ),
                                  );
                                }
                                return SyncWorkspace(repository: repository);
                              },
                            ),
                          ),
                          // 目标（一级入口）：主区为「工作台清单 | 详情 | AI 讨论」，
                          // master 清单与新建入口直接在主区左侧。
                          SidebarTab.goal => const GoalListView(),
                          // 情报（一级入口）：scout 镜像读面（总览/发现/数据源）。
                          SidebarTab.scout => const ScoutView(),
                        },
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 标题栏最左侧的菜单按钮：导航抽屉的开关。模态形态下打开抽屉；停靠形态下
/// 收起/展开停靠的抽屉。
class _NavigationMenuButton extends StatefulWidget {
  const _NavigationMenuButton({
    required this.modal,
    required this.dockedOpen,
    required this.desktop,
    required this.onToggleDocked,
  });

  /// 导航抽屉当前是否是模态的。
  final bool modal;

  /// 停靠的抽屉当前是否展开（模态时恒为 false）。
  final bool dockedOpen;

  /// 桌面平台：标题栏的图标都是小号，菜单按钮跟着收一档。
  final bool desktop;

  final VoidCallback onToggleDocked;

  @override
  State<_NavigationMenuButton> createState() => _NavigationMenuButtonState();
}

class _NavigationMenuButtonState extends State<_NavigationMenuButton> {
  @override
  void didUpdateWidget(_NavigationMenuButton oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.modal && !widget.modal) {
      // 窗口被拖宽、导航改为停靠：还开着的模态抽屉要收起来，否则它连同遮罩
      // 一起盖在已经停靠的导航上。收起会让 Scaffold 重建，放到这一帧之后。
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Scaffold.maybeOf(context)?.closeDrawer();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return IconButton(
      icon: const Icon(Icons.menu_outlined),
      iconSize: widget.desktop ? 18 : null,
      color: widget.desktop ? AppTheme.textSecondary : null,
      tooltip: widget.dockedOpen
          ? context.l10n.closeNavigation
          : context.l10n.openNavigation,
      onPressed: widget.modal
          ? () => Scaffold.of(context).openDrawer()
          : widget.onToggleDocked,
    );
  }
}

/// 移动端知识库 tab 顶栏动作：创建主题 / 网址导入（本页高频入口）。
/// 设置不进顶栏，统一收在导航抽屉底部（移动端唯一设置入口）。
class _WikiTabTitleBarActions extends StatelessWidget {
  const _WikiTabTitleBarActions();

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _TitleBarIconAction(
          tooltip: context.l10n.createTopic,
          icon: Icons.note_add_outlined,
          onPressed: () => openCreateTopicDialog(context),
        ),
        _TitleBarIconAction(
          tooltip: context.l10n.importUrl,
          icon: Icons.link_outlined,
          onPressed: () => openWikiImportDialog(context, urlMode: true),
        ),
      ],
    );
  }
}

/// 移动端回顾/同步 tab 顶栏动作：对话 / 知识 两个快速导航按钮（语义与
/// 导航抽屉里的同名入口一致，知识按钮先复位知识库 tab 再切）。
class _TabNavTitleBarActions extends ConsumerWidget {
  const _TabNavTitleBarActions();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _TitleBarIconAction(
          tooltip: context.l10n.conversation,
          icon: Icons.chat_bubble_outline,
          onPressed: () => ref
              .read(sidebarTabProvider.notifier)
              .set(SidebarTab.conversation),
        ),
        _TitleBarIconAction(
          tooltip: context.l10n.knowledge,
          icon: Icons.menu_book_outlined,
          onPressed: () {
            ref.read(wikiActiveTabIdProvider.notifier).set('import');
            ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
          },
        ),
      ],
    );
  }
}

/// 移动端标题栏的纯图标动作按钮。
class _TitleBarIconAction extends StatelessWidget {
  final String tooltip;
  final IconData icon;
  final Color? color;
  final VoidCallback? onPressed;

  const _TitleBarIconAction({
    required this.tooltip,
    required this.icon,
    this.color,
    this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    return IconButton(
      tooltip: tooltip,
      icon: Icon(icon, size: 20),
      color: color ?? AppTheme.textSecondary,
      visualDensity: VisualDensity.compact,
      onPressed: onPressed,
    );
  }
}

/// 标题栏「今天」操作行（移动端）：目标 / 待办 / 草稿 / 搜索，图标无文案。
/// 与消息区「今天」栏互补：栏里保留信息（对话次数 / token / provider /
/// 复制全部 / 目标入口），这里放高频操作入口。
class TodayTitleBarActions extends ConsumerWidget {
  const TodayTitleBarActions({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final conversationId = ref.watch(selectedConversationIdProvider);
    final activeGoals = ref.watch(activeGoalsProvider).value ?? const [];
    final openTodos = ref
        .watch(todosProvider)
        .value
        ?.where((todo) => !todo.isDone)
        .length;
    final searchVisible =
        conversationId != null &&
        ref.watch(messageSearchVisibleProvider(conversationId));

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _TitleBarIconAction(
          tooltip: context.l10n.goalsCount(activeGoals.length),
          icon: Icons.flag_outlined,
          color: activeGoals.isEmpty
              ? AppTheme.accentPrimary
              : AppTheme.textSecondary,
          onPressed: () => openGoalDialog(context),
        ),
        _TitleBarIconAction(
          tooltip: context.l10n.todoCount(openTodos ?? 0),
          icon: Icons.check_circle_outlined,
          onPressed: () => openTodoDialog(context),
        ),
        _TitleBarIconAction(
          tooltip: context.l10n.createTopic,
          icon: Icons.note_add_outlined,
          onPressed: () => openCreateTopicDialog(context),
        ),
        // 审阅提议：移动端 _NowStatus 整栏隐藏，桌面态的入口在手机上
        // 不可达，而输入区/联系人 banner 文案都让用户「打开审阅提议」
        // ——没有这个入口提议永远 pending。桌面同名按钮自带 pending
        // 徽标与 dialog，直接复用（紧凑态＝图标无文案）。
        if (conversationId != null)
          ChatProposalReviewButton(
            conversationId: conversationId,
            compact: true,
          ),
        _TitleBarIconAction(
          tooltip: searchVisible
              ? context.l10n.hideSearch
              : context.l10n.searchAndDate,
          icon: searchVisible ? Icons.search_off : Icons.search,
          color: searchVisible ? AppTheme.accentPrimary : null,
          onPressed: conversationId == null
              ? null
              : () => ref
                    .read(messageSearchVisibleProvider(conversationId).notifier)
                    .set(!searchVisible),
        ),
        // 设置不进顶栏，统一收在导航抽屉底部（移动端唯一设置入口）。
      ],
    );
  }
}
