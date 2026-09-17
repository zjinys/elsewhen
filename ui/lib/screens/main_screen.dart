import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../theme/app_theme.dart';
import '../providers/wiki_provider.dart';
import '../widgets/left_sidebar.dart';
import '../widgets/message_area.dart';
import '../widgets/wiki_page_detail_view.dart';
import '../widgets/custom_title_bar.dart';

class MainScreen extends ConsumerWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);

    return Scaffold(
      backgroundColor: AppTheme.surface0,
      body: Column(
        children: [
          const CustomTitleBar(title: 'Elsewhen'),
          Expanded(
            child: Row(
              children: [
                // Left: sidebar（对话 / 知识库 + 设置入口）
                const LeftSidebar(),

                // Right: 按当前 Tab 切换内容区
                Expanded(
                  child: Container(
                    color: AppTheme.surface0,
                    child: switch (tab) {
                      SidebarTab.conversation => const MessageArea(),
                      SidebarTab.wiki => const WikiPageDetailView(),
                      SidebarTab.todo => const _TodoPanel(),
                    },
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// 待办 tab 的右侧面板：集中展示规则/说明（真正的操作在左侧待办列表）
class _TodoPanel extends ConsumerWidget {
  const _TodoPanel();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.fact_check_outlined,
              size: 56,
              color: AppTheme.textTertiary,
            ),
            const SizedBox(height: AppTheme.space4),
            Text(
              '个人待办',
              style: TextStyle(
                fontSize: 20,
                fontWeight: FontWeight.w700,
                color: AppTheme.textPrimary,
              ),
            ),
            const SizedBox(height: AppTheme.space2),
            Text(
              '在左侧添加待办；在对话中让 AI 分析出需要跟进的事，\n确认后会自动出现在待办清单里。\n\n点击条目可切换完成状态。',
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textSecondary,
                height: 1.7,
              ),
            ),
          ],
        ),
      ),
    );
  }
}