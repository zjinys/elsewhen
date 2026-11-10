import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/sync/sync_dataset_cleanup_api.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/widgets/dataset_cleanup_card.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';
import 'support/sync_setup.dart';

api.SyncDatasetCleanupDto preview({
  bool pending = false,
  String installation = 'old-install',
}) => api.SyncDatasetCleanupDto(
  entryId: 'old-copy',
  installationId: installation,
  name: '旧副本 Old copy',
  bytes: BigInt.from(4096),
  catalogRevision: 7,
  fileToken: 'reviewed-files',
  pending: pending,
);

class CleanupFake implements SyncDatasetCleanupApi {
  List<api.SyncDatasetCleanupDto> entries = [preview()];
  final removed = <api.SyncDatasetCleanupDto>[];
  bool fail = false;
  @override
  Future<List<api.SyncDatasetCleanupDto>> list() async => entries;
  @override
  Future<void> remove(api.SyncDatasetCleanupDto p) async {
    removed.add(p);
    if (fail) {
      entries = [preview(pending: true)];
      throw StateError('files removed, key store unavailable');
    }
    entries = [];
  }
}

class CleanupBridgeFake implements SyncDatasetCleanupBridge {
  final committed = preview(installation: 'committed-install');
  final finished = <String>[];
  bool failFiles = false;
  api.SyncDatasetCleanupDto? displayed;
  @override
  Future<List<api.SyncDatasetCleanupDto>> list() async => [committed];
  @override
  Future<api.SyncDatasetCleanupDto> removeFiles(
    api.SyncDatasetCleanupDto p,
  ) async {
    displayed = p;
    if (failFiles) throw StateError('database busy');
    return committed;
  }

  @override
  Future<void> finish(String id) async {
    finished.add(id);
  }
}

class CleanupStorage extends TestSyncSecureStorage {
  bool ignoreDelete = false;
  bool failDelete = false;
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
    if (failDelete) {
      throw PlatformException(code: 'denied', message: 'SYNTHETIC_SECRET');
    }
    if (!ignoreDelete) values.remove(key);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  for (final locale in ['zh', 'en']) {
    testWidgets(
      'cleanup $locale: reviewed confirmation, cancel is a no-op, exact preview deletion at 390px',
      (tester) async {
        tester.view.physicalSize = const Size(390, 844);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final fake = CleanupFake();
        final strings = lookupAppLocalizations(Locale(locale));
        await tester.pumpWidget(
          MaterialApp(
            locale: Locale(locale),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: SingleChildScrollView(child: DatasetCleanupCard(api: fake)),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(fake.removed, isEmpty);
        await tester.tap(find.text(strings.datasetCleanupReview));
        await tester.pumpAndSettle();
        expect(
          find.textContaining(strings.datasetCleanupWarning),
          findsOneWidget,
        );
        expect(find.textContaining('old-copy'), findsOneWidget);
        await tester.tap(find.text(strings.cancel));
        await tester.pumpAndSettle();
        expect(fake.removed, isEmpty);
        final displayed = fake.entries.single;
        await tester.tap(find.text(strings.datasetCleanupReview));
        await tester.pumpAndSettle();
        await tester.tap(find.text(strings.datasetCleanupDelete));
        await tester.pumpAndSettle();
        expect(fake.removed, [same(displayed)]);
        expect(find.text(strings.datasetCleanupEmpty), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'partial failure reloads the pending job and supports explicit retry',
    (tester) async {
      final fake = CleanupFake()..fail = true;
      var catalogRefreshes = 0;
      final strings = lookupAppLocalizations(const Locale('en'));
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: DatasetCleanupCard(
              api: fake,
              onChanged: () => catalogRefreshes++,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.datasetCleanupReview));
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.datasetCleanupDelete));
      await tester.pumpAndSettle();
      expect(
        catalogRefreshes,
        1,
        reason: 'partial deletion must refresh dataset selection immediately',
      );
      expect(
        find.textContaining(strings.datasetCleanupPending),
        findsOneWidget,
      );
      expect(find.textContaining(strings.datasetCleanupFailed), findsOneWidget);
      fake.fail = false;
      await tester.tap(find.text(strings.datasetCleanupRetry));
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.datasetCleanupDelete));
      await tester.pumpAndSettle();
      expect(fake.removed.last.pending, isTrue);
      expect(catalogRefreshes, 2);
      expect(find.text(strings.datasetCleanupEmpty), findsOneWidget);
    },
  );

  test('native key cleanup follows the committed job; failure cannot acknowledge it', () async {
    final storage = CleanupStorage();
    final keys = SyncKeyStore(storage: storage);
    for (final id in ['committed-install', 'old-install', 'current-install']) {
      (await keys.createForNewInstallation(id)).dispose();
      await keys.saveRevision(id, 1);
    }
    final bridge = CleanupBridgeFake();
    final service = FrbSyncDatasetCleanupApi(keys: keys, bridge: bridge);
    final p = preview();
    bridge.failFiles = true;
    await expectLater(service.remove(p), throwsStateError);
    expect(storage.values.length, 6);
    expect(bridge.finished, isEmpty);
    bridge.failFiles = false;
    storage.failDelete = true;
    await expectLater(service.remove(p), throwsA(isA<SyncKeyStoreException>()));
    expect(bridge.finished, isEmpty);
    storage.failDelete = false;
    storage.ignoreDelete = true;
    await expectLater(service.remove(p), throwsA(isA<SyncKeyStoreException>()));
    expect(
      bridge.finished,
      isEmpty,
      reason: 'native deletion must be verified',
    );
    storage.ignoreDelete = false;
    await service.remove(p);
    expect(bridge.displayed, same(p));
    expect(bridge.finished, ['old-copy']);
    expect(
      storage.values.keys.any((k) => k.endsWith('committed-install')),
      isFalse,
    );
    expect(
      storage.values.length,
      4,
      reason: 'current and unrelated installation keys survive',
    );
  });

  test('real isolated bridge lists no cleanup candidates and refuses arbitrary paths', () async {
    await createIsolatedBridge();
    // The helper owns a temporary workspace; never access a personal database.
    expect(await api.listSyncDatasetCleanup(), isEmpty);
    await expectLater(
      api.cleanupSyncDatasetCopy(
        entryId: '../elsewhen.db',
        expectedCatalogRevision: 1,
        expectedFileToken: '',
      ),
      throwsA(anything),
    );
    expect(await api.listEvents(), isEmpty);
  });
}
