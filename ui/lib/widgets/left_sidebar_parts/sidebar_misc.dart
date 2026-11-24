part of '../left_sidebar.dart';

//! 空态/回顾导航/拒绝来源折叠

//! 目标域左栏（「长程目标与航向」工作台）在 goal_view.dart 的
//! GoalWorkbenchPanel，不在此文件。

class _EmptyState extends StatelessWidget {
  final IconData icon;
  final String message;

  const _EmptyState({required this.icon, required this.message});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space6),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 48, color: AppTheme.textTertiary),
            const SizedBox(height: AppTheme.space3),
            Text(
              message,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: TypographyTokens.body,
                height: 1.5,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 待办 tab：手写待办视图（列表按进行中/已完成分组）

/// 「已否定」折叠区（负知识一等公民，提案 2026-10-19）：身份 + 否定原因
/// 只读列表，懒加载（展开才查询）。与孵化分组同色系，不放主列表。
class _RejectedSourcesTile extends ConsumerStatefulWidget {
  const _RejectedSourcesTile();

  @override
  ConsumerState<_RejectedSourcesTile> createState() =>
      _RejectedSourcesTileState();
}

class _RejectedSourcesTileState extends ConsumerState<_RejectedSourcesTile> {
  Future<List<api.RejectedSourceRow>>? _future;

  @override
  Widget build(BuildContext context) {
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
        childrenPadding: const EdgeInsets.only(
          left: AppTheme.space4,
          right: AppTheme.space2,
          bottom: AppTheme.space1,
        ),
        iconColor: AppTheme.textTertiary,
        collapsedIconColor: AppTheme.textTertiary,
        title: Row(
          children: [
            Icon(Icons.block_outlined, size: 13, color: AppTheme.textTertiary),
            const SizedBox(width: 6),
            Text(
              context.l10n.rejectedSources,
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w600,
                color: AppTheme.textTertiary,
              ),
            ),
          ],
        ),
        onExpansionChanged: (open) {
          if (open && _future == null) {
            setState(() {
              _future = ref.read(knowledgeRepositoryProvider).rejectedSources();
            });
          }
        },
        children: [
          FutureBuilder<List<api.RejectedSourceRow>>(
            future: _future,
            builder: (context, snap) {
              if (snap.connectionState != ConnectionState.done) {
                return const Padding(
                  padding: EdgeInsets.all(AppTheme.space2),
                  child: SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                );
              }
              final rows = snap.data ?? const [];
              if (rows.isEmpty) {
                return Padding(
                  padding: const EdgeInsets.symmetric(
                    vertical: AppTheme.space1,
                  ),
                  child: Text(
                    context.l10n.noRejectedSources,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                );
              }
              return Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final row in rows)
                    Padding(
                      padding: const EdgeInsets.only(bottom: AppTheme.space1),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            row.title ?? row.identity,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textSecondary,
                            ),
                          ),
                          if (row.rejectReason.isNotEmpty)
                            Text(
                              row.rejectReason,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 11,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                        ],
                      ),
                    ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}
