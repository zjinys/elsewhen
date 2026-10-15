import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// Wiki 页同步冲突的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::list_wiki_page_conflicts` /
/// `resolve_wiki_page_conflict`。裁决必须原样回传审阅时展示的本端
/// `updated_at`（行已删为 null），Rust 侧双绑定失败关闭。
abstract interface class SyncWikiConflictApi {
  Future<List<api.WikiPageConflictDto>> listConflicts();
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  });
}

class FrbSyncWikiConflictApi implements SyncWikiConflictApi {
  const FrbSyncWikiConflictApi();

  @override
  Future<List<api.WikiPageConflictDto>> listConflicts() =>
      api.listWikiPageConflicts();

  @override
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  }) => api.resolveWikiPageConflict(
    opId: opId,
    expectedLocalUpdatedAt: expectedLocalUpdatedAt,
    acceptRemote: acceptRemote,
  );
}

/// 设置页只读展示 + 显式裁决 wiki 页同步冲突（与 Provider 冲突同款纪律）。
///
/// 不自动刷新；展示/裁决失败保留旧视图并显示错误，绝不静默清空列表。
/// 裁决成功后才重新拉取（该冲突应已从列表消失）。
class SyncWikiConflictsController extends ChangeNotifier {
  SyncWikiConflictsController({SyncWikiConflictApi? bridge})
    : _bridge = bridge ?? const FrbSyncWikiConflictApi();

  final SyncWikiConflictApi _bridge;

  List<api.WikiPageConflictDto>? conflicts;
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
  /// 原样回传，不匹配则 Rust 侧失败关闭（wiki_conflict_local_changed）。
  Future<bool> resolve(
    api.WikiPageConflictDto conflict, {
    required bool acceptRemote,
  }) async {
    if (busy || _disposed) return false;
    busy = true;
    error = null;
    _changed();
    try {
      await _bridge.resolve(
        opId: conflict.opId,
        expectedLocalUpdatedAt: conflict.local?.updatedAt,
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
    if (text.contains('wiki_conflict_local_changed')) {
      return '本端页面已变化，请刷新后重新审阅。';
    }
    if (text.contains('wiki_conflict_not_pending')) {
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
