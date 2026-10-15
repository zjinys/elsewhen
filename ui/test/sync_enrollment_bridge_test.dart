import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

import 'support/isolated_bridge.dart';
import 'support/sync_setup.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'enrollment FRB remains scoped to an unlocked isolated operation',
    () async {
      final storage = TestSyncSecureStorage();
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        previewEnabled: true,
      );
      final repo = await createIsolatedBridge(syncSetupController: controller);
      expect(await api.listSyncEnrollments(), isEmpty);
      await repo.localSync.createOwner(
        'synthetic owner',
        'test',
        consent: true,
      );
      expect(repo.localSync.error, isNull);
      expect(await api.listSyncEnrollments(), isEmpty);
      await expectLater(
        api.advanceSyncEnrollment(
          role: 'owner',
          operationId: 'stale-operation',
          invitationId: 'unknown-invitation',
        ),
        throwsA(anything),
      );
      await expectLater(
        api.confirmSyncEnrollment(
          role: 'joiner',
          operationId: 'stale-operation',
          invitationId: 'unknown-invitation',
          token: 'stale-token',
          protectedRevision: 1,
        ),
        throwsA(anything),
      );
      expect((await api.getLocalSyncState()).state, 'local_ready');
      expect(
        await api.releaseSyncKeyRequest(token: 'stale-operation'),
        isFalse,
      );
      expect((await api.getLocalSyncState()).state, 'local_ready');
      await repo.localSync.lock();
      expect(await api.listSyncEnrollments(), isEmpty);
    },
  );
}
