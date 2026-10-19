import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/events_provider.dart';
import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/rule.dart';
import '../theme/app_theme.dart';

part 'events_view_parts/events_stream.dart';
part 'events_view_parts/events_detail.dart';
part 'events_view_parts/events_nav_summary.dart';
// ── 事件视图按流/详情/摘要拆分（纯类段平移，零行为变化）──

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
