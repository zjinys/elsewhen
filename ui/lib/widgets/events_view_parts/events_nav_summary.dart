part of '../events_view.dart';

//! 区块标题组件 + 导航摘要

class _Section extends StatelessWidget {
  final String title;
  final Widget child;

  const _Section({required this.title, required this.child});

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(bottom: AppTheme.space3),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          style: TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.w600,
            color: AppTheme.textTertiary,
          ),
        ),
        const SizedBox(height: 6),
        child,
      ],
    ),
  );
}

/// 左栏事件域概览：计数 + 引导，不放列表（宽屏主区有完整卡流，
/// 窄屏主区自带列表——侧栏再放一份就是用户看到的「两个一样的列表」）。
class EventsNavSummary extends ConsumerWidget {
  const EventsNavSummary({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final events = ref.watch(recentEventsProvider);
    return events.when(
      loading: () =>
          const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => const Center(
        child: Padding(
          padding: EdgeInsets.all(AppTheme.space4),
          child: Text('事件加载失败', style: TextStyle(fontSize: 12.5)),
        ),
      ),
      data: (list) {
        final now = DateTime.now();
        var today = 0;
        var week = 0;
        var pending = 0;
        for (final event in list) {
          final recorded = DateTime.tryParse(event.recordedAt)?.toLocal();
          if (recorded != null) {
            if (recorded.year == now.year &&
                recorded.month == now.month &&
                recorded.day == now.day) {
              today++;
            }
            if (now.difference(recorded).inDays < 7) week++;
          }
          if (event.status == 'pending') pending++;
        }
        return ListView(
          padding: const EdgeInsets.all(AppTheme.space3),
          children: [
            Padding(
              padding: const EdgeInsets.only(bottom: AppTheme.space2),
              child: Text(
                '和 AI 的对话会被记录为事件，用于回顾与知识沉淀。',
                style: TextStyle(
                  fontSize: 12,
                  color: AppTheme.textTertiary,
                  height: 1.5,
                ),
              ),
            ),
            _navStat('$today', '今天记录'),
            _navStat('$week', '最近 7 天'),
            if (pending > 0) _navStat('$pending', '待分析'),
          ],
        );
      },
    );
  }

  Widget _navStat(String value, String label) => Container(
    margin: const EdgeInsets.only(bottom: 6),
    padding: const EdgeInsets.symmetric(
      horizontal: AppTheme.space3,
      vertical: AppTheme.space2,
    ),
    decoration: BoxDecoration(
      color: AppTheme.surface1,
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      border: Border.all(color: AppTheme.surface3),
    ),
    child: Row(
      children: [
        SizedBox(
          width: 32,
          child: Text(
            value,
            style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
          ),
        ),
        Expanded(
          child: Text(
            label,
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
        ),
      ],
    ),
  );
}
