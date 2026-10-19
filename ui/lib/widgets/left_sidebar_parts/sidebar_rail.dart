part of '../left_sidebar.dart';

//! 窄轨侧栏 + footer 动作

/// 窄轨侧栏（64px）：平板 / 小桌面窗口的导航形态。只放域切换 + 设置，
/// 列表内容让位给主区；点域图标即切 tab，需要列表时拖宽窗口即可。
class _SidebarRail extends ConsumerWidget {
  const _SidebarRail();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final activeTab = ref.watch(sidebarTabProvider);
    final actions = _footerActions(context, ref, activeTab, iconOnly: true);
    return Container(
      width: 64,
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(right: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Column(
        children: [
          const SizedBox(height: AppTheme.space2),
          for (final (i, action) in actions.indexed) ...[
            if (i > 0) const SizedBox(height: 2),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: AppTheme.space2),
              child: action,
            ),
          ],
        ],
      ),
    );
  }
}

class _FooterAction extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;
  final bool isActive;

  /// 窄轨模式：只显示图标（居中），文字进 tooltip。
  final bool iconOnly;

  const _FooterAction({
    required this.icon,
    required this.label,
    required this.onTap,
    this.isActive = false,
    this.iconOnly = false,
  });

  @override
  Widget build(BuildContext context) => AppPressable(
    scaleOnPress: 0.98,
    duration: const Duration(milliseconds: 120),
    child: Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        hoverColor: isActive ? null : AppTheme.surface2.withValues(alpha: 0.6),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 150),
          curve: Curves.easeOut,
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space3,
            vertical: AppTheme.space2,
          ),
          decoration: BoxDecoration(
            color: isActive
                ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                : Colors.transparent,
            borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
            border: Border.all(
              color: isActive
                  ? AppTheme.accentPrimary.withValues(alpha: 0.28)
                  : Colors.transparent,
              width: 1,
            ),
          ),
          child: iconOnly
              ? Tooltip(
                  message: label,
                  child: Center(
                    child: Icon(
                      icon,
                      size: 18,
                      color: isActive
                          ? AppTheme.accentPrimary
                          : AppTheme.textSecondary,
                    ),
                  ),
                )
              : Row(
                  children: [
                    Icon(
                      icon,
                      size: 18,
                      color: isActive
                          ? AppTheme.accentPrimary
                          : AppTheme.textSecondary,
                    ),
                    const SizedBox(width: AppTheme.space2),
                    Text(
                      label,
                      style: TextStyle(
                        color: isActive
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                        fontSize: 13,
                        fontWeight: isActive
                            ? FontWeight.w600
                            : FontWeight.w400,
                      ),
                    ),
                  ],
                ),
        ),
      ),
    ),
  );
}
