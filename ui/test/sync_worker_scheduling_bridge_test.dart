import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
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
  String? failure;
  int processedResults = 0;
  bool syncSeenBeforeEveryAnalysis = true;
  Completer<void>? hold;
  @override
  Future<api.AnalysisTriggerResult> triggerAnalysis() async {
    calls++;
    syncSeenBeforeEveryAnalysis &= lastSyncTick.value != null;
    lastSyncTick.value = null;
    final gate = hold;
    if (gate != null) await gate.future;
    if (failure case final String message) throw StateError(message);
    if (processedResults > 0) {
      processedResults--;
      return const api.AnalysisTriggerResult.processed(count: 1);
    }
    return super.triggerAnalysis();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('sync runs before slow AI, between batches, after errors and during backoff', () async {
    Future<void> waitFor(bool Function() ready) async {
      for (var i = 0; i < 200 && !ready(); i++) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect(ready(), isTrue);
    }

    final storage = TestSyncSecureStorage();
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
    );
    final repo = await createIsolatedBridge(
      repositoryFactory: (path) => _ObservedWorker(path, controller),
    ) as _ObservedWorker;
    await waitFor(() => repo.calls > 0 && repo.lastSyncTick.value != null);

    final gate = Completer<void>();
    repo.hold = gate;
    var before = repo.calls;
    await repo.recordEvent('slow analysis');
    await waitFor(() => repo.calls > before);
    expect(repo.syncSeenBeforeEveryAnalysis, isTrue);
    repo.hold = null;
    gate.complete();
    await waitFor(() => repo.lastSyncTick.value != null);

    repo.processedResults = 3;
    before = repo.calls;
    await repo.recordEvent('bounded drain');
    await waitFor(
      () => repo.calls >= before + 4 && repo.lastSyncTick.value != null,
    );
    expect(repo.syncSeenBeforeEveryAnalysis, isTrue);

    repo.failure = 'sync_not_initialized';
    before = repo.calls;
    await repo.recordEvent('analysis fails; sync still runs');
    await waitFor(() => repo.calls > before && repo.lastSyncTick.value != null);
    final blockedCalls = repo.calls;
    repo.lastSyncTick.value = null;
    await repo.recordEvent('another capture during AI backoff');
    await waitFor(() => repo.lastSyncTick.value != null);
    expect(
      repo.calls,
      blockedCalls,
      reason: 'AI remains backed off while sync gets a turn',
    );
  });
}
