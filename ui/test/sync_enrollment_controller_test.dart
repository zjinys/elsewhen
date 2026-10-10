import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_enrollment_api.dart';
import 'package:elsewhen_ui/sync/sync_enrollment_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/sync_enrollment_progress.dart';

import 'support/sync_setup.dart';

api.SyncEnrollmentStateDto enrollmentState({
  String role = 'owner',
  String operation = 'request-1',
  String invitation = 'invitation-1',
  String installation = 'test-install',
  int revision = 1,
  String? token,
  String phase = 'preparing_channel',
  bool localApplied = false,
  bool remoteApplied = false,
  bool notificationPending = false,
}) => api.SyncEnrollmentStateDto(
  role: role,
  operationId: operation,
  invitationId: invitation,
  installationId: installation,
  revision: revision,
  confirmationToken: token,
  phase: phase,
  snapshotId: null,
  receivedChunks: 0,
  totalChunks: 0,
  localApplied: localApplied,
  remoteApplied: remoteApplied,
  notificationPending: notificationPending,
);

class EnrollmentBridge implements SyncEnrollmentApi {
  EnrollmentBridge(this.storage);
  final TestSyncSecureStorage storage;
  api.SyncEnrollmentStateDto current = enrollmentState();
  api.SyncEnrollmentStateDto? advanceResult;
  Future<void>? advanceGate;
  Future<void>? listGate;
  bool failAdvance = false;
  bool authorizationChanged = false;
  int lists = 0;
  final advances = <api.SyncEnrollmentStateDto>[];
  final confirmations = <api.SyncEnrollmentStateDto>[];
  final releases = <api.SyncEnrollmentStateDto>[];

  @override
  Future<List<api.SyncEnrollmentStateDto>> list() async {
    lists++;
    await listGate;
    return [current];
  }

  @override
  Future<api.SyncEnrollmentStateDto> advance(
    api.SyncEnrollmentStateDto state,
  ) async {
    advances.add(state);
    await advanceGate;
    if (authorizationChanged) {
      throw StateError(
        'sync_enrollment_authorization_changed_requires_repair SYNTHETIC_SECRET',
      );
    }
    if (failAdvance) throw StateError('SYNTHETIC_SECRET network failure');
    return current =
        advanceResult ?? enrollmentState(revision: 2, token: 'original-commit');
  }

  @override
  Future<api.SyncEnrollmentStateDto> confirm(
    api.SyncEnrollmentStateDto state,
    int protectedRevision,
  ) async {
    expect(
      storage.values['elsewhen.sync.revision.v1.${state.installationId}'],
      'v1:$protectedRevision',
    );
    expect(protectedRevision, state.revision);
    expect(state.confirmationToken, isNotNull);
    confirmations.add(state);
    return current = enrollmentState(
      role: state.role,
      operation: state.operationId,
      invitation: state.invitationId,
      installation: state.installationId,
      revision: state.revision,
      phase: state.phase,
      localApplied: state.localApplied,
      remoteApplied: state.remoteApplied,
      notificationPending: state.notificationPending,
    );
  }

  @override
  Future<bool> release(api.SyncEnrollmentStateDto state) async {
    releases.add(state);
    return state.operationId == current.operationId;
  }
}

class EnrollmentFixture {
  final storage = TestSyncSecureStorage();
  late final bridge = EnrollmentBridge(storage);
  late final keys = SyncKeyStore(storage: storage);
  late final controller = SyncEnrollmentController(
    keyStore: keys,
    bridge: bridge,
  );

  EnrollmentFixture() {
    storage.values['elsewhen.sync.master.v1.test-install'] =
        'v1:${base64Encode(List.filled(32, 7))}';
    storage.values['elsewhen.sync.revision.v1.test-install'] = 'v1:1';
  }

  Future<void> close() async {
    await controller.close();
    controller.dispose();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'one flight persists and reads the exact native revision before confirming',
    () async {
      final f = EnrollmentFixture();
      final gate = Completer<void>();
      f.bridge.advanceGate = gate.future;
      final first = f.controller.arm(operationId: 'request-1');
      await Future<void>.delayed(Duration.zero);
      final second = f.controller.arm(operationId: 'request-1');
      expect(f.bridge.advances, hasLength(1));
      expect(f.bridge.confirmations, isEmpty);
      gate.complete();
      await Future.wait([first, second]);
      expect(f.bridge.advances, hasLength(1));
      expect(
        f.bridge.confirmations.single.confirmationToken,
        'original-commit',
      );
      expect(f.bridge.confirmations.single.invitationId, 'invitation-1');
      expect(f.controller.error, isNull);
      expect(
        f.storage.values['elsewhen.sync.revision.v1.test-install'],
        'v1:2',
      );
      await f.close();
    },
  );

  test('a recovered native commit is confirmed without advancing or approving again', () async {
    final f = EnrollmentFixture();
    f.bridge.current = enrollmentState(revision: 2, token: 'restored-token');
    await f.controller.arm(operationId: 'request-1');
    expect(f.bridge.advances, isEmpty);
    expect(f.bridge.confirmations.single.confirmationToken, 'restored-token');
    await f.close();
  });

  test(
    'native failure stops output and releases only the original operation',
    () async {
      final f = EnrollmentFixture();
      final master = f.storage.values['elsewhen.sync.master.v1.test-install'];
      f.storage.failWrites = true;
      await f.controller.arm(operationId: 'request-1');
      expect(f.bridge.confirmations, isEmpty);
      expect(f.bridge.releases.single.operationId, 'request-1');
      expect(f.controller.paused, isTrue);
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      expect(f.storage.values['elsewhen.sync.master.v1.test-install'], master);
      expect(
        f.storage.values['elsewhen.sync.revision.v1.test-install'],
        'v1:1',
      );
      await f.close();
    },
  );

