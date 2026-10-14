import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';

import 'support/sync_setup.dart';

void main() {
  testWidgets(
    'owner preparation requires disclosure and does not claim network sync',
    (tester) async {
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
                child: LocalSyncSettings(controller: controller),
              ),
            ),
          ),
        ),
      );
      final prepare = find.widgetWithText(FilledButton, '准备本机');
      expect(tester.widget<FilledButton>(prepare).onPressed, isNull);
      // 说明性长文案已裁剪：页面只保留状态、操作与必要提示。
      await tester.tap(find.byType(CheckboxListTile));
      await tester.pump();
      await tester.tap(prepare);
      await tester.pumpAndSettle();
      expect(controller.state?.state, 'local_ready');
      expect(find.text('本机密钥已解锁'), findsOneWidget);
      expect(find.text('锁定本机密钥'), findsOneWidget);
      expect(find.textContaining('同步完成'), findsNothing);
      await tester.tap(find.text('锁定本机密钥'));
      await tester.pumpAndSettle();
      expect(find.text('解锁本机密钥'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      await controller.close();
      controller.dispose();
    },
  );

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
