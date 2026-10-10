import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

import 'support/isolated_bridge.dart';
import 'support/sync_setup.dart';

String syntheticInvitation() =>
    'elsewhen-pair-v1:${base64Url.encode(utf8.encode(jsonEncode({
      'context': {
        'version': 1,
        'invitation_id': 'isolated-join-invitation',
        'dataset_id': 'other-dataset',
        'issuer': {'device_id': 'synthetic-owner', 'stream_id': 'synthetic-owner-stream', 'name': 'Synthetic owner', 'platform': 'test', 'signing_key': List.filled(32, 7)},
        'expires_at_ms': DateTime.now().add(const Duration(minutes: 1)).millisecondsSinceEpoch,
        'route': {'relay_url': 'http://localhost:9876', 'issuer_mailbox': 'b' * 64, 'request_sender': 'a' * 64},
      },
      'secret': List.filled(32, 9),
    }))).replaceAll('=', '')}';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('real join bridge stages without changing the pinned database or using the network', () async {
    final storage = TestSyncSecureStorage();
    final keys = SyncKeyStore(storage: storage);
    final setup = SyncSetupController(keyStore: keys, previewEnabled: true);
    final repo = await createIsolatedBridge(syncSetupController: setup);
    await repo.recordEvent('original dataset fact');
    await api.updateAiProviderConfig(
      baseUrl: 'https://synthetic.invalid/v1',
      model: 'original-only',
      apiKey: 'SYNTHETIC_LOCAL_KEY',
    );
    final original = await api.getAiProviderConfig();
    final preview = await api.previewSyncJoin(
      invitationCode: syntheticInvitation(),
    );
    String operation = preview.operationId;
    addTearDown(() => api.releaseSyncJoin(operationId: operation));
    expect(preview.phase, 'preview');
    expect(await api.listPendingSyncJoins(), isEmpty);
    await expectLater(
      api.prepareSyncJoinKey(
        previewToken: preview.token,
        name: 'joiner',
        platform: 'test',
        acknowledgeProviderKeySharing: false,
      ),
      throwsA(anything),
    );
    final request = await api.prepareSyncJoinKey(
      previewToken: preview.token,
      name: 'joiner',
      platform: 'test',
      acknowledgeProviderKeySharing: true,
    );
    final key = await keys.createForNewInstallation(request.installationId!);
    addTearDown(key.dispose);
    await keys.saveRevision(request.installationId!, 1);
    final bytes = key.copyBytesForRust();
    late final api.SyncJoinStateDto commit;
    try {
      commit = await api.installSyncJoinKey(
        token: request.token,
        key: bytes,
        protectedRevision: 1,
      );
    } finally {
      bytes.fillRange(0, bytes.length, 0);
    }
    // This failure occurs before any HTTP request; no local port is probed.
    await expectLater(
      api.sendSyncJoinRequest(operationId: operation),
      throwsA(anything),
    );
    await expectLater(
      api.confirmSyncKey(token: commit.token, protectedRevision: 1),
      throwsA(anything),
    );
    await keys.saveRevision(commit.installationId!, commit.revision);
    final ready = await api.confirmSyncJoinKey(
      token: commit.token,
      protectedRevision: await keys.readRevision(commit.installationId!),
    );
    expect(ready.phase, 'ready_to_request');
    expect(ready.localFingerprint, isNotEmpty);
    expect((await api.getLocalSyncState()).state, 'not_configured');
    expect((await api.getAiProviderConfig())?.baseUrl, original?.baseUrl);
    expect((await api.getAiProviderConfig())?.id, original?.id);
    await repo.recordEvent('capture while joining');
    expect((await repo.listEvents()).length, 2);
    expect((await api.listPendingSyncJoins()).single.entryId, request.entryId);
    await api.releaseSyncJoin(operationId: operation);
    final resumed = await api.prepareSyncJoinResume(entryId: request.entryId!);
    operation = resumed.operationId;
    expect(resumed.creating, isFalse);
    final existing = await keys.load(resumed.installationId!);
    final resumeBytes = existing.copyBytesForRust();
    try {
      await expectLater(
        api.installSyncJoinKey(
          token: resumed.token,
          key: resumeBytes,
          protectedRevision: await keys.readRevision(resumed.installationId!),
        ),
        throwsA(anything),
      );
    } finally {
      resumeBytes.fillRange(0, resumeBytes.length, 0);
      existing.dispose();
    }
    expect((await repo.listEvents()).length, 2);
    expect((await api.getAiProviderConfig())?.model, 'original-only');
  });
}
