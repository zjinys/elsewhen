import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/rule.dart';
import 'state_holder.dart';
import 'wiki_provider.dart';

/// 事件域（遗留收口切片 C）：主工作区第三态。事件是知识库的源材料，
/// 不是平级第二大陆——列表为读面入口，D2 跳转（wiki 页来源事件）与
/// D1 命中标记（详情页）在此承载。

/// 左栏 footer 切换事件域时置顶选中（null = 显示列表未选态）。
final selectedEventIdProvider = NotifierProvider<StateHolder<String?>, String?>(
  () => StateHolder(null),
);

/// 最近事件列表（不拖分析结果；days 是窗口边界，默认全量最近 50 条）。
final recentEventsProvider = FutureProvider<List<api.EventSummaryDto>>((
  ref,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listRecentEvents(days: 3650, limit: 50);
});

/// 单事件分析详情（含 guidance/emotion/项目人物等派生面）。
final eventAnalysisDetailProvider =
    FutureProvider.family<api.EventAnalysisDetailDto?, String>((ref, eventId) {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.getEventAnalysisDetail(eventId);
    });

/// 事件旁标记（D1 旧账）：该事件的未拒绝规则命中行。
final eventRuleHitsProvider = FutureProvider.family<List<RuleHit>, String>((
  ref,
  eventId,
) {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return bridge.listEventRuleHits(eventId);
});

/// 供 wiki 页「来源事件记录」跳转（D2 旧账）：切到事件域并选中。
void openEventDetail(WidgetRef ref, String eventId) {
  ref.read(selectedEventIdProvider.notifier).set(eventId);
  ref.read(sidebarTabProvider.notifier).set(SidebarTab.events);
}
