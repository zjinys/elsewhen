import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';

import 'support/sync_join.dart';
import 'support/sync_setup.dart';

void main() {
  testWidgets('删除暂存项：确认弹窗后调 Rust 清理并刷新列表', (tester) async {
    final bridge = TestSyncJoinApi(TestSyncSecureStorage());
    final controller = SyncJoinController(
      bridge: bridge,
      keyStore: SyncKeyStore(storage: TestSyncSecureStorage()),
    );
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [JoinerSyncPairingSettings(controller: controller)],
          ),
        ),
      ),
    );
    // 列表默认为空；点「查看可恢复的加入暂存项」拉取预置暂存项。
    await tester.tap(find.text('查看可恢复的加入暂存项'));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('暂存设备'), findsOneWidget);
    expect(find.text('解锁恢复'), findsOneWidget);
    expect(find.text('删除'), findsOneWidget);

    await tester.tap(find.text('删除'));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('删除这条暂存项？'), findsOneWidget);

    await tester.tap(find.widgetWithText(TextButton, '取消').last);
    await tester.pump(const Duration(milliseconds: 400));
    expect(bridge.discards, isEmpty);

    await tester.tap(find.text('删除'));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tap(find.widgetWithText(TextButton, '删除').last);
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pump(const Duration(milliseconds: 400));
    expect(bridge.discards, ['join-entry']);
    expect(find.textContaining('已删除暂存项'), findsOneWidget);
  });
}
