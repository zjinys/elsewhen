import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_enrollment_controller.dart';
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/owner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/sync_pairing_steps.dart';

import 'support/sync_setup.dart';

api.SyncEnrollmentStateDto enrollmentState({
  required String role,
  required String phase,
  required int received,
  required int total,
}) => api.SyncEnrollmentStateDto(
  role: role,
  operationId: 'op-1',
  invitationId: 'inv-1',
  installationId: 'test-install',
  revision: 2,
  confirmationToken: null,
  phase: phase,
  snapshotId: 'snap-1',
  receivedChunks: received,
  totalChunks: total,
  localApplied: false,
  remoteApplied: false,
  notificationPending: false,
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('SyncSnapshotProgress renders deterministic fraction', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SyncSnapshotProgress(received: 2, total: 4),
        ),
      ),
    );
    expect(find.text('初始快照传输'), findsOneWidget);
    expect(find.text('2/4'), findsOneWidget);
    final indicator = tester.widget<LinearProgressIndicator>(
      find.byType(LinearProgressIndicator),
    );
    expect(indicator.value, closeTo(0.5, 0.0001));
  });

  testWidgets(
    'owner card shows sending progress from owner enrollment states',
    (tester) async {
      final storage = TestSyncSecureStorage();
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: TestSyncSetupApi(storage),
        previewEnabled: true,
      );
      addTearDown(controller.dispose);
      // 直接注入邀请与 enrollment 状态：不联网、不入会话，只验渲染联动。
      controller.invitation = const api.SyncInvitationOfferDto(
        invitationId: 'inv-1',
        code: 'SYNTHETIC',
        expiresAtMs: 0,
      );
      controller.enrollment.states = [
        enrollmentState(
          role: 'owner',
          phase: 'sending_snapshot',
          received: 2,
          total: 5,
        ),
      ];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: OwnerSyncPairingSettings(controller: controller),
            ),
          ),
        ),
      );
      await tester.pump();
      expect(find.text('正在发送初始快照'), findsOneWidget);
      expect(find.text('2/5'), findsOneWidget);
      final indicator = tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(indicator.value, closeTo(0.4, 0.0001));
    },
  );

  testWidgets(
    'joiner card shows receiving progress from joiner enrollment states',
    (tester) async {
      final storage = TestSyncSecureStorage();
      final enrollment = SyncEnrollmentController(
        keyStore: SyncKeyStore(storage: storage),
      );
      final joiner = SyncJoinController(keyStore: SyncKeyStore(storage: storage), enrollment: enrollment);
      addTearDown(joiner.dispose);
      enrollment.states = [
        enrollmentState(
          role: 'joiner',
          phase: 'receiving_snapshot',
          received: 3,
          total: 7,
        ),
      ];
      joiner.state = const api.SyncJoinStateDto(
        operationId: 'op-1',
        token: 't',
        datasetId: 'ds',
        ownerName: 'owner',
        ownerFingerprint: 'ab',
        relayUrl: 'https://relay.test',
        expiresAtMs: 0,
        phase: 'enrollment',
        revision: 2,
        creating: false,
        approvalAckPending: false,
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: JoinerSyncPairingSettings(controller: joiner),
            ),
          ),
        ),
      );
      await tester.pump();
      expect(find.text('正在接收初始快照'), findsOneWidget);
      expect(find.text('3/7'), findsOneWidget);
      final indicator = tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(indicator.value, closeTo(3 / 7, 0.0001));
    },
  );
}
