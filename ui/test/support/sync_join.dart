import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_join_api.dart';

import 'sync_setup.dart';

api.SyncJoinStateDto testJoinState(
  String phase, {
  String operation = 'join-operation',
  String token = 'preview-token',
  int revision = 1,
  bool creating = false,
  String? entry = 'join-entry',
}) => api.SyncJoinStateDto(
  operationId: operation,
  token: token,
  entryId: entry,
  installationId: entry == null ? null : 'join-install',
  datasetId: 'shared-dataset',
  ownerName: '可信已有设备',
  ownerFingerprint: 'ab' * 32,
  localFingerprint: phase == 'preview' ? null : 'cd' * 32,
  relayUrl: 'http://localhost:9876',
  expiresAtMs: DateTime.now()
      .add(const Duration(minutes: 1))
      .millisecondsSinceEpoch,
  phase: phase,
  revision: revision,
  creating: creating,
  approvalAckPending: false,
);

const testJoinEntry = api.SyncJoinEntryDto(
  entryId: 'join-entry',
  installationId: 'join-install',
  datasetId: 'shared-dataset',
  name: '暂存设备',
);

class TestSyncJoinApi implements SyncJoinApi {
  TestSyncJoinApi(this.storage);
  final TestSyncSecureStorage storage;
  api.SyncJoinStateDto current = testJoinState(
    'preview',
    revision: 0,
    creating: true,
    entry: null,
  );
  int previews = 0;
  int preparations = 0;
  int installations = 0;
  int confirmations = 0;
  int requests = 0;
  int inspections = 0;
  int proofs = 0;
  bool nativeConfirmed = false;
  bool checkpoint = false;
  bool failProof = false;
  bool wrongInstallTarget = false;
  String? preparedToken;
  final released = <String>[];
  Future<void>? previewGate;

  @override
  Future<api.SyncJoinStateDto> preview(String invitationCode) async {
    previews++;
    await previewGate;
    return current = testJoinState(
      'preview',
      revision: 0,
      creating: true,
      entry: null,
    );
  }

  @override
  Future<api.SyncJoinStateDto> prepare(
    String token,
    String name,
    String platform,
    bool consent,
  ) async {
    if (!consent || token != 'preview-token') {
      throw StateError('sync_join_preview_stale');
    }
    preparedToken = token;
    preparations++;
    return current = testJoinState(
      'key_required',
      token: 'key-request',
      creating: true,
    );
  }

  @override
  Future<List<api.SyncJoinEntryDto>> pending() async => [testJoinEntry];
  @override
  Future<api.SyncJoinStateDto> resume(String entryId) async {
    if (!checkpoint || entryId != testJoinEntry.entryId) {
      throw StateError('sync_join_requires_new_invitation');
    }
    nativeConfirmed = false;
    return current = testJoinState(
      'key_required',
      token: 'resume-key',
      revision: 2,
    );
  }

  @override
  Future<api.SyncJoinStateDto> install(
    String token,
    List<int> key,
    int revision,
  ) async {
    if (key.length != 32 ||
        token != current.token ||
        revision < 1 ||
        storage.values['elsewhen.sync.revision.v1.join-install'] !=
            'v1:$revision') {
      throw StateError('native key and revision must precede installation');
    }
    installations++;
    return current = testJoinState(
      'key_confirmation',
      token: checkpoint ? 'native-2' : 'native-1',
      revision: checkpoint ? 2 : 1,
      entry: wrongInstallTarget ? 'other-entry' : 'join-entry',
    );
  }

  @override
  Future<api.SyncJoinStateDto> confirm(String token, int revision) async {
    if (token != current.token ||
        revision != current.revision ||
        storage.values['elsewhen.sync.revision.v1.join-install'] !=
            'v1:$revision') {
      throw StateError('native revision must precede confirmation');
    }
    nativeConfirmed = true;
    confirmations++;
    return current = testJoinState(
      revision == 1 ? 'ready_to_request' : 'awaiting_membership',
      token: token,
      revision: revision,
    );
  }

  @override
  Future<api.SyncJoinStateDto> state(String operationId) async => current;
  @override
  Future<api.SyncJoinStateDto> sendRequest(String operationId) async {
    if (!nativeConfirmed || current.revision != 1) {
      throw StateError('request sent before native confirmation');
    }
    requests++;
    return current = testJoinState('awaiting_approval', token: 'native-1');
  }

  @override
  Future<api.SyncJoinStateDto> inspect(String operationId) async {
    inspections++;
    checkpoint = true;
    nativeConfirmed = false;
    return current = testJoinState(
      'installation_confirmation',
      token: 'native-2',
      revision: 2,
    );
  }

  @override
  Future<api.SyncJoinStateDto> sendProof(String operationId) async {
    if (!nativeConfirmed ||
        storage.values['elsewhen.sync.revision.v1.join-install'] != 'v1:2') {
      throw StateError('proof sent before native confirmation');
    }
    proofs++;
    if (failProof) {
      throw StateError('sync_join_proof_delivery_unconfirmed SYNTHETIC_SECRET');
    }
    return current = testJoinState(
      'proof_sent',
      token: 'native-2',
      revision: 2,
    );
  }

  @override
  Future<bool> release(String operationId) async {
    released.add(operationId);
    return operationId == current.operationId;
  }
}
