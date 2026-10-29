import '../bridge/api.dart' as api;

/// 「清除本机同步身份」的原生入口（roadmap C3 / 缺陷 D1）。
///
/// 抽成接口是为了让 UI 测试不需要 frb：替身可以只记录调用次数并返回下一个状态。
abstract interface class SyncRepairApi {
  Future<api.LocalSyncStateDto> clearLocalSyncIdentity();

  /// 旧 vault master 是否还在本进程（C12）：决定确认弹窗的 API Key 文案分支。
  Future<bool> providerKeysMigratable();
}

class FrbSyncRepairApi implements SyncRepairApi {
  const FrbSyncRepairApi();

  @override
  Future<api.LocalSyncStateDto> clearLocalSyncIdentity() =>
      api.clearLocalSyncIdentity();

  @override
  Future<bool> providerKeysMigratable() => api.providerKeysMigratable();
}
