import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/events_provider.dart';
import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/rule.dart';
import '../theme/app_theme.dart';

/// 事件详情视图（遗留收口切片 C）：主工作区第三态。
///
/// 宽屏双栏（左列表右详情），窄屏单列点击进入/返回。列表卡只放摘要与
/// 状态（分析详情按事件单独拉取，不拖 result_json）；详情页顺带闭两笔
/// 旧账：D1 事件旁规则命中标记（含有用/不相关反馈）、D2 以事件为源的
/// 关系读面由 wiki 页详情承载，此处不重复展示。
class EventsView extends ConsumerWidget {
  const EventsView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final selectedId = ref.watch(selectedEventIdProvider);
    final compact = MediaQuery.of(context).size.width < 760;
    if (compact) {
      return selectedId == null
          ? const _EventList()
          : const _EventDetail();
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SizedBox(width: 300, child: _EventList()),
        VerticalDivider(width: 1, color: AppTheme.surface3),
        Expanded(child: selectedId == null ? const SizedBox.shrink() : const _EventDetail()),
      ],
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
      loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
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
        color: selected ? AppTheme.accentPrimary.withValues(alpha: 0.08) : AppTheme.surface1,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          side: BorderSide(
            color: selected ? AppTheme.accentPrimary.withValues(alpha: 0.4) : AppTheme.surface3,
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
                        child: Icon(Icons.schedule, size: 14, color: AppTheme.textTertiary),
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

/// 详情：compact 模式带返回条；宽屏无需返回（列表常驻左侧）。
class _EventDetail extends ConsumerWidget {
  const _EventDetail();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final eventId = ref.watch(selectedEventIdProvider)!;
    final compact = MediaQuery.of(context).size.width < 760;
    final detail = ref.watch(eventAnalysisDetailProvider(eventId));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (compact)
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space2, AppTheme.space2, AppTheme.space2, 0,
            ),
            child: Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                onPressed: () => ref.read(selectedEventIdProvider.notifier).set(null),
                icon: const Icon(Icons.arrow_back, size: 16),
                label: const Text('返回列表'),
              ),
            ),
          ),
        Expanded(
          child: detail.when(
            loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
            error: (e, _) => Center(
              child: Padding(
                padding: EdgeInsets.all(AppTheme.space4),
                child: Text(
                  '详情加载失败：$e',
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              ),
            ),
            data: (value) {
              if (value == null) {
                return Center(
                  child: Text('事件不存在', style: TextStyle(color: AppTheme.textTertiary)),
                );
              }
              return _EventDetailBody(eventId: eventId, detail: value);
            },
          ),
        ),
      ],
    );
  }
}

class _EventDetailBody extends ConsumerWidget {
  final String eventId;
  final api.EventAnalysisDetailDto detail;

  const _EventDetailBody({required this.eventId, required this.detail});

  String _date(String? value) {
    if (value == null || value.isEmpty) return '—';
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    final m = parsed.month.toString().padLeft(2, '0');
    final d = parsed.day.toString().padLeft(2, '0');
    return '${parsed.year}-$m-$d';
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final analysis = detail.summary != null || detail.guidance != null;
    return ListView(
      padding: EdgeInsets.all(AppTheme.space4),
      children: [
        // —— 原文（真源只读）——
        _Section(
          title: '事件记录',
          child: SelectableText(
            detail.rawText,
            style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary, height: 1.6),
          ),
        ),
        _kv('记录时间', _date(detail.recordedAt)),
        _kv('来源', detail.source),
        _kv('状态', detail.jobStatus),
        if (!analysis && detail.jobStatus != 'done')
          Padding(
            padding: const EdgeInsets.only(top: AppTheme.space4),
            child: Text(
              '尚无可用分析（等待处理或未配置分析器）。',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          ),
        if (detail.summary != null && detail.summary!.isNotEmpty) ...[
          SizedBox(height: AppTheme.space3),
          _Section(
            title: '分析摘要',
            child: SelectableText(
              detail.summary!,
              style: TextStyle(fontSize: 13, color: AppTheme.textPrimary, height: 1.6),
            ),
          ),
        ],
        if (detail.eventType != null) _kv('类型', detail.eventType!),
        if (detail.people.isNotEmpty)
          _kv('人物', detail.people.join('、')),
        if (detail.projects.isNotEmpty)
          _kv('项目', detail.projects.join('、')),
        if (detail.followUps.isNotEmpty)
          _Section(
            title: '跟进建议',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final followUp in detail.followUps)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: SelectableText(
                      '- $followUp',
                      style: TextStyle(fontSize: 12.5, color: AppTheme.textSecondary),
                    ),
                  ),
              ],
            ),
          ),
        if (detail.guidance != null &&
            (detail.guidance!.related.isNotEmpty ||
                detail.guidance!.lessons.isNotEmpty))
          _Section(
            title: '指导',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final lesson in detail.guidance!.lessons)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: SelectableText(
                      '- $lesson',
                      style: TextStyle(fontSize: 12.5, color: AppTheme.textSecondary),
                    ),
                  ),
                for (final guidanceRef in detail.guidance!.related)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: SelectableText(
                      '- ${guidanceRef.note}（wiki:${guidanceRef.slug}）',
                      style: TextStyle(fontSize: 12.5, color: AppTheme.textSecondary),
                    ),
                  ),
              ],
            ),
          ),
        // —— D1 旧账：事件旁规则命中标记 + D6 反馈 ——
        SizedBox(height: AppTheme.space3),
        _EventRuleHits(eventId: eventId),
      ],
    );
  }

  Widget _kv(String label, String value) => Padding(
    padding: EdgeInsets.only(top: AppTheme.space2),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 64,
          child: Text(
            label,
            style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
          ),
        ),
        Expanded(
          child: SelectableText(
            value,
            style: TextStyle(fontSize: 12.5, color: AppTheme.textSecondary),
          ),
        ),
      ],
    ),
  );
}

