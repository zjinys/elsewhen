import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_dataset_switch_api.dart';
import 'package:elsewhen_ui/sync/sync_enrollment_controller.dart';
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';

import 'support/sync_setup.dart';

import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';

/// 数据集目录替身：记录选择动作，模拟 native sync-workspace.db。
class FakeDatasetSwitchApi implements SyncDatasetSwitchApi {
  FakeDatasetSwitchApi(this.entries);
  List<api.SyncDatasetEntryDto> entries;
  final selects = <String>[];
  bool get switched => selects.isNotEmpty;

  @override
  Future<List<api.SyncDatasetEntryDto>> listUnselected() async =>
      entries.where((e) => !e.selected).toList();

  @override
  Future<void> select(String entryId) async => selects.add(entryId);
}

api.SyncDatasetEntryDto datasetEntry({
  String id = 'entry-1',
  bool ready = true,
  bool selected = false,
}) => api.SyncDatasetEntryDto(
  entryId: id,
  datasetId: 'ds-1',
  installationId: 'inst-1',
  name: '手机数据集',
  ready: ready,
  selected: selected,
);

api.SyncEnrollmentStateDto joinerComplete() => const api.SyncEnrollmentStateDto(
  role: 'joiner',
  operationId: 'op-1',
  invitationId: 'inv-1',
  installationId: 'inst-1',
  revision: 6,
  confirmationToken: null,
  phase: 'complete',
  snapshotId: 'snap-1',
  receivedChunks: 3,
  totalChunks: 3,
  localApplied: true,
  remoteApplied: true,
  notificationPending: false,
);

Future<void> pumpCard({
  required WidgetTester tester,
  required SyncJoinController controller,
  required SyncDatasetSwitchApi datasetApi,
}) async => tester.pumpWidget(
  MaterialApp(
    home: Scaffold(
      body: SingleChildScrollView(
        child: JoinerSyncPairingSettings(
          controller: controller,
          datasetApi: datasetApi,
        ),
      ),
    ),
  ),
);

void main() {
  testWidgets('joiner complete offers to enter the joined dataset', (
    tester,
  ) async {
    final enrollment = SyncEnrollmentController(keyStore: SyncKeyStore());
    final joiner = SyncJoinController(enrollment: enrollment);
    addTearDown(joiner.dispose);
    // 真实时序：加入完成后条目尚未 ready（select_import 才置位）。
    final datasetApi = FakeDatasetSwitchApi([datasetEntry(ready: false)]);
    joiner.enrollment!.states = [joinerComplete()];
    joiner.state = const api.SyncJoinStateDto(
      operationId: 'op-1',
      token: 't',
      datasetId: 'ds-1',
      ownerName: 'owner',
      ownerFingerprint: 'ab',
      relayUrl: 'https://relay.test',
      expiresAtMs: 0,
      phase: 'complete',
      revision: 6,
      creating: false,
      approvalAckPending: false,
    );

    await pumpCard(tester: tester, controller: joiner, datasetApi: datasetApi);
    await tester.pumpAndSettle();

    expect(find.textContaining('已加入数据集「手机数据集」'), findsOneWidget);
    expect(find.text('进入数据集'), findsOneWidget);
    expect(datasetApi.switched, isFalse);

    await tester.tap(find.text('进入数据集'));
    await tester.pumpAndSettle();

    expect(datasetApi.selects, ['entry-1']);
    expect(
      find.textContaining('请完全退出并重新打开应用'),
      findsOneWidget,
      reason: '选择只在下次进程启动生效，必须明确告知重启',
    );
    expect(find.text('进入数据集'), findsNothing);
  });

  /// 重启后的关键场景：配对会话已清空（joiner/enrollment 均为空状态），
  /// 切换入口必须只依赖原生 catalog 就能被发现——挂在同步设置页顶部。
  testWidgets('dataset switch card survives restart without pairing session', (
    tester,
  ) async {
    final storage = TestSyncSecureStorage();
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: TestSyncSetupApi(storage),
      previewEnabled: true,
    );
    addTearDown(controller.dispose);
    await controller.refresh();
    // 真实时序：加入完成后条目尚未 ready（select_import 才置位）。
    final datasetApi = FakeDatasetSwitchApi([datasetEntry(ready: false)]);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: LocalSyncSettings(
              controller: controller,
              datasetApi: datasetApi,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('已加入数据集「手机数据集」'), findsOneWidget);
    expect(find.text('进入数据集'), findsOneWidget);

    await tester.tap(find.text('进入数据集'));
    await tester.pumpAndSettle();
    expect(datasetApi.selects, ['entry-1']);
    expect(find.textContaining('请完全退出并重新打开应用'), findsOneWidget);
  });

  testWidgets('no pending dataset entry renders no switch card', (
    tester,
  ) async {
    final enrollment = SyncEnrollmentController(keyStore: SyncKeyStore());
    final joiner = SyncJoinController(enrollment: enrollment);
    addTearDown(joiner.dispose);
    final datasetApi = FakeDatasetSwitchApi(const []);
    joiner.enrollment!.states = [joinerComplete()];
    joiner.state = const api.SyncJoinStateDto(
      operationId: 'op-1',
      token: 't',
      datasetId: 'ds-1',
      ownerName: 'owner',
      ownerFingerprint: 'ab',
      relayUrl: 'https://relay.test',
      expiresAtMs: 0,
      phase: 'complete',
      revision: 6,
      creating: false,
      approvalAckPending: false,
    );

    await pumpCard(tester: tester, controller: joiner, datasetApi: datasetApi);
    await tester.pumpAndSettle();

    expect(find.textContaining('已加入数据集'), findsNothing);
    expect(find.text('进入数据集'), findsNothing);
    expect(find.textContaining('已切换'), findsNothing);
  });
}
