import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import 'app_pressable.dart';

import '../models/wiki_page.dart';
import '../bridge/api.dart' as api;
import '../providers/app_provider.dart';
import '../providers/conversation_provider.dart';
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import '../providers/review_provider.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';
import 'sync_workspace.dart';
import '../bridge/rust_bridge_repository.dart';

part 'left_sidebar_parts/sidebar_rail.dart';
part 'left_sidebar_parts/sidebar_wiki_tab.dart';
part 'left_sidebar_parts/sidebar_misc.dart';
// ── 左侧栏按 rail/最近页面/杂项拆分（纯类段平移，零行为变化）──

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
