import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';
import '../models/conversation.dart';
import '../models/wiki_page.dart';
import '../providers/conversation_provider.dart';
import '../providers/wiki_provider.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';

/// 主界面左侧栏：对话 / 知识库 双 Tab + 底部设置入口
class LeftSidebar extends ConsumerWidget {
  const LeftSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(sidebarTabProvider);

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
          _buildHeader(context, ref),
          _buildTabBar(ref, tab),
          Expanded(
            child: switch (tab) {
              SidebarTab.conversation => _buildConversationTab(context, ref),
              SidebarTab.wiki => const _WikiTab(),
            },
          ),
          _buildFooter(context, ref),
        ],
      ),
    );
  }

  Widget _buildHeader(BuildContext context, WidgetRef ref) {
    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
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
          Text(
            'Elsewhen',
            style: Theme.of(context).textTheme.titleLarge?.copyWith(
              color: AppTheme.textPrimary,
              fontWeight: FontWeight.w600,
            ),
          ),
          const Spacer(),
          IconButton(
            icon: const Icon(Icons.add_circle_outline),
            color: AppTheme.accentPrimary,
            onPressed: () async {
              final repo = ref.read(conversationRepositoryProvider);
              final newConv = await repo.createConversation();
              ref.read(sidebarTabProvider.notifier).state = SidebarTab.conversation;
              ref.invalidate(conversationsProvider);
              ref.read(selectedConversationIdProvider.notifier).state = newConv.id;
            },
            tooltip: '新建对话',
          ),
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
        ],
      ),
    );
  }

  Widget _buildConversationTab(BuildContext context, WidgetRef ref) {
    final conversationsAsync = ref.watch(conversationsProvider);
    final selectedId = ref.watch(selectedConversationIdProvider);

    return conversationsAsync.when(
      data: (conversations) {
        if (conversations.isEmpty) {
          return const _EmptyState(
            icon: Icons.chat_bubble_outline,
            message: '还没有对话',
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
    );
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

/// 知识库 Tab：按 kind 分组展示 wiki 页面
class _WikiTab extends ConsumerWidget {
  const _WikiTab();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pagesAsync = ref.watch(wikiPagesProvider);
    final selectedSlug = ref.watch(selectedWikiSlugProvider);

    return pagesAsync.when(
      data: (pages) {
        if (pages.isEmpty) {
          return const _EmptyState(
            icon: Icons.menu_book_outlined,
            message: '知识库还没有页面\n\n用命令行跑一次：\nelsewhen wiki digest',
          );
        }

        // 按 kind 分组，保持首次出现顺序
        final grouped = <String, List<WikiPage>>{};
        for (final page in pages) {
          grouped.putIfAbsent(page.kindLabel, () => []).add(page);
        }

        return ListView(
          padding: const EdgeInsets.symmetric(vertical: AppTheme.space1),
          children: [
            for (final entry in grouped.entries) ...[
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  AppTheme.space3,
                  AppTheme.space3,
                  AppTheme.space3,
                  AppTheme.space1,
                ),
                child: Text(
                  entry.key,
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    letterSpacing: 0.5,
                    color: AppTheme.accentMuted,
                  ),
                ),
              ),
              for (final page in entry.value)
                _WikiPageItem(
                  page: page,
                  isSelected: page.slug == selectedSlug,
                  onTap: () {
                    // 点击页面 → 新开（或激活）一个页面详情 tab
                    openWikiPageTab(ref, page);
                  },
                ),
            ],
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

  const _ConversationItem({
    required this.conversation,
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
              // Title and timestamp
              Row(
                children: [
                  Expanded(
                    child: Text(
                      conversation.displayTitle,
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
              Text(
                '${conversation.messageCount} 条消息',
                style: TextStyle(
                  color: AppTheme.textTertiary,
                  fontSize: 11,
                ),
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