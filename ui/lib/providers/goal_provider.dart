import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/goal.dart';
import '../bridge/rust_bridge_repository.dart';

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
