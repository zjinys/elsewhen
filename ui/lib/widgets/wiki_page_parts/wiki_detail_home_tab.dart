part of '../wiki_page_detail_view.dart';

//! 首页 tab、维护入口、tab 栏与页面 tab 体。

class _HomeTab extends StatelessWidget {
  const _HomeTab();

  @override
  Widget build(BuildContext context) {
    final narrow = MediaQuery.sizeOf(context).width < 560;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 14, 16, 12),
          child: narrow
              ? Column(
                  children: [
                    _HomeAction(
                      icon: Icons.link_outlined,
                      title: '网址导入',
                      subtitle: '抓取网页或推文，确认后保存',
                      onTap: () => openWikiImportDialog(context, urlMode: true),
                    ),
                    const SizedBox(height: 8),
                    _HomeAction(
                      icon: Icons.notes_outlined,
                      title: '文本导入',
                      subtitle: '粘贴内容并创建知识页',
                      onTap: () =>
                          openWikiImportDialog(context, urlMode: false),
                    ),
                  ],
                )
              : Row(
                  children: [
                    Expanded(
                      child: _HomeAction(
                        icon: Icons.link_outlined,
                        title: '网址导入',
                        subtitle: '抓取网页或推文，确认后保存',
                        onTap: () =>
                            openWikiImportDialog(context, urlMode: true),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: _HomeAction(
                        icon: Icons.notes_outlined,
                        title: '文本导入',
                        subtitle: '粘贴内容并创建知识页',
                        onTap: () =>
                            openWikiImportDialog(context, urlMode: false),
                      ),
                    ),
                  ],
                ),
        ),
        // 知识审阅入口（全局待办：待确认修改 + 来源检查）。只放首页，不进单页。
        const _MaintenanceEntry(),
        Divider(height: 1, color: AppTheme.surface3),
        const Expanded(child: _WikiBrowseTab()),
      ],
    );
  }
}

/// 首页的「知识审阅」入口条：显示全局待办数量（待确认修改 + 来源检查），
/// 点击打开审阅对话框。只在首页出现，不进入单个知识页。
class _MaintenanceEntry extends ConsumerWidget {
  const _MaintenanceEntry();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final proposals = ref.watch(knowledgeProposalsProvider);
    final issues = ref.watch(knowledgeIssuesProvider);
    // 待办数 = 待确认的修改 + 待核对的来源/内容检查项。
    final pendingCount =
        (proposals.value?.where((p) => p.status == 'pending').length ?? 0) +
        (issues.value?.length ?? 0);

    return Material(
      color: AppTheme.surface1,
      child: InkWell(
        onTap: () => showKnowledgeMaintenance(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 11),
          child: Row(
            children: [
              Icon(
                Icons.fact_check_outlined,
                size: 18,
                color: pendingCount > 0
                    ? AppTheme.accentPrimary
                    : AppTheme.textTertiary,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  '知识审阅',
                  style: TextStyle(
                    fontSize: 13.5,
                    fontWeight: FontWeight.w500,
                    color: AppTheme.textPrimary,
                  ),
                ),
              ),
              if (pendingCount > 0)
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 7,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.accentPrimary,
                    borderRadius: BorderRadius.circular(20),
                  ),
                  child: Text(
                    '$pendingCount',
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: AppTheme.surface0,
                    ),
                  ),
                )
              else
                Text(
                  '无待办',
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              const SizedBox(width: 4),
              Icon(Icons.chevron_right, size: 18, color: AppTheme.textTertiary),
            ],
          ),
        ),
      ),
    );
  }
}

class _HomeAction extends StatelessWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;

  const _HomeAction({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) => Material(
    color: AppTheme.surface1,
    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
    child: InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Row(
          children: [
            Icon(icon, size: 19, color: AppTheme.accentPrimary),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    subtitle,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ),
            ),
            Icon(Icons.chevron_right, size: 18, color: AppTheme.textTertiary),
          ],
        ),
      ),
    ),
  );
}