  test(
    'close waits for a native write and suppresses the late confirmation',
    () async {
      final f = EnrollmentFixture();
      final gate = Completer<void>();
      f.storage.writeGate = gate.future;
      final advancing = f.controller.arm(operationId: 'request-1');
      await Future<void>.delayed(Duration.zero);
      expect(f.storage.writes, 1);
      var closed = false;
      final closing = f.controller.close().then((_) => closed = true);
      await Future<void>.delayed(Duration.zero);
      expect(closed, isFalse);
      expect(f.bridge.confirmations, isEmpty);
      gate.complete();
      await advancing;
      await closing;
      expect(f.bridge.confirmations, isEmpty);
      expect(
        f.bridge.releases,
        isEmpty,
        reason: 'the owning controller releases after close',
      );
      f.controller.dispose();
    },
  );

  test(
    'late network results cannot write native state after a pause',
    () async {
      final f = EnrollmentFixture();
      final gate = Completer<void>();
      f.bridge.advanceGate = gate.future;
      final advancing = f.controller.arm(operationId: 'request-1');
      await Future<void>.delayed(Duration.zero);
      final pause = f.controller.pause();
      gate.complete();
      await advancing;
      await pause;
      expect(f.storage.writes, 0);
      expect(f.bridge.confirmations, isEmpty);
      await f.close();
    },
  );

  test(
    'old inventory cannot adopt or release a newer workspace session',
    () async {
      final f = EnrollmentFixture();
      f.bridge.current = enrollmentState(
        operation: 'new-operation',
        revision: 2,
        token: 'new-token',
      );
      await f.controller.arm(operationId: 'request-1');
      expect(f.controller.paused, isTrue);
      expect(f.bridge.advances, isEmpty);
      expect(f.bridge.confirmations, isEmpty);
      expect(f.bridge.releases, isEmpty);
      expect(f.storage.writes, 0);
      await f.close();
    },
  );

  testWidgets(
    'changed relay authorization stops automatic retries without replacing permissions',
    (tester) async {
      final f = EnrollmentFixture();
      f.bridge.authorizationChanged = true;
      await f.controller.arm(operationId: 'request-1');
      expect(f.controller.paused, isTrue);
      expect(f.controller.error, contains('授权或修订已改变'));
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      await tester.pump(const Duration(minutes: 1));
      expect(f.bridge.advances, hasLength(1));
      expect(f.bridge.confirmations, isEmpty);
      expect(f.bridge.releases, isEmpty);
      await f.close();
    },
  );

  testWidgets(
    'automatic retry uses backoff and preserves the displayed scope',
    (tester) async {
      final f = EnrollmentFixture();
      f.bridge.failAdvance = true;
      await f.controller.arm(operationId: 'request-1');
      expect(f.bridge.advances, hasLength(1));
      await tester.pump(const Duration(seconds: 3));
      expect(f.bridge.advances, hasLength(1));
      await tester.pump(const Duration(seconds: 1));
      expect(f.bridge.advances, hasLength(2));
      expect(f.bridge.advances.last.invitationId, 'invitation-1');
      expect(f.controller.error, isNot(contains('SYNTHETIC_SECRET')));
      await f.close();
    },
  );

  test('legacy setup fakes do not implicitly call enrollment FRB', () async {
    final f = EnrollmentFixture();
    final setup = TestSyncSetupApi(f.storage);
    final controller = SyncSetupController(
      keyStore: f.keys,
      bridge: setup,
      enrollmentBridge: f.bridge,
      previewEnabled: true,
    );
    // This is an unlock of synthetic existing state, not creation over a key.
    setup.current = testSyncState('locked');
    await controller.unlock();
    expect(controller.error, isNull);
    expect(f.bridge.lists, 0);
    await controller.close();
    controller.dispose();
    await f.close();
  });

  test(
    'owner close waits for enrollment native I/O before releasing its lease',
    () async {
      final f = EnrollmentFixture();
      final setup = TestSyncSetupApi(f.storage)
        ..current = testSyncState('locked');
      final gate = Completer<void>();
      f.bridge.advanceGate = gate.future;
      final controller = SyncSetupController(
        keyStore: f.keys,
        bridge: setup,
        enrollmentBridge: f.bridge,
        autoEnrollment: true,
        previewEnabled: true,
      );
      await controller.unlock();
      await Future<void>.delayed(Duration.zero);
      final write = Completer<void>();
      f.storage.writeGate = write.future;
      gate.complete();
      await Future<void>.delayed(Duration.zero);
      final closing = controller.close();
      await Future<void>.delayed(Duration.zero);
      expect(setup.released, isEmpty);
      write.complete();
      await closing;
      expect(setup.released, ['request-1']);
      expect(f.bridge.confirmations, isEmpty);
      controller.dispose();
      await f.close();
    },
  );

  testWidgets(
    'progress distinguishes local application, remote receipt and switching',
    (tester) async {
      final f = EnrollmentFixture();
      f.controller.states = [
        enrollmentState(
          role: 'joiner',
          phase: 'awaiting_receipt',
          localApplied: true,
        ),
      ];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: SyncEnrollmentProgress(controller: f.controller),
            ),
          ),
        ),
      );
      expect(find.textContaining('尚不能声称旧设备已收到'), findsOneWidget);
      expect(find.textContaining('当前数据集不会切换'), findsOneWidget);
      expect(find.textContaining('旧设备已确认应用回执'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await f.close();
    },
  );
}
