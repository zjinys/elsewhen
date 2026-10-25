import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_enrollment_api.dart';
import 'package:elsewhen_ui/sync/sync_enrollment_controller.dart';
import 'package:elsewhen_ui/sync/sync_keep_alive.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';

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
  bool storageBlocked = false,
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
  storageBlocked: storageBlocked,
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
      storageBlocked: false,
    );
  }

  @override
  Future<bool> release(api.SyncEnrollmentStateDto state) async {
    releases.add(state);
    return state.operationId == current.operationId;
  }
}

class EnrollmentFixture {
  EnrollmentFixture({
    Duration interval = const Duration(seconds: 2),
    Duration stalledTick = const Duration(seconds: 45),
  }) {
    storage.values['elsewhen.sync.master.v1.test-install'] =
        'v1:${base64Encode(List.filled(32, 7))}';
    storage.values['elsewhen.sync.revision.v1.test-install'] = 'v1:1';
    controller = SyncEnrollmentController(
      keyStore: keys,
      bridge: bridge,
      interval: interval,
      stalledTick: stalledTick,
    );
  }

  final storage = TestSyncSecureStorage();
  late final bridge = EnrollmentBridge(storage);
  late final keys = SyncKeyStore(storage: storage);
  late final SyncEnrollmentController controller;

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

  test(
    'a stalled native call is requeued instead of stopping the polling chain',
    () async {
      final f = EnrollmentFixture(
        // 间隔大于看门狗：失联判定之后、下一轮开始之前留出断言窗口。
        interval: const Duration(milliseconds: 100),
        stalledTick: const Duration(milliseconds: 40),
      );
      // 永远不返回的原生推进：模拟中继半开连接 / 原生侧卡在锁上。
      final hang = Completer<void>();
      f.bridge.advanceGate = hang.future;
      unawaited(f.controller.arm(operationId: 'request-1'));
      await Future<void>.delayed(const Duration(milliseconds: 5));
      expect(f.bridge.advances, hasLength(1));
      expect(f.controller.busy, isTrue);

      await Future<void>.delayed(const Duration(milliseconds: 60));
      // 看门狗把这一轮判定失联：给出可读原因，并重新排下一轮。
      expect(f.controller.error, contains('没有响应'));
      // 后面的轮次不再卡住（hang 只影响已挂上的那次调用）。
      f.bridge.advanceGate = null;
      await Future<void>.delayed(const Duration(milliseconds: 400));
      expect(f.bridge.advances.length, greaterThan(1));
      expect(f.bridge.confirmations, isNotEmpty);
      await f.close();
    },
  );

  test('pause stops the keep-alive even when the in-flight tick never returns', () async {
    final calls = <String>[];
    SyncKeepAlive.debugSupportedOverride = true;
    SyncKeepAlive.debugInvoke = (method, _) => calls.add(method);
    addTearDown(() {
      SyncKeepAlive.debugSupportedOverride = null;
      SyncKeepAlive.debugInvoke = null;
      SyncKeepAlive.releaseAll();
    });
    final f = EnrollmentFixture(
      interval: const Duration(milliseconds: 10),
      stalledTick: const Duration(milliseconds: 40),
    );
    f.bridge.advanceGate = Completer<void>().future;
    unawaited(f.controller.arm(operationId: 'request-1'));
    await Future<void>.delayed(const Duration(milliseconds: 5));
    expect(calls, ['start']);
    // 没等满 stalledTick 也要能收尾：否则通知栏会永久残留。
    await f.controller.pause().timeout(const Duration(seconds: 2));
    expect(calls.last, 'stop');
    expect(SyncKeepAlive.active, isFalse);
  });

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
}
