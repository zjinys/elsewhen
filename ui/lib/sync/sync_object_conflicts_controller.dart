import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// 待办/目标/规则同步冲突的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::list_object_conflicts` /
/// `resolve_object_conflict`。裁决必须原样回传审阅时展示的本端
/// `updated_at`（行已删为 null），Rust 侧双绑定失败关闭。
abstract interface class SyncObjectConflictApi {
  Future<List<api.ObjectConflictDto>> listConflicts();
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  });
}

class FrbSyncObjectConflictApi implements SyncObjectConflictApi {
  const FrbSyncObjectConflictApi();

  @override
  Future<List<api.ObjectConflictDto>> listConflicts() =>
      api.listObjectConflicts();

  @override
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  }) => api.resolveObjectConflict(
    opId: opId,
    expectedLocalUpdatedAt: expectedLocalUpdatedAt,
    acceptRemote: acceptRemote,
  );
}

/// 设置页只读展示 + 显式裁决待办/目标/规则同步冲突（与 wiki 页冲突同款纪律）。
///
/// 不自动刷新；展示/裁决失败保留旧视图并显示错误，绝不静默清空列表。
/// 裁决成功后才重新拉取（该冲突应已从列表消失）。
class SyncObjectConflictsController extends ChangeNotifier {
  SyncObjectConflictsController({SyncObjectConflictApi? bridge})
    : _bridge = bridge ?? const FrbSyncObjectConflictApi();

  final SyncObjectConflictApi _bridge;

  List<api.ObjectConflictDto>? conflicts;
  String? error;
  bool busy = false;
  bool _disposed = false;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> refresh() async {
    if (busy || _disposed) return;
    busy = true;
    error = null;
    _changed();
    try {
      conflicts = await _bridge.listConflicts();
    } catch (e) {
      error = _describe(e);
    } finally {
      busy = false;
      _changed();
    }
  }

  /// 显式裁决。`conflict` 必须是用户当前审阅的同一 DTO——本端 updated_at
  /// 原样回传，不匹配则 Rust 侧失败关闭（object_conflict_local_changed）。
  Future<bool> resolve(
    api.ObjectConflictDto conflict, {
    required bool acceptRemote,
  }) async {
    if (busy || _disposed) return false;
    busy = true;
    error = null;
    _changed();
    try {
      await _bridge.resolve(
        opId: conflict.opId,
        expectedLocalUpdatedAt: conflict.localUpdatedAt,
        acceptRemote: acceptRemote,
      );
      // 成功后重拉，确认该冲突确实消失（而不是乐观地从本地移除）。
      conflicts = await _bridge.listConflicts();
      return true;
    } catch (e) {
      error = _describe(e);
      return false;
    } finally {
      busy = false;
      _changed();
    }
  }

  static String _describe(Object e) {
    final text = e.toString();
    if (text.contains('object_conflict_local_changed')) {
      return '本端记录已变化，请刷新后重新审阅。';
    }
    if (text.contains('object_conflict_not_pending')) {
      return '该冲突已裁决，请刷新列表。';
    }
    if (text.contains('goal_active_limit')) {
      return '本端活跃目标已满，无法直接采纳对端；请先归档目标或选择保留本端。';
    }
    return '操作失败：$text';
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
