import '../bridge/api.dart' as api;

abstract interface class SyncJoinApi {
  Future<api.SyncJoinStateDto> preview(String invitationCode);
  Future<api.SyncJoinStateDto> prepare(
    String token,
    String name,
    String platform,
    bool consent,
  );
  Future<List<api.SyncJoinEntryDto>> pending();
  Future<api.SyncJoinStateDto> resume(String entryId);
  Future<api.SyncJoinStateDto> install(
    String token,
    List<int> key,
    int revision,
  );
  Future<api.SyncJoinStateDto> confirm(String token, int revision);
  Future<api.SyncJoinStateDto> state(String operationId);
  Future<api.SyncJoinStateDto> sendRequest(String operationId);
  Future<api.SyncJoinStateDto> inspect(String operationId);
  Future<api.SyncJoinStateDto> sendProof(String operationId);
  Future<bool> release(String operationId);
}

class FrbSyncJoinApi implements SyncJoinApi {
  @override
  Future<api.SyncJoinStateDto> preview(String code) =>
      api.previewSyncJoin(invitationCode: code);
  @override
  Future<api.SyncJoinStateDto> prepare(
    String token,
    String name,
    String platform,
    bool consent,
  ) => api.prepareSyncJoinKey(
    previewToken: token,
    name: name,
    platform: platform,
    acknowledgeProviderKeySharing: consent,
  );
  @override
  Future<List<api.SyncJoinEntryDto>> pending() => api.listPendingSyncJoins();
  @override
  Future<api.SyncJoinStateDto> resume(String entryId) =>
      api.prepareSyncJoinResume(entryId: entryId);
  @override
  Future<api.SyncJoinStateDto> install(
    String token,
    List<int> key,
    int revision,
  ) => api.installSyncJoinKey(
    token: token,
    key: key,
    protectedRevision: revision,
  );
  @override
  Future<api.SyncJoinStateDto> confirm(String token, int revision) =>
      api.confirmSyncJoinKey(token: token, protectedRevision: revision);
  @override
  Future<api.SyncJoinStateDto> state(String operationId) =>
      api.getSyncJoinState(operationId: operationId);
  @override
  Future<api.SyncJoinStateDto> sendRequest(String operationId) =>
      api.sendSyncJoinRequest(operationId: operationId);
  @override
  Future<api.SyncJoinStateDto> inspect(String operationId) =>
      api.inspectSyncJoinApproval(operationId: operationId);
  @override
  Future<api.SyncJoinStateDto> sendProof(String operationId) =>
      api.sendSyncJoinProof(operationId: operationId);
  @override
  Future<bool> release(String operationId) =>
      api.releaseSyncJoin(operationId: operationId);
}