class _WikiTabBar extends ConsumerWidget {
  final List<WikiTabEntry> tabs;
  final String activeId;

  const _WikiTabBar({required this.tabs, required this.activeId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Container(
      height: 42,
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space3,
        vertical: 5,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        itemCount: tabs.length,
        separatorBuilder: (_, _) => const SizedBox(width: 6),
        itemBuilder: (context, index) {
          final tab = tabs[index];
          return _WikiTabChip(
            tab: tab,
            active: tab.id == activeId,
            onTap: () {
              ref.read(wikiActiveTabIdProvider.notifier).set(tab.id);
            },
            onClose: tab.closable ? () => _closeTab(context, ref, tab) : null,
          );
        },
      ),
    );
  }

  /// 关闭 tab；若有未保存的编辑（§6.3），先弹确认再关（v1.5：三档）。
  Future<void> _closeTab(
    BuildContext context,
    WidgetRef ref,
    WikiTabEntry tab,
  ) async {
    final slug = switch (tab) {
      PageTabEntry(:final slug) => slug,
      _ => null,
    };
    final dirty =
        slug != null && ref.read(wikiDirtyTabsProvider).contains(slug);
    if (!dirty) {
      closeWikiTab(ref, tab.id);
      return;
    }
    final action = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('关闭前确认'),
        content: const Text('该页面还有未保存的编辑，关闭将丢失这些修改。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('cancel'),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('discard'),
            child: const Text('放弃修改并关闭'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop('save'),
            child: const Text('保存并关闭'),
          ),
        ],
      ),
    );
    if (action == null || action == 'cancel') return;
    if (action == 'save') {
      // v1.5：先走页面注册的保存回调；保存失败/冲突取消则保持 tab 打开。
      final save = ref.read(wikiSaveCallbacksProvider)[slug];
      final ok = save == null ? false : await save();
      if (!ok) return;
    }
    ref
        .read(wikiDirtyTabsProvider.notifier)
        .update((set) => {...set}..remove(slug));
    closeWikiTab(ref, tab.id);
  }
}

class _WikiTabChip extends StatelessWidget {
  final WikiTabEntry tab;
  final bool active;
  final VoidCallback onTap;
  final VoidCallback? onClose;

  const _WikiTabChip({
    required this.tab,
    required this.active,
    required this.onTap,
    this.onClose,
  });

  @override
  Widget build(BuildContext context) {
    return Listener(
      // 鼠标中键点击 tab 关闭（浏览器/编辑器惯例），与「×」按钮同走
      // 关闭前脏检查（由上层传入的 onClose 承载确认逻辑）。
      onPointerDown: (event) {
        if (onClose != null && event.buttons == kMiddleMouseButton) {
          onClose!();
        }
      },
      child: Material(
        color: active ? AppTheme.surface3 : Colors.transparent,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          child: Padding(
            padding: const EdgeInsets.only(left: 12, right: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 150),
                  child: Text(
                    tab.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: active ? FontWeight.w600 : FontWeight.w400,
                      color: active
                          ? AppTheme.textPrimary
                          : AppTheme.textSecondary,
                    ),
                  ),
                ),
                if (onClose != null) ...[
                  const SizedBox(width: 2),
                  _buildCloseButton(),
                ] else
                  const SizedBox(width: 8),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildCloseButton() {
    return Tooltip(
      message: '关闭标签页',
      waitDuration: const Duration(milliseconds: 500),
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onClose,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          hoverColor: AppTheme.surface3.withValues(alpha: 0.8),
          child: Padding(
            padding: const EdgeInsets.all(5),
            child: Icon(Icons.close, size: 14, color: AppTheme.textSecondary),
          ),
        ),
      ),
    );
  }
}

// ─────────────────────────────────────────────
// 页面详情 tab
// ─────────────────────────────────────────────
