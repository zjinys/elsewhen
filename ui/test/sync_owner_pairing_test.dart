import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_pairing_api.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';

import 'support/sync_setup.dart';

class PairingSetup extends TestSyncSetupApi {
  PairingSetup(super.storage);
  bool pairingConfirmed = false;
  @override
  Future<api.LocalSyncStateDto> confirm(String token, int revision) async {
    if (token == 'pair-commit') {
      if (revision != 2 ||
          storage.values['elsewhen.sync.revision.v1.test-install'] != 'v1:2') {
        throw StateError('native revision must precede pairing confirmation');
      }
      pairingConfirmed = true;
      return current = testSyncState('local_ready');
    }
    return super.confirm(token, revision);
  }
}

class PairingApi implements SyncPairingApi {
  PairingApi(this.setup);
  final PairingSetup setup;
  int issued = 0;
  int approved = 0;
  int sent = 0;
  bool failSend = false;
  bool failCancel = false;
  String? approvedToken;
  final preview = api.SyncInvitationPreviewDto(
    invitationId: 'invite',
    token: 'displayed-token',
    deviceName: '加入的设备',
    devicePlatform: 'test',
    fingerprint: 'ab' * 32,
    expiresAtMs: DateTime.now()
        .add(const Duration(minutes: 1))
        .millisecondsSinceEpoch,
  );

  @override
  Future<api.SyncInvitationOfferDto> issue(
    String relayUrl,
    bool consent,
  ) async {
    if (!consent) throw StateError('consent required');
    issued++;
    return api.SyncInvitationOfferDto(
      invitationId: 'invite',
      code: 'SYNTHETIC_INVITATION',
      expiresAtMs: preview.expiresAtMs,
    );
  }

  @override
  Future<api.SyncInvitationPreviewDto?> inspect(String invitationId) async =>
      preview;
  @override
  Future<api.SyncKeyCommitDto> approve(
    String invitationId,
    String previewToken,
    bool consent,
  ) async {
    if (!consent) throw StateError('consent required');
    approved++;
    approvedToken = previewToken;
    setup.current = testSyncState('locked');
    return const api.SyncKeyCommitDto(
      token: 'pair-commit',
      installationId: 'test-install',
      revision: 2,
    );
  }

  @override
  Future<void> sendApproval(String invitationId) async {
    if (!setup.pairingConfirmed ||
        setup.storage.values['elsewhen.sync.revision.v1.test-install'] !=
            'v1:2') {
      throw StateError('must not send before native confirmation');
    }
    sent++;
    if (failSend) {
      throw StateError('sync_pairing_delivery_unconfirmed SYNTHETIC_SECRET');
    }
  }

  @override
  Future<void> cancel(String invitationId) async {
    if (failCancel) {
      throw StateError(
        'sync_invitation_revocation_unconfirmed SYNTHETIC_SECRET',
      );
    }
  }
}

class Fixture {
  final storage = TestSyncSecureStorage();
  late final setup = PairingSetup(storage);
  late final pairing = PairingApi(setup);
  late final controller = SyncSetupController(
    keyStore: SyncKeyStore(storage: storage),
    bridge: setup,
    pairingBridge: pairing,
    previewEnabled: true,
  );
  Future<void> initialize() =>
      controller.createOwner('owner', 'test', consent: true);
  Future<void> invite() async {
    await controller.createInvitation('http://localhost:9876', consent: true);
    await controller.inspectInvitation();
  }

  Future<void> close() async {
    await controller.close();
    controller.dispose();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'invitation and approval each require disclosure and reuse displayed token',
    () async {
      final f = Fixture();
      await f.initialize();
      await f.controller.createInvitation(
        'http://localhost:9876',
        consent: false,
      );
      expect(f.pairing.issued, 0);
      await f.invite();
      final displayed = f.controller.pairingPreview!;
      await f.controller.approveInvitation(displayed, consent: false);
      expect(f.pairing.approved, 0);
      // A modal may still show the earlier preview; never fetch a new token for it.
      f.controller.pairingPreview = api.SyncInvitationPreviewDto(
        invitationId: 'other',
        token: 'new-token',
        deviceName: 'other',
        devicePlatform: 'test',
        fingerprint: 'cd',
        expiresAtMs: displayed.expiresAtMs,
      );
      await f.controller.approveInvitation(displayed, consent: true);
      expect(f.pairing.approvedToken, 'displayed-token');
      expect(f.pairing.sent, 1);
      expect(
        f.storage.values['elsewhen.sync.revision.v1.test-install'],
        'v1:2',
      );
      expect(f.controller.pairingNotice, contains('不表示设备已加入'));
      expect(f.controller.invitation, isNull);
      await f.close();
    },
  );

