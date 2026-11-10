import '../bridge/api.dart' as api;
import 'sync_key_store.dart';

abstract interface class SyncDatasetCleanupApi {
  Future<List<api.SyncDatasetCleanupDto>> list();
  Future<void> remove(api.SyncDatasetCleanupDto preview);
}

abstract interface class SyncDatasetCleanupBridge {
  Future<List<api.SyncDatasetCleanupDto>> list();
  Future<api.SyncDatasetCleanupDto> removeFiles(
    api.SyncDatasetCleanupDto preview,
  );
  Future<void> finish(String entryId);
}

class FrbSyncDatasetCleanupBridge implements SyncDatasetCleanupBridge {
  const FrbSyncDatasetCleanupBridge();
  @override
  Future<List<api.SyncDatasetCleanupDto>> list() =>
      api.listSyncDatasetCleanup();
  @override
  Future<api.SyncDatasetCleanupDto> removeFiles(
    api.SyncDatasetCleanupDto preview,
  ) => api.cleanupSyncDatasetCopy(
    entryId: preview.entryId,
    expectedCatalogRevision: preview.catalogRevision,
    expectedFileToken: preview.fileToken,
  );
  @override
  Future<void> finish(String entryId) =>
      api.finishSyncDatasetCleanup(entryId: entryId);
}

class FrbSyncDatasetCleanupApi implements SyncDatasetCleanupApi {
  FrbSyncDatasetCleanupApi({
    SyncKeyStore? keys,
    this.bridge = const FrbSyncDatasetCleanupBridge(),
  }) : _keys = keys ?? SyncKeyStore();
  final SyncDatasetCleanupBridge bridge;
  final SyncKeyStore _keys;

  @override
  Future<List<api.SyncDatasetCleanupDto>> list() => bridge.list();

  @override
  Future<void> remove(api.SyncDatasetCleanupDto preview) async {
    final job = await bridge.removeFiles(preview);
    // Use the committed job's installation, never an editable UI identity.
    // Failure leaves the native cleanup journal pending across app restarts.
    await _keys.deleteInstallation(job.installationId);
    await bridge.finish(job.entryId);
  }
}
