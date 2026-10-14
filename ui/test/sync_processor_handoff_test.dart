import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_processor_handoff_controller.dart';
import 'package:elsewhen_ui/widgets/sync_processor_handoff_settings.dart';

api.ProcessorHandoffStateDto _state({
  String state = 'idle',
  String? from = 'a',
  String? to,
  int? finalSequence,
  bool canConfirm = false,
}) => api.ProcessorHandoffStateDto(
  state: state,
  fromDeviceId: from,
  toDeviceId: to,
  finalSequence: finalSequence,
  canConfirm: canConfirm,
);

api.ProcessingYieldProofDto _proof() => api.ProcessingYieldProofDto(
  datasetId: 'dataset',
  fromDeviceId: 'a',
  toDeviceId: 'b',
  rosterRevision: 3,
  finalSequence: 2,
  signature: Uint8List.fromList(const [1, 2, 3]),
);

class FakeHandoffApi implements ProcessorHandoffApi {
  /// 按顺序返回的状态队列；耗尽后永远返回最后一个（不变视图）。
  FakeHandoffApi(List<api.ProcessorHandoffStateDto> states) : _states = states;
  final List<api.ProcessorHandoffStateDto> _states;
  api.ProcessorHandoffStateDto get _next =>
      _states.length <= 1 ? _states.first : _states.removeAt(0);
  Object? yieldError;
  Object? approveError;
  Object? confirmError;
  int yieldCalls = 0;
  int approveCalls = 0;
  int confirmCalls = 0;
  String? yieldedTo;
  api.ProcessingYieldProofDto? approvedProof;

  @override
  Future<api.ProcessorHandoffStateDto> getState() async => _next;
  @override
  Future<api.ProcessingYieldProofDto> yieldProcessor({
    required String toDeviceId,
  }) async {
    yieldCalls++;
    yieldedTo = toDeviceId;
    if (yieldError != null) throw yieldError!;
    return _proof();
  }

  @override
  Future<void> approve({required api.ProcessingYieldProofDto proof}) async {
    approveCalls++;
    approvedProof = proof;
    if (approveError != null) throw approveError!;
  }

  @override
  Future<void> confirmTakeover() async {
    confirmCalls++;
    if (confirmError != null) throw confirmError!;
  }
}

Widget _host(SyncProcessorHandoffController controller) => MaterialApp(
  home: Scaffold(
    body: SingleChildScrollView(
      child: SyncProcessorHandoffSettings(
        controller: controller,
        isProcessor: true,
        isOwner: true,
      ),
    ),
  ),
);

Future<SyncProcessorHandoffController> _pump(
  WidgetTester tester,
  FakeHandoffApi bridge,
) async {
  final controller = SyncProcessorHandoffController(bridge: bridge);
  await tester.pumpWidget(_host(controller));
  await tester.pumpAndSettle();
  return controller;
}

void main() {
  testWidgets('让出：owner 同机时生成证明并立即审批，失败文案可读', (tester) async {
    final bridge = FakeHandoffApi([
      _state(state: 'idle', from: null),
      _state(state: 'relinquished', to: 'b', finalSequence: 2),
    ]);
    await _pump(tester, bridge);
    expect(bridge.yieldCalls, 0);
    expect(find.text('本机状态：未参与移交。'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'b');
    // 二次确认对话框。
    await tester.tap(find.text('让出加工权'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, '让出'));
    await tester.pumpAndSettle();

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 1, reason: 'owner 同机时必须自动审批');
    expect(bridge.yieldedTo, 'b');
    expect(bridge.approvedProof, isNotNull);
    expect(find.text('本机已让出加工权，等待目标设备确认接管。'), findsOneWidget);
    expect(find.byType(TextField), findsNothing, reason: '让出后不再重复让出');
  });

  testWidgets('让出被静默防线拦下时展示可读文案且不审批', (tester) async {
    final bridge = FakeHandoffApi([_state(state: 'idle', from: null)]);
    bridge.yieldError = Exception(
      'sync_background_processing_must_finish_before_enable',
    );
    await _pump(tester, bridge);

    await tester.enterText(find.byType(TextField), 'b');
    await tester.tap(find.text('让出加工权'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, '让出'));
    await tester.pumpAndSettle();

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 0, reason: '证明生成失败绝不能继续审批');
    expect(find.text('本机仍有进行中的后台任务，必须等它们完成后才能让出加工权。'), findsOneWidget);
  });

  testWidgets('非 owner 本机让出后不自动审批并提示等待 owner', (tester) async {
    final bridge = FakeHandoffApi([
      _state(state: 'idle', from: null),
      _state(state: 'relinquished', to: 'b', finalSequence: 1),
    ]);
    final controller = SyncProcessorHandoffController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProcessorHandoffSettings(
              controller: controller,
              isProcessor: true,
              isOwner: false,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'b');
    await tester.tap(find.text('让出加工权'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, '让出'));
    await tester.pumpAndSettle();

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 0, reason: '非 owner 本机绝不能自动审批');
    expect(find.text('移交证明需要由 owner 设备审批签发新名册后才会生效。'), findsOneWidget);
  });

  testWidgets('接管：仅 frontier 追平（canConfirm）展示确认按钮且成功后刷新', (tester) async {
    final bridge = FakeHandoffApi([
      _state(state: 'pending_takeover', from: 'a', finalSequence: 2),
      _state(
        state: 'pending_takeover',
        from: 'a',
        finalSequence: 2,
        canConfirm: true,
      ),
      _state(state: 'active', from: null),
    ]);
    final controller = await _pump(tester, bridge);
    expect(find.text('确认接管'), findsNothing, reason: '未追平时绝不展示确认按钮');
    expect(find.text('有移交等待本机确认，但本机尚未同步到移交边界。'), findsOneWidget);

    // 追平后展示并确认。
    await controller.refresh();
    await tester.pumpAndSettle();
    expect(find.text('确认接管'), findsOneWidget);
    await tester.tap(find.text('确认接管'));
    await tester.pumpAndSettle();

    expect(bridge.confirmCalls, 1);
    expect(find.text('本机正在加工（此前的一次接管已完成）。'), findsOneWidget);
  });

  testWidgets('确认被 sync_frontier_lagging 拦下时展示可读文案', (tester) async {
    final bridge = FakeHandoffApi([
      _state(state: 'pending_takeover', from: 'a', canConfirm: true),
    ]);
    bridge.confirmError = Exception('sync_frontier_lagging');
    await _pump(tester, bridge);

    await tester.tap(find.text('确认接管'));
    await tester.pumpAndSettle();

    expect(bridge.confirmCalls, 1);
    expect(find.text('本机尚未同步到移交边界，请先等待同步追平再确认接管。'), findsOneWidget);
  });
}
