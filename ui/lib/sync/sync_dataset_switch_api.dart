import '../bridge/api.dart' as api;

/// 数据集目录切换（原生 sync-workspace.db）：列出未选中的数据集并提交选择。
///
/// 条目生命周期：加入流程建 staging → 用户 select_import 时置 ready+selected。
/// 所以「待进入」的数据集在 catalog 里永远是未选中条目（多为 staging）——
/// 不能用 ready 过滤，否则完成加入后永远查不出可切换项。
///
/// 注意：原生 select_sync_dataset 只写 catalog 的 selected_id，当前进程 pin 着的
/// 主库连接不会移动——选中后必须完全退出并重新打开应用才生效。
abstract interface class SyncDatasetSwitchApi {
  Future<List<api.SyncDatasetEntryDto>> listUnselected();
  Future<void> select(String entryId);
}

class FrbSyncDatasetSwitchApi implements SyncDatasetSwitchApi {
  const FrbSyncDatasetSwitchApi();

  @override
  Future<List<api.SyncDatasetEntryDto>> listUnselected() async {
    final (entries, _) = await api.listSyncDatasets();
    return entries.where((e) => !e.selected).toList();
  }

  @override
  Future<void> select(String entryId) =>
      api.selectSyncDataset(entryId: entryId);
}
