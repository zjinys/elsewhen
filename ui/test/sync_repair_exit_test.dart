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
  _RepairApi(this.setup, {this.migratable = false, this.judgementThrows = false});
  final TestSyncSetupApi setup;
  final bool migratable;
  final bool judgementThrows;
  int calls = 0;

  @override
  Future<api.LocalSyncStateDto> clearLocalSyncIdentity() async {
    calls++;
    setup.current = testSyncState('not_configured');
    return setup.current;
  }

  @override
  Future<bool> providerKeysMigratable() async {
    if (judgementThrows) {
      throw StateError('bridge down');
    }
    return migratable;
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

  testWidgets('旧 master 在本进程时，弹窗文案走「自动保留」分支（C12）', (tester) async {
    // 判定可用 ⇒ 弹窗必须告诉用户 key 会自动保留，不能仍说「会失效」
    // （否则用户被吓得不敢用出口，或者清完后重复手工重填）。
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final storage = TestSyncSecureStorage();
    final setup = TestSyncSetupApi(storage)
      ..current = testSyncState('requires_repair');
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: setup,
      previewEnabled: true,
    );
    await controller.refresh();
    final repair = _RepairApi(setup, migratable: true);
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

    await tester.tap(find.text('清除本机同步身份'));
    await tester.pumpAndSettle();
    expect(find.textContaining('自动保留（重新配对后自动迁移）'), findsOneWidget);
    expect(find.textContaining('会失效'), findsNothing);
    expect(find.text('我已知晓，本机需要重新配对才能同步'), findsOneWidget);

    await tester.tap(find.text('我已知晓，本机需要重新配对才能同步'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('清除并重新配对'));
    await tester.pumpAndSettle();
    expect(repair.calls, 1);
  });

  testWidgets('可迁移性判定抛错时，弹窗回退到保守「会失效」文案（C12 负向）', (tester) async {
    // 变异验证补的负向：判定不可用/桥接抛错时绝不能误报「自动保留」，
    // 否则用户清完身份才发现 key 全没了 —— 宁多提醒，不静默承诺。
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final storage = TestSyncSecureStorage();
    final setup = TestSyncSetupApi(storage)
      ..current = testSyncState('requires_repair');
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: setup,
      previewEnabled: true,
    );
    await controller.refresh();
    final repair = _RepairApi(setup, judgementThrows: true);
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

    await tester.tap(find.text('清除本机同步身份'));
    await tester.pumpAndSettle();
    expect(find.textContaining('会失效，需要重新填写'), findsOneWidget);
    expect(find.textContaining('自动保留'), findsNothing);
    expect(find.text('我已知晓，Provider API Key 需要重新填写'), findsOneWidget);
  });
}
