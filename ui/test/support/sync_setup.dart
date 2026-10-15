import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

/// Test-only storage: no OS keychain access, no persistent secrets.
class TestSyncSecureStorage extends FlutterSecureStorage {
  final values = <String, String>{};
  Future<void>? writeGate;
  bool failWrites = false;
  bool failReads = false;
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
    if (failReads) {
      throw PlatformException(code: 'test', message: 'SYNTHETIC_SECRET');
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
    await writeGate;
    if (failWrites) {
      throw PlatformException(code: 'test', message: 'SYNTHETIC_SECRET');
    }
    values[key] = value!;
  }

  @override
  Future<void> delete({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    values.remove(key);
  }
}

api.LocalSyncStateDto testSyncState(String state) => api.LocalSyncStateDto(
  state: state,
  installationId: state == 'not_configured' ? null : 'test-install',
  vaultRevision: state == 'not_configured' ? 0 : 1,
  datasetId: state == 'not_configured' ? null : 'test-dataset',
  deviceId: state == 'not_configured' ? null : 'test-device',
  deviceName: state == 'not_configured' ? null : '测试设备',
  isOwner: state != 'not_configured',
  isProcessor: state != 'not_configured',
  activePeers: 0,
  pendingOperations: 0,
);

class TestSyncSetupApi implements SyncSetupApi {
  TestSyncSetupApi(this.storage);
  final TestSyncSecureStorage storage;
  api.LocalSyncStateDto current = testSyncState('not_configured');
  int preparations = 0;
  int installations = 0;
  int confirmations = 0;
  String? operation;
  final released = <String>[];

  @override
  Future<api.LocalSyncStateDto> state() async => current;
  @override
  Future<api.SyncKeyRequestDto> prepare({
    String? name,
    String? platform,
    required bool consent,
  }) async {
    if (current.state == 'not_configured' && !consent) {
      throw StateError('consent required');
    }
    operation = 'request-${++preparations}';
    return api.SyncKeyRequestDto(
      token: operation!,
      installationId: 'test-install',
      revision: 1,
      creating: current.state == 'not_configured',
    );
  }

  @override
  Future<api.SyncKeyCommitDto> install(
    String token,
    List<int> key,
    int revision,
  ) async {
    if (token != operation ||
        key.length != 32 ||
        revision != 1 ||
        storage.values['elsewhen.sync.revision.v1.test-install'] != 'v1:1') {
      throw StateError('protected storage must precede activation');
    }
    installations++;
    current = testSyncState('locked');
    return const api.SyncKeyCommitDto(
      token: 'commit',
      installationId: 'test-install',
      revision: 1,
    );
  }

  @override
  Future<api.LocalSyncStateDto> confirm(String token, int revision) async {
    if (token != 'commit' || revision != 1) {
      throw StateError('invalid confirmation');
    }
    confirmations++;
    return current = testSyncState('local_ready');
  }

  @override
  Future<bool> release(String token) async {
    released.add(token);
    if (operation != token) return false;
    operation = null;
    if (current.state != 'not_configured') current = testSyncState('locked');
    return true;
  }

  @override
  Future<api.LocalSyncStateDto> lock() async {
    operation = null;
    return current = testSyncState('locked');
  }

  String? renameError;
  final List<(String, String)> renames = [];

  @override
  Future<int> renameDevice(
    String deviceId,
    String name,
    int vaultRevision,
  ) async {
    if (renameError != null) throw Exception(renameError);
    renames.add((deviceId, name));
    final next = testSyncState(current.state);
    current = api.LocalSyncStateDto(
      state: next.state,
      installationId: next.installationId,
      vaultRevision: next.vaultRevision + 1,
      datasetId: next.datasetId,
      deviceId: next.deviceId,
      deviceName: name,
      isOwner: next.isOwner,
      isProcessor: next.isProcessor,
      activePeers: next.activePeers,
      pendingOperations: next.pendingOperations,
    );
    return current.vaultRevision;
  }
}
