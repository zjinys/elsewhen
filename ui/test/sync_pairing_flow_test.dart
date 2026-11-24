import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/widgets/app_error_notice.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';
import 'package:elsewhen_ui/widgets/owner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/joiner_qr_scan_page.dart';
import 'package:elsewhen_ui/widgets/sync_pairing_steps.dart';

import 'sync_owner_pairing_test.dart' as owner;
import 'sync_pairing_terminal_test.dart' as terminal;
import 'support/sync_setup.dart';
import 'support/sync_join.dart';

Widget host(Widget child) => MaterialApp(
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

class DelayedApprovalApi extends TestSyncJoinApi {
  DelayedApprovalApi(super.storage);
  Completer<void>? inspectGate;
  bool approvalAvailable = false;
  bool failInspect = false;
  int runningInspections = 0;
  int maxInspections = 0;

  @override
  Future<api.SyncJoinStateDto> inspect(String operationId) async {
    runningInspections++;
    if (runningInspections > maxInspections) {
      maxInspections = runningInspections;
    }
    try {
      await inspectGate?.future;
      if (failInspect) {
        inspections++;
        throw StateError('sync_relay_unavailable');
      }
      if (!approvalAvailable) {
        inspections++;
        return current;
      }
      return await super.inspect(operationId);
    } finally {
      runningInspections--;
    }
  }
}

void main() {
  testWidgets('approval keeps this flow progress, storage and error visible', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.invite();
    await tester.pumpWidget(
      host(OwnerSyncPairingSettings(controller: f.controller)),
    );
    await f.controller.approveInvitation(
      f.controller.pairingPreview!,
      consent: true,
    );
    f.controller.enrollment.states = [
      const api.SyncEnrollmentStateDto(
        role: 'owner',
        operationId: 'request-1',
        invitationId: 'invite',
        installationId: 'test-install',
        revision: 2,
        phase: 'sending_snapshot',
        receivedChunks: 2,
        totalChunks: 5,
        localApplied: true,
        remoteApplied: false,
        notificationPending: false,
        storageBlocked: true,
      ),
    ];
    f.controller.enrollment.error = 'SYNTHETIC_ENROLLMENT_FAILURE';
    await tester.pump();
    expect(f.controller.invitation, isNull);
    expect(find.byType(SyncPairingSteps), findsOneWidget);
    expect(find.text('40% · 2/5'), findsOneWidget);
    expect(find.byType(AppErrorMessage), findsOneWidget);
    expect(find.textContaining('中继信箱已满'), findsOneWidget);
    expect(find.byType(TextField), findsNothing);
    expect(find.text('加入的设备'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets(
    'another invitation cannot supply completion or transfer progress',
    (tester) async {
      final f = owner.Fixture();
      await f.initialize();
      f.controller.invitation = api.SyncInvitationOfferDto(
        invitationId: 'new-invitation',
        code: 'SYNTHETIC',
        expiresAtMs: DateTime.now()
            .add(const Duration(minutes: 5))
            .millisecondsSinceEpoch,
      );
      f.controller.enrollment.states = [
        terminal.enrollmentState(role: 'owner', phase: 'complete'),
        terminal.enrollmentState(role: 'owner', phase: 'sending_snapshot'),
      ];
      await tester.pumpWidget(
        host(OwnerSyncPairingSettings(controller: f.controller)),
      );
      expect(
        tester.widget<SyncPairingSteps>(find.byType(SyncPairingSteps)).current,
        0,
      );
      expect(find.byType(SyncSnapshotProgress), findsNothing);
      expect(find.byType(QrImageView), findsOneWidget);
      expect(find.text('再连接一台设备'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      await f.close();
    },
  );

  testWidgets('intent survives preparation and does not expose join controls', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.controller.refresh();
    await tester.pumpWidget(
      host(
        LocalSyncSettings(
          controller: f.controller,
          datasetApi: terminal.NoopDatasetSwitchApi(),
        ),
      ),
    );
    expect(find.byType(TextField), findsNothing);
    await tester.tap(find.text('连接新设备'));
    await tester.pump();
    await f.initialize();
    await tester.pump();
    expect(find.byType(OwnerSyncPairingSettings), findsOneWidget);
    expect(find.text('锁定本机密钥'), findsNothing);
    expect(find.text('粘贴邀请'), findsNothing);
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets('active owner flow reopens in pairing after remount', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.invite();
    await tester.pumpWidget(
      host(
        LocalSyncSettings(
          controller: f.controller,
          datasetApi: terminal.NoopDatasetSwitchApi(),
        ),
      ),
    );
    expect(find.byType(OwnerSyncPairingSettings), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets('expired invitation hides QR and offers regeneration', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    f.controller.invitation = const api.SyncInvitationOfferDto(
      invitationId: 'expired',
      code: 'SYNTHETIC',
      expiresAtMs: 1,
    );
    await tester.pumpWidget(
      host(OwnerSyncPairingSettings(controller: f.controller)),
    );
    expect(find.byType(QrImageView), findsNothing);
    expect(find.text('重新生成邀请'), findsOneWidget);
    expect(find.textContaining('邀请已过期'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets('owner checks requests without an open pairing widget', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.controller.createInvitation('http://localhost:9876', consent: true);
    expect(f.controller.pairingPreview, isNull);
    await tester.pump(const Duration(seconds: 3));
    expect(f.controller.pairingPreview?.deviceName, '加入的设备');
    await f.close();
  });

  testWidgets('owner retries saved delivery without repeating approval', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.invite();
    f.pairing.failSend = true;
    await f.controller.approveInvitation(
      f.controller.pairingPreview!,
      consent: true,
    );
    expect(f.controller.approvalDelivered, isFalse);
    f.pairing.failSend = false;
    await tester.pump(const Duration(seconds: 3));
    expect(f.controller.approvalDelivered, isTrue);
    expect(f.pairing.approved, 1);
    expect(f.pairing.sent, 2);
    await f.close();
  });

  testWidgets(
    'protected storage failure cannot be retried as approval delivery',
    (tester) async {
      final f = owner.Fixture();
      await f.initialize();
      await f.invite();
      f.storage.failWrites = true;
      await f.controller.approveInvitation(
        f.controller.pairingPreview!,
        consent: true,
      );
      await f.controller.resendPairingApproval();
      await tester.pump(const Duration(seconds: 12));
      expect(f.controller.approvalReady, isFalse);
      expect(f.pairing.sent, 0);
      await f.close();
    },
  );

  testWidgets('other handoff errors do not leak into current transfer', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.invite();
    await f.controller.approveInvitation(
      f.controller.pairingPreview!,
      consent: true,
    );
    f.controller.enrollment.error = 'SYNTHETIC_OTHER_FLOW';
    f.controller.enrollment.errorInvitationId = 'older-invitation';
    await tester.pumpWidget(
      host(OwnerSyncPairingSettings(controller: f.controller)),
    );
    expect(find.byType(AppErrorMessage), findsNothing);
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets('approved transfer ignores expiry of original invitation', (
    tester,
  ) async {
    final f = owner.Fixture();
    await f.initialize();
    await f.invite();
    await f.controller.approveInvitation(
      f.controller.pairingPreview!,
      consent: true,
    );
    f.controller.invitation = const api.SyncInvitationOfferDto(
      invitationId: 'invite',
      code: 'SYNTHETIC',
      expiresAtMs: 1,
    );
    await tester.pumpWidget(
      host(OwnerSyncPairingSettings(controller: f.controller)),
    );
    expect(find.text('重新生成邀请'), findsNothing);
    expect(find.textContaining('邀请已过期'), findsNothing);
    expect(
      tester.widget<SyncPairingSteps>(find.byType(SyncPairingSteps)).current,
      2,
    );
    await tester.pumpWidget(const SizedBox());
    await f.close();
  });

  testWidgets('unknown snapshot total has no fabricated percent', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(const SyncSnapshotProgress(received: 0, total: 0)),
    );
    expect(
      tester
          .widget<LinearProgressIndicator>(find.byType(LinearProgressIndicator))
          .value,
      isNull,
    );
    expect(find.textContaining('0/0'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });

  for (final scenario in [
    'approval',
    'retry',
    'proof',
    'cancel',
    'close',
    'late-close',
  ]) {
    testWidgets('automatic join coordination: $scenario', (tester) async {
      final storage = TestSyncSecureStorage();
      final bridge = DelayedApprovalApi(storage);
      final c = SyncJoinController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
      );
      await c.preview('SYNTHETIC');
      await c.join(c.state!, 'phone', 'test', consent: true);
      if (scenario == 'cancel') {
        await c.cancel();
      } else if (scenario == 'close') {
        await c.close();
      } else if (scenario == 'late-close') {
        bridge.approvalAvailable = true;
        bridge.inspectGate = Completer<void>();
        await tester.pump(const Duration(seconds: 3));
        await tester.pump(const Duration(seconds: 15));
        expect(bridge.maxInspections, 1);
        final closing = c.close();
        bridge.inspectGate!.complete();
        await closing;
      } else {
        if (scenario == 'retry') bridge.failInspect = true;
        if (scenario == 'proof') {
          bridge.approvalAvailable = true;
          bridge.failProof = true;
        }
        await tester.pump(const Duration(seconds: 3));
        expect(bridge.inspections, 1);
        if (scenario == 'approval') {
          expect(c.state?.phase, 'awaiting_approval');
          expect(bridge.proofs, 0);
        }
        bridge.failInspect = false;
        bridge.failProof = false;
        bridge.approvalAvailable = true;
        await tester.pump(const Duration(seconds: 3));
        expect(c.state?.phase, 'proof_sent');
        expect(bridge.confirmations, 2);
        expect(bridge.installations, 1);
        expect(bridge.proofs, scenario == 'proof' ? 2 : 1);
      }
      if (const ['cancel', 'close', 'late-close'].contains(scenario)) {
        await tester.pump(const Duration(seconds: 12));
        expect(bridge.proofs, 0);
        expect(bridge.released, ['join-operation']);
      }
      await c.close();
      c.dispose();
    });
  }

  testWidgets('scanner delivers once across different results and rebuilds', (
    tester,
  ) async {
    final detected = <String>[];
    late void Function(String) deliver;
    final pageKey = GlobalKey();
    Widget scanner() => MaterialApp(
      home: JoinerQrScanPage(
        key: pageKey,
        onDetected: detected.add,
        scannerBuilder: (context, onCode) {
          deliver = onCode;
          return const SizedBox();
        },
      ),
    );
    await tester.pumpWidget(scanner());
    deliver('FIRST');
    deliver('SECOND');
    await tester.pumpWidget(scanner());
    deliver('THIRD');
    expect(detected, ['FIRST']);
  });
}
