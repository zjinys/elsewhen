part of '../events_view.dart';

//! 事件流/列表/卡片

class _EventStream extends ConsumerWidget {
  const _EventStream({required this.autoSelect});

  final bool autoSelect;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final selectedId = ref.watch(selectedEventIdProvider);
    final events = ref.watch(recentEventsProvider);
    return events.when(
      loading: () =>
          const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('事件加载失败', style: TextStyle(color: AppTheme.textTertiary)),
            const SizedBox(height: AppTheme.space2),
            TextButton(
              onPressed: () => ref.invalidate(recentEventsProvider),
              child: const Text('重试'),
            ),
          ],
        ),
      ),
      data: (list) {
        if (list.isEmpty) {
          return Center(
            child: Padding(
              padding: const EdgeInsets.all(AppTheme.space4),
              child: Text(
                '还没有记录过事件\n和 AI 的对话会自动作为事件记录在这里',
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: 12.5,
                  color: AppTheme.textTertiary,
                  height: 1.6,
                ),
              ),
            ),
          );
        }
        if (autoSelect && list.isNotEmpty) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            final current = ref.read(selectedEventIdProvider);
            if (current == null) {
              ref.read(selectedEventIdProvider.notifier).set(list.first.id);
            }
          });
        }
        return ListView.builder(
          padding: const EdgeInsets.all(AppTheme.space3),
          itemCount: list.length,
          itemBuilder: (context, index) {
            final event = list[index];
            return _EventCard(
              event: event,
              selected: event.id == selectedId,
              onTap: () {
                ref.read(selectedEventIdProvider.notifier).set(event.id);
                ref.invalidate(eventAnalysisDetailProvider(event.id));
                ref.invalidate(eventRuleHitsProvider(event.id));
              },
            );
          },
        );
      },
    );
  }
}

class _EventList extends ConsumerWidget {
  const _EventList();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final selectedId = ref.watch(selectedEventIdProvider);
    final events = ref.watch(recentEventsProvider);
    return events.when(
      loading: () =>
          const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => Padding(
        padding: EdgeInsets.all(AppTheme.space4),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('事件加载失败', style: TextStyle(color: AppTheme.textTertiary)),
            SizedBox(height: AppTheme.space2),
            TextButton(
              onPressed: () => ref.invalidate(recentEventsProvider),
              child: const Text('重试'),
            ),
          ],
        ),
      ),
      data: (list) {
        if (list.isEmpty) {
          return Center(
            child: Padding(
              padding: const EdgeInsets.all(AppTheme.space4),
              child: Text(
                '最近没有记录的事件',
                style: TextStyle(fontSize: 12.5, color: AppTheme.textTertiary),
              ),
            ),
          );
        }
        return ListView.builder(
          padding: EdgeInsets.all(AppTheme.space2),
          itemCount: list.length,
          itemBuilder: (context, index) {
            final event = list[index];
            return _EventCard(
              event: event,
              selected: event.id == selectedId,
              onTap: () {
                ref.read(selectedEventIdProvider.notifier).set(event.id);
                ref.invalidate(eventAnalysisDetailProvider(event.id));
                ref.invalidate(eventRuleHitsProvider(event.id));
              },
            );
          },
        );
      },
    );
  }
}

class _EventCard extends StatelessWidget {
  final api.EventSummaryDto event;
  final bool selected;
  final VoidCallback onTap;

  const _EventCard({
    required this.event,
    required this.selected,
    required this.onTap,
  });

  String _shortDate(String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    final m = parsed.month.toString().padLeft(2, '0');
    final d = parsed.day.toString().padLeft(2, '0');
    return '${parsed.year}-$m-$d';
  }

  @override
  Widget build(BuildContext context) {
    final text = event.rawText.trim();
    final excerpt = text.length > 80 ? '${text.substring(0, 80)}…' : text;
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Material(
        color: selected
            ? AppTheme.accentPrimary.withValues(alpha: 0.08)
            : AppTheme.surface1,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          side: BorderSide(
            color: selected
                ? AppTheme.accentPrimary.withValues(alpha: 0.4)
                : AppTheme.surface3,
          ),
        ),
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: AppTheme.space3,
              vertical: AppTheme.space2,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        excerpt,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 12.5,
                          color: AppTheme.textPrimary,
                          height: 1.4,
                        ),
                      ),
                    ),
                    if (event.status == 'pending')
                      Tooltip(
                        message: '已记录，待分析',
                        child: Icon(
                          Icons.schedule,
                          size: 14,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                  ],
                ),
                const SizedBox(height: 2),
                Text(
                  '${_shortDate(event.recordedAt)} · ${event.source}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
