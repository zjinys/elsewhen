import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

import 'support/isolated_bridge.dart';
import 'support/sync_setup.dart';

api.AiProviderConfigDto edited(
  api.AiProviderConfigDto original, {
  required String url,
  String key = '',
}) => api.AiProviderConfigDto(
  id: original.id,
  name: original.name,
  providerType: original.providerType,
  baseUrl: url,
  model: original.model,
  apiKeySource: original.apiKeySource,
  hasApiKey: true,
  apiKey: key,
  isActive: original.isActive,
  temperature: original.temperature,
  maxTokens: original.maxTokens,
  contextWindow: original.contextWindow,
  revisionToken: original.revisionToken,
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('real bridge prepares owner, locks and unlocks without replacing local data', () async {
    final storage = TestSyncSecureStorage();
    final keys = SyncKeyStore(storage: storage);
    final controller = SyncSetupController(
      keyStore: keys,
      previewEnabled: true,
    );
    final repo = await createIsolatedBridge(syncSetupController: controller);
    expect(controller.state?.state, 'not_configured');
    expect(storage.writes, 0);
    await repo.recordEvent('before local sync setup');
    await api.updateAiProviderConfig(
      baseUrl: 'https://synthetic.invalid/v1',
      model: 'synthetic',
      apiKey: 'SYNTHETIC_PROVIDER_SEALING_KEY',
    );
    await expectLater(
      api.prepareSyncKeyRequest(
        name: 'owner',
        platform: 'test',
        acknowledgeProviderKeySharing: false,
      ),
      throwsA(anything),
    );
    await controller.createOwner('isolated device', 'test', consent: true);
    expect(controller.error, isNull);
    expect(controller.state?.state, 'local_ready');
    expect(controller.state?.deviceName, 'isolated device');
    expect(controller.state?.isOwner, isTrue);
    expect(controller.state?.activePeers, 0);
    final installation = controller.state!.installationId!;
    expect(await keys.readRevision(installation), 1);
    final originalMaster =
        storage.values['elsewhen.sync.master.v1.$installation'];
    await controller.lock();
    expect(controller.state?.state, 'locked');
    final metadata = (await api.getAiProviderConfig())!;
    expect(metadata.apiKey, '');
    expect(metadata.baseUrl, 'https://synthetic.invalid/v1');
    await expectLater(
      api.saveAiProviderConfig(
        provider: edited(
          metadata,
          url: 'https://other.invalid',
          key: 'replacement',
        ),
      ),
      throwsA(anything),
    );
    expect(
      (await api.getAiProviderConfig())?.baseUrl,
      'https://synthetic.invalid/v1',
    );
    await repo.recordEvent('while local key is locked');
    final paused = await api.triggerAnalysis();
    expect(paused, isA<api.AnalysisTriggerResult_Processed>());
    expect((paused as api.AnalysisTriggerResult_Processed).count, 0);
    await controller.unlock();
    expect(controller.error, isNull);
    expect(controller.state?.state, 'local_ready');
    expect(
      storage.values['elsewhen.sync.master.v1.$installation'],
      originalMaster,
    );
    await api.saveAiProviderConfig(
      provider: edited(
        metadata,
        url: 'https://new-config.invalid',
        key: 'NEW_SYNTHETIC_KEY',
      ),
    );
    await expectLater(
      api.saveAiProviderConfig(
        provider: edited(metadata, url: 'https://synthetic.invalid/v1'),
      ),
      throwsA(anything),
    );
    await expectLater(
      api.setActiveAiProviderConfig(
        id: metadata.id,
        revisionToken: metadata.revisionToken,
      ),
      throwsA(anything),
    );
    await expectLater(
      api.deleteAiProviderConfig(
        id: metadata.id,
        revisionToken: metadata.revisionToken,
      ),
      throwsA(anything),
    );
    expect(
      (await api.getAiProviderConfig())?.baseUrl,
      'https://new-config.invalid',
    );
    await controller.lock();
    storage.values.remove('elsewhen.sync.revision.v1.$installation');
    await controller.unlock();
    expect(controller.error, contains('受保护的修订记录'));
    expect((await api.getLocalSyncState()).state, 'locked');
    expect((await repo.listEvents()).length, 2);
    expect(
      storage.values['elsewhen.sync.master.v1.$installation'],
      originalMaster,
    );
  });
}
