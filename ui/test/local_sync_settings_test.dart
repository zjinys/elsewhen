import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/sync/sync_object_conflicts_controller.dart';
import 'package:elsewhen_ui/sync/sync_wiki_conflicts_controller.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';

import 'support/sync_setup.dart';

/// 空冲突列表替身：视口测试不渲染 wiki 冲突卡片。
class _EmptyObjectConflictApi implements SyncObjectConflictApi {
  const _EmptyObjectConflictApi();

  @override
  Future<List<api.ObjectConflictDto>> listConflicts() async => const [];

  @override
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  }) async {}
}

class _EmptyWikiConflictApi implements SyncWikiConflictApi {
  const _EmptyWikiConflictApi();

  @override
  Future<List<api.WikiPageConflictDto>> listConflicts() async => const [];

  @override
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  }) async {}
}

void main() {
  testWidgets(
    'owner preparation requires disclosure and does not claim network sync',
    (tester) async {
      // 新增待办/目标冲突卡片后默认视口放不下全部按钮：抬高视口。
      tester.view.physicalSize = const Size(800, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final storage = TestSyncSecureStorage();
      final bridge = TestSyncSetupApi(storage);
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
        previewEnabled: true,
      );
      await controller.refresh();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: LocalSyncSettings(
                  controller: controller,
                  wikiConflictsController: SyncWikiConflictsController(
                    bridge: const _EmptyWikiConflictApi(),
                  ),
                  objectConflictsController: SyncObjectConflictsController(
                    bridge: const _EmptyObjectConflictApi(),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      final prepare = find.widgetWithText(FilledButton, '准备本机');
      expect(tester.widget<FilledButton>(prepare).onPressed, isNotNull);
      // 共享 Provider/API Key 的确认改为点击后弹窗，不再常驻复选框。
      await tester.tap(prepare);
      await tester.pumpAndSettle();
      expect(
        find.text('我理解：后续授权设备加入时，会共享所有 Provider 配置和 API Key。'),
        findsOneWidget,
      );
      await tester.tap(find.widgetWithText(FilledButton, '我理解并准备'));
      await tester.pumpAndSettle();
      expect(controller.state?.state, 'local_ready');
      expect(find.text('同步已开启'), findsOneWidget);
      expect(find.text('锁定本机密钥'), findsOneWidget);
      expect(find.textContaining('同步完成'), findsNothing);
      // 本机信息卡：owner 可重命名（ready 态才有入口）；保存后显示新名。
      await tester.ensureVisible(find.text('重命名'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('重命名'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.byType(TextField),
        ),
        '书桌主机',
      );
      await tester.tap(find.widgetWithText(TextButton, '保存'));
      await tester.pumpAndSettle();
      expect(bridge.renames.single.$2, '书桌主机');
      expect(find.text('设备名：书桌主机'), findsOneWidget);
      // 本机信息卡加高管理 tab：按钮可能落到测试视口外，先滚到可见再点。
      await tester.ensureVisible(find.text('锁定本机密钥'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('锁定本机密钥'));
      await tester.pumpAndSettle();
      expect(find.text('解锁本机密钥'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      await controller.close();
      controller.dispose();
    },
  );

  testWidgets('设备名称在状态卡之前，准备本机紧跟输入框，确认走弹窗', (tester) async {
    tester.view.physicalSize = const Size(320, 780);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final storage = TestSyncSecureStorage();
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: TestSyncSetupApi(storage),
      previewEnabled: true,
    );
    await controller.refresh();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: LocalSyncSettings(controller: controller),
            ),
          ),
        ),
      ),
    );
    final nameField = find.byType(TextField);
    expect(nameField, findsOneWidget);
    // 设备名称在「同步未开启」状态卡之前。
    expect(
      tester.getTopLeft(nameField).dy,
      lessThan(tester.getTopLeft(find.text('同步未开启')).dy),
    );
    // 「准备本机」紧跟输入框右侧（同一行）；窄屏不溢出。
    final prepare = find.widgetWithText(FilledButton, '准备本机');
    expect(
      tester.getTopLeft(prepare).dx,
      greaterThanOrEqualTo(tester.getTopRight(nameField).dx),
    );
    expect(tester.takeException(), isNull);
    // 共享 Provider/API Key 的确认是弹窗，不是页面上的常驻复选框。
    expect(find.byType(CheckboxListTile), findsNothing);
    await tester.tap(prepare);
    await tester.pumpAndSettle();
    expect(find.text('准备本机加入同步？'), findsOneWidget);
    await tester.tap(find.widgetWithText(TextButton, '取消'));
    await tester.pumpAndSettle();
    expect(find.text('准备本机加入同步？'), findsNothing);
    expect(controller.state?.state, 'not_configured');
    await tester.pumpWidget(const SizedBox());
    await controller.close();
    controller.dispose();
  });

  testWidgets(
    'release default shows the boundary without exposing unfinished setup',
    (tester) async {
      final storage = TestSyncSecureStorage();
      final controller = SyncSetupController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: TestSyncSetupApi(storage),
        previewEnabled: false,
      );
      await controller.refresh();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: LocalSyncSettings(controller: controller)),
        ),
      );
      expect(find.text('准备本机'), findsNothing);
      expect(find.textContaining('当前构建中关闭'), findsOneWidget);
      expect(find.textContaining('ELSEWHEN_SYNC_PREVIEW'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
  );

  testWidgets('narrow layout and unavailable backends remain readable', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(360, 780);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final storage = TestSyncSecureStorage()..failWrites = true;
    final controller = SyncSetupController(
      keyStore: SyncKeyStore(storage: storage),
      bridge: TestSyncSetupApi(storage),
      previewEnabled: true,
    );
    await controller.refresh();
    await controller.createOwner('owner', 'test', consent: true);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: LocalSyncSettings(controller: controller),
            ),
          ),
        ),
      ),
    );
    expect(find.textContaining('系统密钥库不可用'), findsOneWidget);
    expect(tester.takeException(), isNull);
    expect(find.textContaining('SYNTHETIC_SECRET'), findsNothing);
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: LocalSyncSettings(controller: null)),
      ),
    );
    expect(find.text('当前存储后端不支持设备同步。'), findsOneWidget);
    controller.dispose();
  });
}
