import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../providers/knowledge_provider.dart';
import '../providers/review_provider.dart';
import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';

part 'review_view_parts/review_day_view.dart';
part 'review_view_parts/review_period_report.dart';
part 'review_view_parts/review_period_view.dart';
// ── 回顾页组件按日视图/周期报告/周期视图拆分（纯类段平移，零行为变化）──

/// 回顾页（R1 第一刀）：主工作区的回顾读面。
///
/// 三粒度（日/周/月）共用一个锚点日期：
/// - 日视图 = `get_daily_overview` 五块（AI 回顾、当日事件、待办、
///   规则提醒、记忆回引）+ 当日事件明细区（原文内联展开）；
/// - 周/月视图 = 本地 SQL 实时拼装统计（Rust `get_period_stats`），
///   无 AI、不落表；空数据有明确空态。
///
/// 旧「事件」域不再作为独立工作区：入口在本页左栏 footer「回顾」。
///
/// 禁 pumpAndSettle（门禁约束）；宽窄屏同一纵向流，仅边距随宽收放。
class ReviewView extends ConsumerWidget {
  const ReviewView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final granularity = ref.watch(reviewGranularityProvider);
    // 断点走令牌：平板/小窗（≥600）加宽边距；桌面（>1024）再给足呼吸感。
    final width = MediaQuery.sizeOf(context).width;
    final wide = width >= Breakpoints.phone;
    final horizontalPadding = width >= Breakpoints.desktop
        ? AppTheme.space12
        : wide
            ? AppTheme.space8
            : AppTheme.space3;
    // 粒度切换处的未读小圆点（R4 唯一提醒形态）：任一该粒度报告
    // readAt 为空 = 未读；报告面尚未加载时不显示（静默降级）。
    final weekUnread = _hasUnread(ref, 'week');
    final monthUnread = _hasUnread(ref, 'month');
    return Column(
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(
            horizontalPadding,
            12,
            horizontalPadding,
            0,
          ),
          child: Row(
            children: [
              SegmentedButton<ReviewGranularity>(
                segments: [
                  ButtonSegment(
                    value: ReviewGranularity.day,
                    label: const Text('日'),
                  ),
                  ButtonSegment(
                    value: ReviewGranularity.week,
                    label: _UnreadBadge(unread: weekUnread, child: const Text('周')),
                  ),
                  ButtonSegment(
                    value: ReviewGranularity.month,
                    label: _UnreadBadge(
                      unread: monthUnread,
                      child: const Text('月'),
                    ),
                  ),
                ],
                selected: {granularity},
                onSelectionChanged: (selection) {
                  ref
                      .read(reviewGranularityProvider.notifier)
                      .set(selection.first);
                },
              ),
              const Spacer(),
              _AnchorDateStepper(wide: wide),
            ],
          ),
        ),
        KnowledgeHealthBar(horizontalPadding: horizontalPadding),
        Expanded(
          child: switch (granularity) {
            ReviewGranularity.day => _ReviewDayView(
              horizontalPadding: horizontalPadding,
            ),
            ReviewGranularity.week => _ReviewPeriodView(
              periodType: 'week',
              horizontalPadding: horizontalPadding,
            ),
            ReviewGranularity.month => _ReviewPeriodView(
              periodType: 'month',
              horizontalPadding: horizontalPadding,
            ),
          },
        ),
      ],
    );
  }
}

/// 锚点日期翻页（日粒度 ±1 天，周/月粒度 ±7/±30 天粗步进）。
