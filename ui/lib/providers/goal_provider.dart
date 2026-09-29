import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/goal.dart';
import '../bridge/rust_bridge_repository.dart';

/// 活跃目标（最多 3 条，按近期→中期→长远排序）
final activeGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listActiveGoals();
});

/// 已归档目标（历史）
final archivedGoalsProvider = FutureProvider<List<Goal>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listArchivedGoals();
});
