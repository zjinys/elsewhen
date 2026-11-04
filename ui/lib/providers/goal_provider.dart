import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/goal.dart';
import '../bridge/rust_bridge_repository.dart';
import 'state_holder.dart';

/// 目标页选中的目标 ID：左栏工作台清单（GoalWorkbenchPanel）与主区
/// 详情/讨论面板共享，两栏渲染同一目标。null = 未选中（主区显占位指引）。
final selectedGoalIdProvider =
    NotifierProvider<StateHolder<String?>, String?>(
      () => StateHolder(null),
    );

/// 目标页的状态过滤（活跃/暂停搁置/已达成），多选、缺省看活跃；
/// 由左栏工作台清单持有，主区详情只读。不允许清空为空集
/// （保底逻辑见 [toggleGoalStatusFilter]）。
final goalStatusFilterProvider =
    NotifierProvider<StateHolder<Set<GoalStatusFilter>>, Set<GoalStatusFilter>>(
      () => StateHolder(const {GoalStatusFilter.active}),
    );

/// 目标清单的状态过滤枚举（含中文标签，供胶囊直接用）。
enum GoalStatusFilter {
  active('活跃'),
  paused('暂停搁置'),
  completed('已达成');

  const GoalStatusFilter(this.label);
  final String label;
}

/// 切换某个状态的选中态（多选）。最后一个选中项不可被取消：
/// 过滤集至少保留一项，避免出现「什么都不显示」的死状态。
void toggleGoalStatusFilter(WidgetRef ref, GoalStatusFilter filter) {
  ref.read(goalStatusFilterProvider.notifier).update((current) {
    final next = Set<GoalStatusFilter>.of(current);
    if (!next.remove(filter)) {
      next.add(filter);
    }
    return next.isEmpty ? current : next;
  });
}

/// 航向偏离校准提醒（Compass Nudge，FR-PES-005-04）。
/// 订阅 `RustBridgeRepository.activeCompassNudge` 的实时值。
final activeCompassNudgeProvider = Provider<GoalCompassNudge?>((ref) {
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return null;
  final nudgeNotifier = repo.activeCompassNudge;
  void onChange() => ref.invalidateSelf();
  nudgeNotifier.addListener(onChange);
  ref.onDispose(() => nudgeNotifier.removeListener(onChange));
  return nudgeNotifier.value;
});

/// 活跃目标（最多 3 条，按近期→中期→长远排序）
final activeGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listActiveGoals();
});

/// 已归档目标（历史所有非活跃，含暂停与达成）
final archivedGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listArchivedGoals();
});

/// 暂停搁置的目标（可随时唤醒复活）
final pausedGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listPausedGoals();
});

/// 已达成的目标（成就勋章）
final completedGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listCompletedGoals();
});
