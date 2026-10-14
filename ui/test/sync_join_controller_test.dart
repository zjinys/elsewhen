import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';

import 'support/sync_join.dart';
import 'support/sync_setup.dart';

class Fixture {
  final storage = TestSyncSecureStorage();
  late final bridge = TestSyncJoinApi(storage);
  late final controller = SyncJoinController(
    keyStore: SyncKeyStore(storage: storage),
    bridge: bridge,
    previewEnabled: true,
  );
  Future<void> join() async {
    await controller.preview('SYNTHETIC_INVITATION');
    await controller.join(controller.state!, 'joining', 'test', consent: true);
  }

  Future<void> close() async {
    await controller.close();
    controller.dispose();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'unmounting the join preview does not touch a disposed text controller',
    (tester) async {
      final f = Fixture();
      final gate = Completer<void>();
      f.bridge.previewGate = gate.future;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: JoinerSyncPairingSettings(controller: f.controller),
            ),
          ),
        ),
      );
      await tester.enterText(
        find.byType(TextField).first,
        'SYNTHETIC_INVITATION',
      );
      await tester.ensureVisible(find.text('预览邀请（不联网）'));
      await tester.tap(find.text('预览邀请（不联网）'));
      await tester.pumpWidget(const SizedBox());
      gate.complete();
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await f.close();
    },
  );

  test('join reuses the displayed preview and confirms native revisions before request and proof', () async {
    final f = Fixture();
    await f.controller.preview('SYNTHETIC_INVITATION');
    expect(f.storage.writes, 0);
    final shown = f.controller.state!;
    await f.controller.join(shown, 'joining', 'test', consent: false);
    expect(f.bridge.preparations, 0);
    f.controller.state = testJoinState('preview', token: 'newer-not-displayed');
    await f.controller.join(shown, 'joining', 'test', consent: true);
    expect(f.bridge.preparedToken, 'preview-token');
    expect(f.controller.error, isNull);
    expect(f.bridge.requests, 1);
    expect(f.bridge.proofs, 0);
    expect(f.storage.values['elsewhen.sync.revision.v1.join-install'], 'v1:1');
    await f.controller.inspectApproval();
    expect(f.controller.error, isNull);
    expect(f.bridge.proofs, 1);
    expect(f.storage.values['elsewhen.sync.revision.v1.join-install'], 'v1:2');
    expect(f.controller.state?.phase, 'proof_sent');
    expect(f.controller.notice, contains('没有切换'));
    await f.close();
  });

  test(
    'initial native failure releases only the join operation and never sends',
    () async {
      final f = Fixture();
      await f.controller.preview('SYNTHETIC_INVITATION');
      f.storage.failWrites = true;
      await f.controller.join(
        f.controller.state!,
        'joining',
        'test',
        consent: true,
      );
      expect(f.bridge.requests, 0);
      expect(f.bridge.proofs, 0);
      expect(f.bridge.released, ['join-operation']);
      expect(f.controller.active, isFalse);
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      await f.close();
    },
  );

  test('approval native failure resumes the saved checkpoint without replacing the master', () async {
    final f = Fixture();
    await f.join();
    final original = f.storage.values['elsewhen.sync.master.v1.join-install'];
    f.storage.failWrites = true;
    await f.controller.inspectApproval();
    expect(f.bridge.checkpoint, isTrue);
    expect(f.bridge.proofs, 0);
    expect(f.controller.active, isFalse);
    expect(f.storage.values['elsewhen.sync.revision.v1.join-install'], 'v1:1');
    f.storage.failWrites = false;
    await f.controller.resume(testJoinEntry);
    expect(f.controller.error, isNull);
    expect(f.bridge.proofs, 1);
    expect(f.bridge.preparations, 1);
    expect(f.bridge.inspections, 1);
    expect(f.storage.values['elsewhen.sync.master.v1.join-install'], original);
    expect(f.controller.state?.phase, 'proof_sent');
    await f.close();
  });

  test('closing waits for the native write and never sends a late installation proof', () async {
    final f = Fixture();
    await f.join();
    final gate = Completer<void>();
    f.storage.writeGate = gate.future;
    final writes = f.storage.writes;
    final receiving = f.controller.inspectApproval();
    await Future<void>.delayed(Duration.zero);
    expect(f.storage.writes, writes + 1);
    final closing = f.controller.close();
    await Future<void>.delayed(Duration.zero);
    expect(f.bridge.released, isEmpty);
    gate.complete();
    await receiving;
    await closing;
    expect(f.bridge.proofs, 0);
    expect(f.bridge.released, ['join-operation']);
    f.controller.dispose();
  });

  test('lost proof response retries proof delivery without reinstalling or approving', () async {
    final f = Fixture();
    await f.join();
    f.bridge.failProof = true;
    await f.controller.inspectApproval();
    expect(f.controller.error, contains('中继步骤未确认'));
    expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
    expect(f.bridge.proofs, 1);
    expect(f.bridge.released, isEmpty);
    f.bridge.failProof = false;
    await f.controller.sendProof();
    expect(f.bridge.proofs, 2);
    expect(f.bridge.installations, 1);
    expect(f.bridge.inspections, 1);
    await f.close();
  });

  test(
    'closing during preview releases its eventual original operation',
    () async {
      final f = Fixture();
      final gate = Completer<void>();
      f.bridge.previewGate = gate.future;
      final preview = f.controller.preview('SYNTHETIC_INVITATION');
      final closing = f.controller.close();
      expect(f.bridge.released, isEmpty);
      gate.complete();
      await preview;
      await closing;
      expect(f.bridge.released, ['join-operation']);
      expect(f.storage.writes, 0);
      f.controller.dispose();
    },
  );

  test('a changed installation target is not confirmed or used for network traffic', () async {
    final f = Fixture();
    f.bridge.wrongInstallTarget = true;
    await f.join();
    expect(f.bridge.confirmations, 0);
    expect(f.bridge.requests, 0);
    expect(f.controller.active, isFalse);
    await f.close();
  });

  test(
    'disabled preview does not access native storage or the join API',
    () async {
      final storage = TestSyncSecureStorage();
      final bridge = TestSyncJoinApi(storage);
      final controller = SyncJoinController(
        keyStore: SyncKeyStore(storage: storage),
        bridge: bridge,
        previewEnabled: false,
      );
      await controller.preview('SYNTHETIC_INVITATION');
      expect(bridge.previews, 0);
      expect(storage.writes, 0);
      controller.dispose();
    },
  );

  testWidgets(
    'join preview fits a phone, requires consent, and stops at awaiting membership',
    (tester) async {
      tester.view.physicalSize = const Size(360, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final f = Fixture();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: JoinerSyncPairingSettings(controller: f.controller),
              ),
            ),
          ),
        ),
      );
      expect(find.text('预览邀请（不联网）'), findsOneWidget);
      await tester.enterText(
        find.byType(TextField).first,
        'SYNTHETIC_INVITATION',
      );
      await tester.tap(find.text('预览邀请（不联网）'));
      await tester.pumpAndSettle();
      final join = find.widgetWithText(FilledButton, '在独立暂存库发起加入');
      expect(tester.widget<FilledButton>(join).onPressed, isNull);
      expect(find.text('http://localhost:9876'), findsOneWidget);
      await tester.ensureVisible(find.byType(CheckboxListTile));
      await tester.tap(find.byType(CheckboxListTile));
      await tester.pump();
      await tester.ensureVisible(join);
      await tester.tap(join);
      await tester.pumpAndSettle();
      expect(f.bridge.requests, 1);
      expect(find.text('本机公钥指纹（请在已有设备核对）'), findsOneWidget);
      final inspect = find.text('检查已有设备审批');
      await tester.ensureVisible(inspect);
      await tester.tap(inspect);
      await tester.pumpAndSettle();
      expect(f.bridge.proofs, 1);
      expect(f.controller.state?.phase, 'proof_sent');
      expect(find.textContaining('安装证明已投递，等待授权与初始同步'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await f.close();
    },
  );
}
