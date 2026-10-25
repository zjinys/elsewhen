import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/widgets/sync_tick_status.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// R6-2 的面板口径：状态由 Rust 推导，Dart 只负责把分类码翻成人话。
// 这些断言锁住的是「别把原因吞掉」——静默显示「同步中」比显示错误更难查。
void main() {
  Future<void> pump(
    WidgetTester tester,
    ValueNotifier<List<api.SyncDeliveryStatusDto>> statuses,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SyncDeliverStatusLine(statuses: statuses),
        ),
      ),
    );
  }

  api.SyncDeliveryStatusDto dto({
    String state = 'awaiting_receipt',
    String reason = '',
    int pending = 2,
    int awaiting = 2,
  }) => api.SyncDeliveryStatusDto(
    peerDeviceId: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
    state: state,
    reason: reason,
    untilMs: 0,
    pending: pending,
    awaitingReceipt: awaiting,
  );

  testWidgets('空闲时不占位置：没有待投递的信封就不显示', (tester) async {
    final statuses = ValueNotifier<List<api.SyncDeliveryStatusDto>>([]);
    await pump(tester, statuses);
    expect(find.textContaining('投递'), findsNothing);
  });

  testWidgets('等回执与发送中分别说清楚在等谁', (tester) async {
    final statuses = ValueNotifier<List<api.SyncDeliveryStatusDto>>([
      dto(state: 'awaiting_receipt'),
    ]);
    await pump(tester, statuses);
    expect(find.textContaining('已送达中继，等对方确认'), findsOneWidget);
    expect(find.textContaining('待发 0'), findsOneWidget);

    statuses.value = [dto(state: 'uploading', pending: 3, awaiting: 1)];
    await tester.pump();
    expect(find.textContaining('发送中'), findsOneWidget);
    // 待发 = pending - awaiting_receipt：这个差值是用户唯一能看懂「还差多少」的数。
    expect(find.textContaining('待发 2'), findsOneWidget);
  });

  testWidgets('blocked 与 backoff 都翻译成原因，未知码原样显示', (tester) async {
    final statuses = ValueNotifier<List<api.SyncDeliveryStatusDto>>([
      dto(state: 'blocked', reason: 'relay_quota'),
    ]);
    await pump(tester, statuses);
    expect(find.textContaining('中继信箱已满'), findsOneWidget);

    statuses.value = [dto(state: 'backoff', reason: 'transport')];
    await tester.pump();
    expect(find.textContaining('稍后重试：网络不稳定'), findsOneWidget);

    // 没见过的新分类码不能被吞掉：显示原码，才有线索去查。
    statuses.value = [dto(state: 'blocked', reason: 'some_new_code')];
    await tester.pump();
    expect(find.textContaining('some_new_code'), findsOneWidget);

    // 阻塞带着「到点会再试」的时刻：不说这一句，用户只会以为要自己动手。
    final untilMs = DateTime.now()
        .add(const Duration(seconds: 15))
        .millisecondsSinceEpoch;
    statuses.value = [
      api.SyncDeliveryStatusDto(
        peerDeviceId: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
        state: 'blocked',
        reason: 'relay_quota',
        untilMs: untilMs,
        pending: 2,
        awaitingReceipt: 0,
      ),
    ];
    await tester.pump();
    expect(find.textContaining('中继信箱已满'), findsOneWidget);
    expect(find.textContaining('后重试'), findsOneWidget);
  });

  testWidgets('未知状态码原样显示，不回落成「空闲」', (tester) async {
    // Rust 将来新增一个状态（比如 `paused`）：旧 UI 必须把它显示出来，而不是
    // 说「空闲」—— 否则「有信封在等、界面却说没事」这种现象无从排查。
    final statuses = ValueNotifier<List<api.SyncDeliveryStatusDto>>([
      dto(state: 'paused_future_state'),
    ]);
    await pump(tester, statuses);
    expect(find.textContaining('paused_future_state'), findsOneWidget);
    expect(find.textContaining('空闲'), findsNothing);

    // `idle` 是已知状态：仍然说「空闲」（不过面板本身会把它过滤掉）。
    expect(
      SyncDeliverStatusLine.stateLabel(dto(state: 'idle')),
      '空闲',
    );
  });

  testWidgets('对齐口径：reasonLabel 对表内五项都有中文', (tester) async {
    for (final code in [
      'relay_quota',
      'protocol',
      'integrity',
      'transport',
      'replay',
    ]) {
      expect(
        SyncDeliverStatusLine.reasonLabel(code),
        isNot(code),
        reason: '$code 必须有中文说法',
      );
    }
  });
}
