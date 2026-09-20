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
