import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/app_theme.dart';
import '../providers/wiki_provider.dart';
import '../screens/settings_screen.dart';
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
          // 设置入口：图标按钮放标题栏右侧（窗口按钮左边）
          CustomTitleBar(
            title: 'Elsewhen',
            actions: [
              IconButton(
                icon: const Icon(Icons.settings_outlined),
                iconSize: 16,
                color: const Color(0xFF6C7A89),
                tooltip: '设置',
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => const SettingsScreen()),
                ),
              ),
            ],
          ),
          Expanded(
            child: Row(
              children: [
                // Left: sidebar（对话 / 知识库 + 设置入口）
                const LeftSidebar(),

                // Right: 按当前 Tab 切换内容区
                // 背景色放在 Material 上而非外层 Container：右侧内容区里的裸
                // ListTile（ExpansionTile / CheckboxListTile 等）因此有最近的
                // Material 祖先兜底，ink 波纹不会被 DecoratedBox 背景盖掉。
                Expanded(
                  child: Container(
                    decoration: BoxDecoration(
                      border: Border(left: BorderSide(color: AppTheme.surface3.withValues(alpha: 0.65))),
                    ),
                    child: Material(
                      color: AppTheme.surface0,
                      child: switch (tab) {
                        SidebarTab.conversation => const MessageArea(),
                        SidebarTab.wiki => const WikiPageDetailView(),
                      },
                    ),
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
