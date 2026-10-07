import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

import 'support/sync_setup.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('creation needs preview and consent, then stores revision before activation', () async {
    final storage = TestSyncSecureStorage();
    final bridge = TestSyncSetupApi(storage);
    final disabled = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: bridge,
      previewEnabled: false,
    );
    await disabled.createOwner('owner', 'test', consent: true);
    expect(bridge.preparations, 0);
    disabled.dispose();
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: bridge,
      previewEnabled: true,
    );
    await controller.createOwner('owner', 'test', consent: false);
    expect(bridge.preparations, 0);
    await controller.createOwner('owner', 'test', consent: true);
    expect(controller.error, isNull);
    expect(controller.state?.state, 'local_ready');
    expect(bridge.installations, 1);
    expect(bridge.confirmations, 1);
    expect(storage.values['elsewhen.sync.revision.v1.test-install'], 'v1:1');
    await controller.lock();
    expect(controller.state?.state, 'locked');
    await controller.unlock();
    expect(controller.state?.state, 'local_ready');
    expect(bridge.preparations, 2);
    await controller.close();
    controller.dispose();
  });

  test(
    'native failure does not initialize the DB or leak key diagnostics',
    () async {
      final storage = TestSyncSecureStorage()..failWrites = true;
      final bridge = TestSyncSetupApi(storage);
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
        previewEnabled: true,
      );
      await controller.createOwner('owner', 'test', consent: true);
      expect(bridge.installations, 0);
      expect(bridge.confirmations, 0);
      expect(controller.error, isNot(contains('SYNTHETIC_SECRET')));
      expect(bridge.released, ['request-1']);
      controller.dispose();
    },
  );

  test(
    'closing waits for native writes and cancels only its own request',
    () async {
      final gate = Completer<void>();
      final storage = TestSyncSecureStorage()..writeGate = gate.future;
      final bridge = TestSyncSetupApi(storage);
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
        previewEnabled: true,
      );
      final activation = controller.createOwner('owner', 'test', consent: true);
      await Future<void>.delayed(Duration.zero);
      expect(storage.writes, 1);
      final closing = controller.close();
      await Future<void>.delayed(Duration.zero);
      expect(
        bridge.released,
        isEmpty,
        reason: 'the Rust lease must cover the pending native write',
      );
      gate.complete();
      await activation;
      await closing;
      expect(bridge.installations, 0);
      expect(bridge.released, ['request-1']);
      expect(
        storage.values.containsKey('elsewhen.sync.revision.v1.test-install'),
        isFalse,
      );
      controller.dispose();
    },
  );

  test(
    'late close from an older controller cannot lock a newer session',
    () async {
      final storage = TestSyncSecureStorage();
      final bridge = TestSyncSetupApi(storage);
      final first = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
        previewEnabled: true,
      );
      await first.createOwner('owner', 'test', consent: true);
      await bridge.lock();
      final second = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
      );
      await second.restoreIfConfigured();
      expect(second.state?.state, 'local_ready');
      await first.close();
      expect(bridge.current.state, 'local_ready');
      expect(bridge.operation, 'request-2');
      await second.close();
      first.dispose();
      second.dispose();
    },
  );

  test(
    'missing protected revision is not reconstructed from database metadata',
    () async {
      final storage = TestSyncSecureStorage();
      final keys = SyncKeyStore(storage: storage);
      final key = await keys.createForNewInstallation('test-install');
      key.dispose();
      final bridge = TestSyncSetupApi(storage)
        ..current = testSyncState('locked');
      final controller = SyncSetupController(keyStore: keys, bridge: bridge);
      await controller.restoreIfConfigured();
      expect(bridge.installations, 0);
      expect(controller.error, contains('受保护的修订记录'));
      expect(
        storage.values.containsKey('elsewhen.sync.revision.v1.test-install'),
        isFalse,
      );
      controller.dispose();
    },
  );

  test(
    'protected revision cannot roll back or skip missing initial state',
    () async {
      final storage = TestSyncSecureStorage();
      final keys = SyncKeyStore(storage: storage);
      final key = await keys.createForNewInstallation('install');
      key.dispose();
      await expectLater(
        keys.saveRevision('install', 2),
        throwsA(isA<SyncKeyStoreException>()),
      );
      await keys.saveRevision('install', 1);
      await keys.saveRevision('install', 3);
      await expectLater(
        keys.saveRevision('install', 2),
        throwsA(
          isA<SyncKeyStoreException>().having(
            (e) => e.code,
            'code',
            SyncKeyStoreError.revisionRollback,
          ),
        ),
      );
      expect(await keys.readRevision('install'), 3);
      storage.values['elsewhen.sync.revision.v1.install'] = 'v1:03';
      await expectLater(
        keys.readRevision('install'),
        throwsA(isA<SyncKeyStoreException>()),
      );
    },
  );
}