  test(
    'native persistence failure releases ownership and never sends approval',
    () async {
      final f = Fixture();
      await f.initialize();
      await f.invite();
      f.storage.failWrites = true;
      await f.controller.approveInvitation(
        f.controller.pairingPreview!,
        consent: true,
      );
      expect(f.pairing.approved, 1);
      expect(f.pairing.sent, 0);
      expect(f.setup.pairingConfirmed, isFalse);
      expect(f.setup.released, ['request-1']);
      expect(f.controller.state?.state, 'locked');
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      await f.close();
    },
  );

  test(
    'close waits for pending native mutation before releasing the lease',
    () async {
      final f = Fixture();
      await f.initialize();
      await f.invite();
      final gate = Completer<void>();
      f.storage.writeGate = gate.future;
      final previousWrites = f.storage.writes;
      final approval = f.controller.approveInvitation(
        f.controller.pairingPreview!,
        consent: true,
      );
      await Future<void>.delayed(Duration.zero);
      expect(f.storage.writes, previousWrites + 1);
      final closing = f.controller.close();
      await Future<void>.delayed(Duration.zero);
      expect(f.setup.released, isEmpty);
      gate.complete();
      await approval;
      await closing;
      expect(f.setup.released, ['request-1']);
      expect(f.pairing.sent, 0);
      expect(f.setup.pairingConfirmed, isFalse);
      f.controller.dispose();
    },
  );

  test(
    'lost delivery response retries saved approval, not the approval action',
    () async {
      final f = Fixture();
      await f.initialize();
      await f.invite();
      f.pairing.failSend = true;
      await f.controller.approveInvitation(
        f.controller.pairingPreview!,
        consent: true,
      );
      expect(f.controller.error, contains('投递未确认'));
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      expect(f.controller.pendingPairingId, 'invite');
      f.pairing.failSend = false;
      await f.controller.resendPairingApproval();
      expect(f.pairing.approved, 1);
      expect(f.pairing.sent, 2);
      expect(f.setup.released, isEmpty);
      await f.close();
    },
  );

  test(
    'uncertain cancellation hides the code without claiming relay revocation',
    () async {
      final f = Fixture();
      await f.initialize();
      await f.invite();
      f.pairing.failCancel = true;
      await f.controller.cancelInvitation();
      expect(f.controller.invitation, isNull);
      expect(f.controller.pairingPreview, isNull);
      expect(f.controller.error, contains('中继撤销尚未确认'));
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      await f.close();
    },
  );

  testWidgets(
    'owner invitation review fits phone width and requires explicit checkbox',
    (tester) async {
      tester.view.physicalSize = const Size(360, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final f = Fixture();
      await f.initialize();
      await f.invite();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: LocalSyncSettings(controller: f.controller),
              ),
            ),
          ),
        ),
      );
      final review = find.text('核对并审批');
      await tester.ensureVisible(review);
      await tester.tap(review);
      await tester.pumpAndSettle();
      final approveButton = find.widgetWithText(FilledButton, '批准这台设备');
      expect(tester.widget<FilledButton>(approveButton).onPressed, isNull);
      expect(find.textContaining('公钥指纹'), findsOneWidget);
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.byType(CheckboxListTile),
        ),
      );
      await tester.pump();
      await tester.tap(approveButton);
      await tester.pumpAndSettle();
      expect(f.pairing.approved, 1);
      expect(f.pairing.sent, 1);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await f.close();
    },
  );
}
