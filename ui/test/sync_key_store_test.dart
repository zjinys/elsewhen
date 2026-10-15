import 'dart:convert';

import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';

// This fake never touches the OS keystore. Native platform acceptance is separate.
class _Storage extends FlutterSecureStorage {
  final values = <String, String>{};
  bool readFails = false;
  bool writeFails = false;
  bool losesWrite = false;
  int writes = 0;

  @override
  Future<String?> read({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    if (readFails) {
      throw PlatformException(code: 'native', message: 'SYNTHETIC_SECRET');
    }
    return values[key];
  }

  @override
  Future<void> write({
    required String key,
    required String? value,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    writes++;
    if (writeFails) {
      throw PlatformException(code: 'native', message: 'SYNTHETIC_SECRET');
    }
    if (!losesWrite) values[key] = value!;
  }
}

Matcher failsWith(SyncKeyStoreError code) => throwsA(
  isA<SyncKeyStoreException>().having((error) => error.code, 'code', code),
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'new installation keys persist and different installations differ',
    () async {
      final storage = _Storage();
      final store = SyncKeyStore(storage: storage);
      final a = await store.createForNewInstallation('installation-a');
      final reopened = await SyncKeyStore(storage: storage)
          .load('installation-a');
      expect(a.copyBytesForRust(), reopened.copyBytesForRust());
      expect(a.copyBytesForRust().length, 32);
      final b = await store.createForNewInstallation('installation-b');
      expect(a.copyBytesForRust(), isNot(equals(b.copyBytesForRust())));
      expect('$a', 'SyncMasterKey([REDACTED])');
      expect(storage.writes, 2);
      a.dispose();
      b.dispose();
      reopened.dispose();
    },
  );

  test('existing and corrupted keys are never silently replaced', () async {
    final storage = _Storage();
    final store = SyncKeyStore(storage: storage);
    final key = await store.createForNewInstallation('installation');
    final before = Map<String, String>.from(storage.values);
    await expectLater(
      store.createForNewInstallation('installation'),
      failsWith(SyncKeyStoreError.alreadyExists),
    );
    expect(storage.values, before);
    storage.values[storage.values.keys.single] = 'bad-key-SYNTHETIC_SECRET';
    await expectLater(
      store.load('installation'),
      failsWith(SyncKeyStoreError.corrupt),
    );
    await expectLater(
      store.createForNewInstallation('installation'),
      failsWith(SyncKeyStoreError.alreadyExists),
    );
    expect(storage.writes, 1);
    key.dispose();
  });

  test(
    'restored database with missing key never generates a replacement',
    () async {
      final storage = _Storage();
      await expectLater(
        SyncKeyStore(storage: storage).load('old-installation'),
        failsWith(SyncKeyStoreError.missing),
      );
      expect(storage.writes, 0);
      expect(storage.values, isEmpty);
    },
  );

  test(
    'locked or failing native stores fail closed without leaking diagnostics',
    () async {
      final storage = _Storage()..readFails = true;
      final store = SyncKeyStore(storage: storage);
      await expectLater(
        store.load('installation'),
        failsWith(SyncKeyStoreError.unavailable),
      );
      await expectLater(
        store.createForNewInstallation('installation'),
        failsWith(SyncKeyStoreError.unavailable),
      );
      expect(storage.writes, 0);
      storage.readFails = false;
      storage.writeFails = true;
      try {
        await store.createForNewInstallation('installation');
        fail('write should fail');
      } on SyncKeyStoreException catch (error) {
        expect('$error', isNot(contains('SYNTHETIC_SECRET')));
        expect(error.code, SyncKeyStoreError.unavailable);
      }
      storage.writeFails = false;
      storage.losesWrite = true;
      await expectLater(
        store.createForNewInstallation('installation'),
        failsWith(SyncKeyStoreError.unavailable),
      );
    },
  );

  test(
    'concurrent creates cannot rotate a key within the Flutter isolate',
    () async {
      final storage = _Storage();
      final first = SyncKeyStore(storage: storage)
          .createForNewInstallation('installation');
      final second = SyncKeyStore(storage: storage)
          .createForNewInstallation('installation');
      final assertion = expectLater(
        second,
        failsWith(SyncKeyStoreError.alreadyExists),
      );
      final key = await first;
      await assertion;
      expect(storage.writes, 1);
      final loaded = await SyncKeyStore(storage: storage).load('installation');
      expect(loaded.copyBytesForRust(), key.copyBytesForRust());
      key.dispose();
      loaded.dispose();
    },
  );

  test(
    'identity validation and disposed handles never expose key bytes',
    () async {
      final storage = _Storage();
      final store = SyncKeyStore(storage: storage);
      await expectLater(
        store.load('../other'),
        failsWith(SyncKeyStoreError.invalidIdentity),
      );
      await expectLater(
        store.createForNewInstallation(''),
        failsWith(SyncKeyStoreError.invalidIdentity),
      );
      expect(storage.writes, 0);
      final key = await store.createForNewInstallation('installation');
      key.dispose();
      expect(key.copyBytesForRust, failsWith(SyncKeyStoreError.disposed));
      expect('$key', 'SyncMasterKey([REDACTED])');
    },
  );

  test('wrong length and unknown secret record versions fail closed', () async {
    final storage = _Storage();
    final store = SyncKeyStore(storage: storage);
    for (final value in [
      'v2:${base64Encode(List.filled(32, 1))}',
      'v1:${base64Encode(List.filled(31, 1))}',
      'x' * 5000,
    ]) {
      storage.values['elsewhen.sync.master.v1.installation'] = value;
      await expectLater(
        store.load('installation'),
        failsWith(SyncKeyStoreError.corrupt),
      );
    }
    expect(storage.writes, 0);
  });

  test('native options disable reset-on-error and Apple cloud migration', () {
    expect(syncSecureStorage.aOptions.toMap()['resetOnError'], 'false');
    expect(
      syncSecureStorage.aOptions.toMap()['storageNamespace'],
      'elsewhen_sync_keys',
    );
    expect(syncSecureStorage.iOptions.synchronizable, isFalse);
    expect(syncSecureStorage.mOptions.synchronizable, isFalse);
    expect(
      syncSecureStorage.iOptions.accessibility,
      KeychainAccessibility.first_unlock_this_device,
    );
    expect(
      syncSecureStorage.mOptions.accessibility,
      KeychainAccessibility.first_unlock_this_device,
    );
  });
}
