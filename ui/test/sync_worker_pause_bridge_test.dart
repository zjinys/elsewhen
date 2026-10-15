import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

import 'support/isolated_bridge.dart';
import 'support/sync_setup.dart';

class _ObservedWorker extends RustBridgeRepository {
  _ObservedWorker(String path, SyncSetupController controller)
    : super(
        databasePath: path,
        runBackgroundWorker: true,
        runChatQueueWorker: false,
        syncSetupController: controller,
      );
  int calls = 0;
  @override
  Future<api.AnalysisTriggerResult> triggerAnalysis() async {
    calls++;
    return super.triggerAnalysis();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test(
    'zero processed while locked cannot busy-loop over durable pending jobs',
    () async {
      final storage = TestSyncSecureStorage();
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        previewEnabled: true,
      );
      final repo = await createIsolatedBridge(
        repositoryFactory: (path) => _ObservedWorker(path, controller),
      ) as _ObservedWorker;
      // Let the startup NoProvider tick finish before acquiring its workspace lease.
      await Future<void>.delayed(const Duration(milliseconds: 150));
      for (var attempt = 0; attempt < 10; attempt++) {
        await controller.createOwner('worker-test', 'test', consent: true);
        if (controller.state?.state == 'local_ready') break;
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      expect(controller.state?.state, 'local_ready');
      await controller.lock();
      final before = repo.calls;
      await repo.recordEvent('saved while locked');
      await Future<void>.delayed(const Duration(milliseconds: 400));
      expect(
        repo.calls - before,
        inInclusiveRange(1, 3),
        reason: 'Processed(0) must stop the drain even with pending rows',
      );
      final count = repo.calls;
      await Future<void>.delayed(const Duration(milliseconds: 200));
      expect(repo.calls, count);
      expect((await repo.getAnalysisJobStats()).pending, 1);
    },
  );
}