/// D1 事件旁标记（旧账闭口）：未拒绝命中行 + 有用/不相关反馈（再点一次
/// 清除）。只读展示与反馈记录，无任何自动行为消费反馈。
class _EventRuleHits extends ConsumerStatefulWidget {
  final String eventId;

  const _EventRuleHits({required this.eventId});

  @override
  ConsumerState<_EventRuleHits> createState() => _EventRuleHitsState();
}

class _EventRuleHitsState extends ConsumerState<_EventRuleHits> {
  Map<String, String?>? _optimistic;

  List<RuleHit> _applied(List<RuleHit> hits) {
    final overrides = _optimistic;
    if (overrides == null) return hits;
    return [
      for (final hit in hits)
        if (overrides.containsKey(hit.id))
          RuleHit(
            id: hit.id,
            ruleId: hit.ruleId,
            ruleContent: hit.ruleContent,
            status: hit.status,
            createdAt: hit.createdAt,
            feedback: overrides[hit.id],
          )
        else
          hit,
    ];
  }

  Future<void> _setFeedback(RuleHit hit, String feedback) async {
    final previous = hit.feedback;
    final next = previous == feedback ? null : feedback;
    final undo = _optimistic;
    setState(() {
      _optimistic = {...?undo, hit.id: next};
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setRuleHitFeedback(hit.id, next);
      _optimistic = null;
      if (mounted) ref.invalidate(eventRuleHitsProvider(widget.eventId));
    } catch (e) {
      if (!mounted) return;
      setState(() => _optimistic = undo);
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('反馈失败: $e')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final hits = ref.watch(eventRuleHitsProvider(widget.eventId));
    return hits.when(
      loading: () => const SizedBox.shrink(),
      error: (e, _) => const SizedBox.shrink(),
      data: (raw) {
        final list = _applied(raw);
        if (list.isEmpty) {
          return Padding(
            padding: EdgeInsets.only(bottom: AppTheme.space2),
            child: Text(
              '无相关规则提醒',
              style: TextStyle(
                fontSize: 12,
                color: AppTheme.textTertiary.withValues(alpha: 0.8),
              ),
            ),
          );
        }
        return _Section(
          title: '相关规则提醒（${list.length}）',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (final hit in list)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Row(
                    children: [
                      Expanded(
                        child: SelectableText(
                          hit.ruleContent,
                          style: TextStyle(
                            fontSize: 12.5,
                            color: AppTheme.textSecondary,
                            height: 1.5,
                          ),
                        ),
                      ),
                      _feedbackButton(
                        icon: Icons.thumb_up,
                        filled: hit.feedback == 'helpful',
                        tooltip: '有用（再点一次清除）',
                        onTap: () => _setFeedback(hit, 'helpful'),
                      ),
                      const SizedBox(width: 4),
                      _feedbackButton(
                        icon: Icons.thumb_down,
                        filled: hit.feedback == 'not_relevant',
                        tooltip: '不相关（再点一次清除）',
                        onTap: () => _setFeedback(hit, 'not_relevant'),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  Widget _feedbackButton({
    required IconData icon,
    required bool filled,
    required String tooltip,
    required VoidCallback onTap,
  }) => SizedBox(
    width: 30,
    height: 26,
    child: IconButton(
      padding: EdgeInsets.zero,
      iconSize: 14,
      tooltip: tooltip,
      icon: Icon(filled ? icon : iconOutlined(icon)),
      color: filled ? AppTheme.accentPrimary : AppTheme.textTertiary,
      onPressed: onTap,
    ),
  );
}

IconData iconOutlined(IconData filled) => filled == Icons.thumb_up
    ? Icons.thumb_up_outlined
    : Icons.thumb_down_outlined;

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


/// 左栏事件域精简列表：与主工作区列表共用 provider，选中态同步。
class EventsNavList extends ConsumerWidget {
  const EventsNavList({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final selectedId = ref.watch(selectedEventIdProvider);
    final events = ref.watch(recentEventsProvider);
    return events.when(
      loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => const Center(
        child: Text('事件加载失败', style: TextStyle(fontSize: 12.5)),
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
          padding: const EdgeInsets.all(AppTheme.space2),
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
