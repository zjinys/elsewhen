import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/app_theme.dart';
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
import '../widgets/glass.dart';
import '../widgets/review_view.dart';
import '../widgets/sync_workspace.dart';
import '../bridge/rust_bridge_repository.dart';

class MainScreen extends ConsumerWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);
    // 平台判定：移动端（Android/iOS）一律走 app 风格布局；桌面端按宽度。
    // 用平台而不是宽度断点决定，避免手机横屏/小窗误判回桌面布局。
    final isMobilePlatform = !kIsWeb && (Platform.isAndroid || Platform.isIOS);
    final compact = isMobilePlatform || MediaQuery.sizeOf(context).width < 700;
    // 移动端：状态栏被隐藏时（main.dart 只留 bottom overlay）不需要再补
    // 安全区；仍显示时（如系统强制显示）把 padding 加进标题栏，
    // 否则内容会被状态栏裁掉。
    final topInset = isMobilePlatform
        ? MediaQuery.viewPaddingOf(context).top
        : 0.0;

    return Scaffold(
      backgroundColor: AppTheme.surface0,
      drawer: compact ? const Drawer(child: LeftSidebar()) : null,
      // 氛围渐变底：为玻璃层提供可透出的层次，纯色底上玻璃看不出模糊。
      body: AmbientBackground(
        child: Column(
          children: [
            // 移动端 app 风格：菜单按钮在左（抽屉），logo+「今天」操作行居中，
            // 设置收进导航抽屉 footer，标题栏不再出现设置图标。
            // 桌面端保留窗口 chrome 风格：标题在左，设置图标 + 窗口按钮在右。
            CustomTitleBar(
              title: 'Elsewhen',
              topInset: topInset,
              leading: compact
                  ? Builder(
                      builder: (context) => IconButton(
                        icon: const Icon(Icons.menu_outlined),
                        tooltip: '打开导航',
                        onPressed: () => Scaffold.of(context).openDrawer(),
                      ),
                    )
                  : null,
              middleActions: compact
                  ? (tab == SidebarTab.conversation
                        ? const [TodayTitleBarActions()]
                        : const [_WikiTitleBarActions()])
                  : null,
              actions: [
                if (!compact)
                  IconButton(
                    icon: const Icon(Icons.settings_outlined),
                    iconSize: 16,
                    color: const Color(0xFF6C7A89),
                    tooltip: '设置',
                    onPressed: () async {
                      await Navigator.of(context).push(
                        MaterialPageRoute(
                          builder: (_) => const SettingsScreen(),
                        ),
                      );
                      // 从设置页返回后刷新首次运行探测（横幅是否该消失）
                      ref.invalidate(aiProviderConfiguredProvider);
                    },
                  ),
              ],
            ),
            // 首次运行引导：未配置 AI Provider 时提示去设置页
            const AiProviderSetupHint(),
            // 情感回访（D6）：打开应用时最多一条温和的记忆回引，可关（设置→规则库）
            const CompanionRevisitHint(),
            Expanded(
              child: Row(
                children: [
                  // Left: sidebar（对话 / 知识库 + 设置入口）
                  if (!compact) const LeftSidebar(),

                  // Right: 按当前 Tab 切换内容区
                  // 背景色放在 Material 上而非外层 Container：右侧内容区里的裸
                  // ListTile（ExpansionTile / CheckboxListTile 等）因此有最近的
                  // Material 祖先兜底，ink 波纹不会被 DecoratedBox 背景盖掉。
                  Expanded(
                    child: Container(
                      decoration: BoxDecoration(
                        border: Border(
                          left: BorderSide(
                            color: AppTheme.surface3.withValues(alpha: 0.65),
                          ),
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
                          SidebarTab.sync => Builder(
                            builder: (context) {
                              final repository = ref.watch(
                                storageRepositoryProvider,
                              );
                              if (repository is! RustBridgeRepository) {
                                return Center(
                                  child: Text(
                                    '同步在此平台暂不可用',
                                    style: TextStyle(
                                      color: AppTheme.textTertiary,
                                      fontSize: 13,
                                    ),
                                  ),
                                );
                              }
                              return SyncWorkspace(repository: repository);
                            },
                          ),
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

/// 移动端知识库 Tab 的标题栏操作行：文本导入 / 网址导入 / 设置。
/// 与桌面端 logo 标题不同——移动端标题栏空间有限，放高频操作入口。
class _WikiTitleBarActions extends ConsumerWidget {
  const _WikiTitleBarActions();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _TitleBarIconAction(
          tooltip: '创建主题',
          icon: Icons.note_add_outlined,
          onPressed: () => openWikiImportDialog(context, urlMode: false),
        ),
        _TitleBarIconAction(
          tooltip: '网址导入',
          icon: Icons.link_outlined,
          onPressed: () => openWikiImportDialog(context, urlMode: true),
        ),
        _TitleBarIconAction(
          tooltip: '设置',
          icon: Icons.settings_outlined,
          onPressed: () async {
            await Navigator.of(
              context,
            ).push(MaterialPageRoute(builder: (_) => const SettingsScreen()));
            ref.invalidate(aiProviderConfiguredProvider);
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
          tooltip: '目标${activeGoals.isEmpty ? '' : '（${activeGoals.length}）'}',
          icon: Icons.flag_outlined,
          color: activeGoals.isEmpty
              ? AppTheme.accentPrimary
              : AppTheme.textSecondary,
          onPressed: () => openGoalDialog(context),
        ),
        _TitleBarIconAction(
          tooltip: '待办（${openTodos ?? 0}）',
          icon: Icons.check_circle_outlined,
          onPressed: () => openTodoDialog(context),
        ),
        _TitleBarIconAction(
          tooltip: searchVisible ? '隐藏搜索' : '搜索与日期定位',
          icon: searchVisible ? Icons.search_off : Icons.search,
          color: searchVisible ? AppTheme.accentPrimary : null,
          onPressed: conversationId == null
              ? null
              : () => ref
                    .read(messageSearchVisibleProvider(conversationId).notifier)
                    .set(!searchVisible),
        ),
      ],
    );
  }
}
