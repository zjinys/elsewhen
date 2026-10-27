import 'package:elsewhen_ui/sync/sync_backlog_notice.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/sync_setup.dart';

/// 真机现场（2026-10-27 15:30）：手机侧 `sync_wire_outbox` 有 1 条未回执，
/// 同步页的投递行实时写着「待确认 1」，但主界面状态位整整 8 分钟没有出现
/// —— 因为状态快照只在 banner 的 `initState` 里读过一次。重启 app 后立刻
/// 显示「对端未确认 1 条 · 2.7 KB」，证明判定与数字都对，错的是「谁来刷新」。
SyncSetupController _controllerFor(TestSyncSetupApi setup) => SyncSetupController(
  keyStore: SyncKeyStore(storage: setup.storage),
  bridge: setup,
  previewEnabled: true,
);

/// 投递进度信号（真身是 `ValueNotifier<List<SyncDeliveryStatusDto>>`）。
/// 这里只需要「能推一次」和「是否还挂着监听」两个能力；`hasListeners` 是
/// `ChangeNotifier` 的 protected 成员，只能在子类内部读。
class _TickSignal extends ChangeNotifier {
  bool attached = false;
  bool get isAttached {
    attached = hasListeners;
    return attached;
  }

  void fire() => notifyListeners();
}

void main() {
  test('投递进度一变就重读快照：积压从无到有时状态位必须自己浮出来', () async {
    final setup = TestSyncSetupApi(TestSyncSecureStorage())
      ..current = testSyncState('local_ready');
    final controller = _controllerFor(setup);
    final ticks = _TickSignal();
    controller.attachBacklogSource(ticks);

    await controller.refresh();
    expect(
      syncBacklogNotice(controller.state),
      isNull,
      reason: '此时确实没有积压，状态位不该出现',
    );

    // 本机发出一条消息 → 封包入 outbox 但未获回执（对端此刻不在线）。
    setup.current = testSyncState(
      'local_ready',
      pendingWireEnvelopes: 1,
      pendingBytes: 2765,
    );
    ticks.fire(); // 引擎跑了一拍，投递状态被重写
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);

    final notice = syncBacklogNotice(controller.state);
    expect(notice, isNotNull, reason: '积压出现后状态位必须随之出现');
    expect(notice!.kind, SyncBacklogKind.awaitingPeer);
    expect(notice.message, '同步延迟：对端未确认 1 条 · 2.7 KB，正在重试');
    controller.dispose();
  });

  test('值没变就不惊动界面：重复的投递心跳不该反复 notify', () async {
    final setup = TestSyncSetupApi(TestSyncSecureStorage())
      ..current = testSyncState(
        'local_ready',
        pendingWireEnvelopes: 1,
        pendingBytes: 2765,
      );
    final controller = _controllerFor(setup);
    final ticks = _TickSignal();
    controller.attachBacklogSource(ticks);
    await controller.refresh();

    var notifications = 0;
    controller.addListener(() => notifications++);
    for (var i = 1; i <= 3; i++) {
      ticks.fire();
      await Future<void>.delayed(Duration.zero);
      await Future<void>.delayed(Duration.zero);
    }
    expect(setup.stateReads, greaterThan(1), reason: '每拍都该重读（不然就是 8 分钟不更新）');
    expect(
      notifications,
      0,
      reason: '读数没变就不该 notify：状态位每次都重建会闪',
    );
    controller.dispose();
  });

  test('订阅可断：dispose 之后投递心跳不再读库', () async {
    final setup = TestSyncSetupApi(TestSyncSecureStorage())
      ..current = testSyncState('local_ready');
    final controller = _controllerFor(setup);
    final ticks = _TickSignal();
    controller.attachBacklogSource(ticks);
    await controller.refresh();
    expect(ticks.isAttached, isTrue, reason: 'dispose 前确实是订阅着的');
    controller.dispose();
    // 「不再读库」由 `_disposed` 早退守着；`removeListener` 的可观测效应只有
    // 监听器是否还挂在信号上（否则控制器被信号强引用，且回调永远在里面跑）。
    expect(ticks.isAttached, isFalse, reason: 'dispose 必须摘掉订阅');
    final reads = setup.stateReads;

    ticks.fire();
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);
    expect(setup.stateReads, reads);
  });
}
