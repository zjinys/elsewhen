import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_enrollment_controller.dart';
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/sync/sync_dataset_switch_api.dart';
import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/owner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/sync_pairing_steps.dart';

import 'support/sync_setup.dart';

/// 终态必须能被画出来：五步全 ✔、不出现转圈、不出现「当前步」的 ◀ 标记。
///
/// 真机 2026-10-28 10:10 的缺陷是「快照已 applied、名册已激活，界面仍在最后
/// 一步转圈」—— 因为映射表最大只到 4，最后一步只能落在「进行中」。这些用例钉住
/// 两件事：①`joined` 映射到**全部完成**；②渲染结果里确实没有进行中标志。
void expectAllStepsDone(WidgetTester tester, {required int steps}) {
  expect(
    find.byIcon(Icons.check_circle),
    findsNWidgets(steps),
    reason: '每一步都必须是完成态',
  );
  expect(
    find.byType(CircularProgressIndicator),
    findsNothing,
    reason: '终态里不许再有「进行中」的转圈',
  );
  expect(
    find.byIcon(Icons.radio_button_unchecked),
    findsNothing,
    reason: '终态里不许还有未开始的步骤',
  );
  expect(find.text('◀'), findsNothing, reason: '终态里不该再指向「当前步」');
}

api.SyncEnrollmentStateDto enrollmentState({
  required String role,
  required String phase,
}) => api.SyncEnrollmentStateDto(
  role: role,
  operationId: 'op-1',
  invitationId: 'inv-1',
  installationId: 'inst-1',
  revision: 2,
  confirmationToken: null,
  phase: phase,
  snapshotId: 'snap-1',
  receivedChunks: 3,
  totalChunks: 3,
  localApplied: true,
  remoteApplied: true,
  notificationPending: false,
  storageBlocked: false,
);

class NoopDatasetSwitchApi implements SyncDatasetSwitchApi {
  @override
  Future<List<api.SyncDatasetEntryDto>> listUnselected() async => const [];
  @override
  Future<void> select(String entryId) async {}
}

api.SyncJoinStateDto joinState(String phase) => api.SyncJoinStateDto(
  operationId: 'op-1',
  token: 't',
  datasetId: 'ds-1',
  ownerName: 'owner',
  ownerFingerprint: 'ab',
  relayUrl: 'https://relay.test',
  expiresAtMs: 0,
  phase: phase,
  revision: 6,
  creating: false,
  approvalAckPending: false,
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('joined is mapped to "all steps done", not to the last running step', () {
    // 五步：'joined' 必须等于 steps.length（=5），否则第五步只能是「进行中」。
    expect(joinerStageFor('joined'), 5);
    // 既有映射不变。
    expect(joinerStageFor('preview'), 0);
    expect(joinerStageFor('awaiting_membership'), 4);
    expect(joinerStageFor('enrollment_confirmation'), 4);
    // 未知/未开始仍是 -1。注意 `requires_new_invitation`/`expired` 也走这一支。
    expect(joinerStageFor('expired'), -1);
  });

  test('owner stage reaches all-done only through deviceJoined', () {
    const running = 4;
    expect(
      ownerPairingStage(
        invitationId: 'inv-1',
        hasPreview: true,
        pendingPairingId: 'pending-1',
        approvalDelivered: true,
      ),
      running,
    );
    expect(
      ownerPairingStage(
        invitationId: 'inv-1',
        hasPreview: true,
        pendingPairingId: 'pending-1',
        approvalDelivered: true,
        deviceJoined: true,
      ),
      5,
    );
  });

  testWidgets('joiner card renders the finished flow without a spinner', (
    tester,
  ) async {
    final enrollment = SyncEnrollmentController(keyStore: SyncKeyStore());
    final joiner = SyncJoinController(enrollment: enrollment);
    addTearDown(joiner.dispose);
    joiner.state = joinState('joined');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: JoinerSyncPairingSettings(
              controller: joiner,
              datasetApi: NoopDatasetSwitchApi(),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expectAllStepsDone(tester, steps: 5);
    expect(find.text('等待成员授权与初始同步'), findsOneWidget);
  });

  testWidgets('owner card renders the finished flow without a spinner', (
    tester,
  ) async {
    final storage = TestSyncSecureStorage();
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: TestSyncSetupApi(storage),
      previewEnabled: true,
    );
    addTearDown(controller.dispose);
    controller.invitation = const api.SyncInvitationOfferDto(
      invitationId: 'inv-1',
      code: 'SYNTHETIC',
      expiresAtMs: 0,
    );
    controller.enrollment.states = [
      enrollmentState(role: 'owner', phase: 'complete'),
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
    await tester.pump();

    expectAllStepsDone(tester, steps: 5);
    expect(find.text('新设备接入（在「管理」确认设备列表）'), findsOneWidget);
  });
}
