import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// 名册设备列表 + owner 撤销的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::get_sync_roster` / `revoke_sync_member`。
/// 撤销必须绑定 UI 审阅时所见的名册修订号，Rust 侧失败关闭。
abstract interface class SyncRosterApi {
  Future<api.SyncRosterDto> getRoster();

  Future<void> revokeMember({
    required String deviceId,
    required int expectedRevision,
  });
}

class FrbSyncRosterApi implements SyncRosterApi {
  const FrbSyncRosterApi();

  @override
  Future<api.SyncRosterDto> getRoster() => api.getSyncRoster();

  @override
  Future<void> revokeMember({
    required String deviceId,
    required int expectedRevision,
  }) => api.revokeSyncMember(
    deviceId: deviceId,
    expectedRevision: expectedRevision,
  );
}

/// 设置页的设备列表与撤销入口。
///
/// 列表对所有成员可读（含已撤销审计）；撤销按钮只对 owner 展示，且仅针对
/// 活跃的非 owner、非加工设备、非本机成员。加工设备必须先移交（Rust 侧
/// `sync_membership_handoff_required` 硬拦）。失败保留旧视图并显示错误。
class SyncRosterController extends ChangeNotifier {
  SyncRosterController({SyncRosterApi? bridge})
    : _bridge = bridge ?? const FrbSyncRosterApi();

  final SyncRosterApi _bridge;

  api.SyncRosterDto? roster;
  String? error;
  bool busy = false;

  /// 待确认撤销目标（对话框展示審閱信息；与最终提交绑定同一修订号）。
  api.SyncRosterMemberDto? pendingRevoke;
  bool _disposed = false;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> refresh() async {
    try {
      roster = await _bridge.getRoster();
      error = null;
    } catch (e) {
      error = rosterErrorText(e.toString());
    }
    _changed();
  }

  /// 打开撤销确认（绑定审阅时的修订号）。
  void beginRevoke(api.SyncRosterMemberDto member) {
    pendingRevoke = member;
    _changed();
  }

  void cancelRevoke() {
    pendingRevoke = null;
    _changed();
  }

  /// 提交撤销；成功后刷新名册（该成员应变为非活跃）。
  Future<void> confirmRevoke() async {
    final member = pendingRevoke;
    if (member == null) return;
    final revision = roster?.revision;
    if (revision == null) return;
    busy = true;
    _changed();
    try {
      await _bridge.revokeMember(
        deviceId: member.deviceId,
        expectedRevision: revision,
      );
      pendingRevoke = null;
      await refresh();
    } catch (e) {
      error = rosterErrorText(e.toString());
    } finally {
      busy = false;
      _changed();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

/// 名册/撤销错误码 → 设置页可读文案；未识别的原样透传。
String rosterErrorText(String raw) {
  String? matched;
  for (final entry in _rosterErrors.entries) {
    if (raw.contains(entry.key)) {
      matched = entry.value;
      break;
    }
  }
  return matched ?? raw;
}

const Map<String, String> _rosterErrors = {
  'sync_membership_handoff_required': '该设备当前负责 AI 分析，必须先移交职责才能撤销。',
  'sync_membership_revision_changed': '名册刚被其他设备更新，请刷新后重新审阅撤销。',
  'sync_membership_owner_required': '只有 owner 设备才能撤销成员。',
  'sync_membership_already_revoked': '该设备已被撤销。',
  'sync_membership_member_missing': '该设备不在名册中。',
  'sync_membership_revision_invalid': '名册修订号无效。',
  'sync_membership_limit': '名册成员数已达上限。',
  'sync_membership_identity_conflict': '名册设备身份冲突（重复密钥或流）。',
  'sync_runtime_locked': '本机同步密钥已锁定，请先解锁后再操作。',
  'sync_not_initialized': '本机尚未准备同步身份。',
};
