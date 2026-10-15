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
      return selectedId == null ? const _EventList() : const _EventDetail();
    }
    // 宽屏：事件流是主体——进入本域即见内容铺开（默认选中最近一条），
    // 右侧常驻详情面板；不是「空面板等用户去侧栏挑」。
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(child: _EventStream(autoSelect: selectedId == null)),
        VerticalDivider(width: 1, color: AppTheme.surface3),
        SizedBox(
          width: 460,
          child: selectedId == null
              ? Center(
                  child: Text(
                    '左侧选择事件查看详情',
                    style: TextStyle(
                      fontSize: 12.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                )
              : const _EventDetail(),
        ),
      ],
    );
  }
}

/// 宽屏事件卡流：主体内容面。autoSelect 时默认选中最近一条，
/// 用户一进域就看到「有什么 + 详情」，不需要先做一次点击。
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
              AppTheme.space2,
              AppTheme.space2,
              AppTheme.space2,
              0,
            ),
            child: Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                onPressed: () =>
                    ref.read(selectedEventIdProvider.notifier).set(null),
                icon: const Icon(Icons.arrow_back, size: 16),
                label: const Text('返回列表'),
              ),
            ),
          ),
        Expanded(
          child: detail.when(
            loading: () =>
                const Center(child: CircularProgressIndicator(strokeWidth: 2)),
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
                  child: Text(
                    '事件不存在',
                    style: TextStyle(color: AppTheme.textTertiary),
                  ),
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
            style: TextStyle(
              fontSize: 13.5,
              color: AppTheme.textPrimary,
              height: 1.6,
            ),
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
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textPrimary,
                height: 1.6,
              ),
            ),
          ),
        ],
        if (detail.eventType != null) _kv('类型', detail.eventType!),
        if (detail.people.isNotEmpty) _kv('人物', detail.people.join('、')),
        if (detail.projects.isNotEmpty) _kv('项目', detail.projects.join('、')),
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
                      style: TextStyle(
                        fontSize: 12.5,
                        color: AppTheme.textSecondary,
                      ),
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
                      style: TextStyle(
                        fontSize: 12.5,
                        color: AppTheme.textSecondary,
                      ),
                    ),
                  ),
                for (final guidanceRef in detail.guidance!.related)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: SelectableText(
                      '- ${guidanceRef.note}（wiki:${guidanceRef.slug}）',
                      style: TextStyle(
                        fontSize: 12.5,
                        color: AppTheme.textSecondary,
                      ),
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
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('反馈失败: $e')));
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
