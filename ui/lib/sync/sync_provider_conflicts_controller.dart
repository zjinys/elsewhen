import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// Provider 同步冲突的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::list_sync_provider_conflicts` /
/// `resolve_sync_provider_conflict`。DTO 只携带掩码 Key；裁决必须原样回传
/// 审阅时展示的 `localRevision` / `revisionId`，Rust 侧双版本绑定失败关闭。
abstract interface class SyncProviderConflictApi {
  Future<List<api.ProviderSyncConflictDto>> listConflicts();
  Future<void> resolve({
    required String revisionId,
    required String expectedLocalRevision,
    required String expectedRemoteRevision,
    required bool acceptRemote,
  });
}

class FrbSyncProviderConflictApi implements SyncProviderConflictApi {
  const FrbSyncProviderConflictApi();

  @override
  Future<List<api.ProviderSyncConflictDto>> listConflicts() =>
      api.listSyncProviderConflicts();

  @override
  Future<void> resolve({
    required String revisionId,
    required String expectedLocalRevision,
    required String expectedRemoteRevision,
    required bool acceptRemote,
  }) => api.resolveSyncProviderConflict(
    revisionId: revisionId,
    expectedLocalRevision: expectedLocalRevision,
    expectedRemoteRevision: expectedRemoteRevision,
    acceptRemote: acceptRemote,
  );
}

/// 设置页只读展示 + 显式裁决 Provider 同步冲突。
///
/// 不自动刷新（避免打断审阅）；展示/裁决失败保留旧视图并显示错误，绝不
/// 静默清空列表。裁决成功后才重新拉取（此时该冲突应已从列表消失）。
class SyncProviderConflictsController extends ChangeNotifier {
  SyncProviderConflictsController({SyncProviderConflictApi? bridge})
    : _bridge = bridge ?? const FrbSyncProviderConflictApi();

  final SyncProviderConflictApi _bridge;

  List<api.ProviderSyncConflictDto>? conflicts;
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
      // 保留旧视图；锁定/密封未开等失败关闭信息原样展示。
      error = _describe(e);
    } finally {
      busy = false;
      _changed();
    }
  }

  /// 显式裁决。`conflict` 必须是用户当前审阅的同一 DTO——本地/远端版本
  /// 原样回传，任一不匹配 Rust 侧失败关闭（`provider_conflict_local_changed`
  /// / `provider_conflict_revision_mismatch`），本控制器原样上抛错误。
  Future<bool> resolve(
    api.ProviderSyncConflictDto conflict, {
    required bool acceptRemote,
  }) async {
    if (busy || _disposed) return false;
    busy = true;
    error = null;
    _changed();
    try {
      await _bridge.resolve(
        revisionId: conflict.revisionId,
        expectedLocalRevision: conflict.localRevision,
        expectedRemoteRevision: conflict.revisionId,
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
    if (text.contains('provider_key_locked')) {
      return '本机同步密钥已锁定：请先在上方解锁后审阅冲突。';
    }
    if (text.contains('provider_conflict_local_changed')) {
      return '本地配置已变化，请刷新后重新审阅。';
    }
    if (text.contains('provider_conflict_revision_mismatch')) {
      return '冲突内容已变化，请刷新后重新审阅。';
    }
    if (text.contains('provider_conflict_not_pending')) {
      return '该冲突已裁决，请刷新列表。';
    }
    return '操作失败：$text';
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
