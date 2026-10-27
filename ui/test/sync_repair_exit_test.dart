import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_repair_api.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/sync_setup.dart';

/// 记录调用次数并返回「清完之后」的状态。
class _RepairApi implements SyncRepairApi {
  _RepairApi(this.setup);
  final TestSyncSetupApi setup;
  int calls = 0;

  @override
  Future<api.LocalSyncStateDto> clearLocalSyncIdentity() async {
    calls++;
    setup.current = testSyncState('not_configured');
    return setup.current;
  }
}

Future<(_RepairApi, SyncSetupController)> pumpRepair(WidgetTester tester) async {
  tester.view.physicalSize = const Size(900, 1400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final storage = TestSyncSecureStorage();
  final setup = TestSyncSetupApi(storage)..current = testSyncState('requires_repair');
  final controller = SyncSetupController(
    keyStore: SyncKeyStore(storage: storage),
    bridge: setup,
    previewEnabled: true,
  );
  await controller.refresh();
  final repair = _RepairApi(setup);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: LocalSyncSettings(controller: controller, repairApi: repair),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return (repair, controller);
}

void main() {
  testWidgets('requires_repair 给出唯一出口，并且只在它出现时出现', (tester) async {
    final (_, controller) = await pumpRepair(tester);
    expect(controller.state?.state, 'requires_repair');
    expect(find.text('清除本机同步身份'), findsOneWidget);
    expect(
      find.textContaining('笔记、对话、知识库都会保留'),
      findsOneWidget,
      reason: '必须先说清代价，用户才可能做决定',
    );
  });

  testWidgets('健康身份下不出现清除入口（不能当通用重置）', (tester) async {
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final storage = TestSyncSecureStorage();
    final setup = TestSyncSetupApi(storage)..current = testSyncState('local_ready');
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: setup,
      previewEnabled: true,
    );
    await controller.refresh();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: LocalSyncSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('清除本机同步身份'), findsNothing);
  });

  testWidgets('二次确认必须勾选副作用，未勾选时不能执行', (tester) async {
    final (repair, _) = await pumpRepair(tester);
    await tester.tap(find.text('清除本机同步身份'));
    await tester.pumpAndSettle();

    expect(find.text('清除本机同步身份？'), findsOneWidget);
    expect(find.textContaining('Provider API Key 会失效'), findsOneWidget);
    expect(find.textContaining('需要重新配对或重新加入'), findsOneWidget);

    final confirm = find.widgetWithText(FilledButton, '清除并重新配对');
    expect(
      tester.widget<FilledButton>(confirm).onPressed,
      isNull,
      reason: '没勾选「已知晓」就不该能点',
    );

    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(repair.calls, 0, reason: '取消不能有任何副作用');
  });

  testWidgets('勾选后执行清除，控制器刷新到 not_configured', (tester) async {
    final (repair, controller) = await pumpRepair(tester);
    await tester.tap(find.text('清除本机同步身份'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('我已知晓，Provider API Key 需要重新填写'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('清除并重新配对'));
    await tester.pumpAndSettle();

    expect(repair.calls, 1);
    expect(controller.state?.state, 'not_configured');
    expect(
      find.text('清除本机同步身份'),
      findsNothing,
      reason: '清完之后这条出口必须消失（状态已经不是 requires_repair）',
    );
  });
}
