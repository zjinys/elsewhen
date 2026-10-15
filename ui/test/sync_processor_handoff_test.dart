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

api.SyncRosterMemberDto _candidate(String id) => api.SyncRosterMemberDto(
  deviceId: id,
  name: '设备 $id',
  platform: 'android',
  active: true,
  isOwner: false,
  isProcessor: false,
  isLocal: false,
);

Future<SyncProcessorHandoffController> _pump(
  WidgetTester tester,
  FakeHandoffApi bridge, {
  List<api.SyncRosterMemberDto> candidates = const [],
  bool isOwner = true,
}) async {
  final controller = SyncProcessorHandoffController(bridge: bridge);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: SyncProcessorHandoffSettings(
            controller: controller,
            isProcessor: true,
            isOwner: isOwner,
            candidates: candidates,
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return controller;
}

/// 从下拉中选择目标设备并让出（含二次确认）。
Future<void> _yieldViaDropdown(WidgetTester tester, String deviceName) async {
  await tester.tap(find.byType(DropdownButtonFormField<String>).first);
  await tester.pumpAndSettle();
  await tester.tap(find.textContaining(deviceName).last);
  await tester.pumpAndSettle();
  await tester.tap(find.text('让出 AI 分析职责'));
  await tester.pumpAndSettle();
  await tester.tap(find.widgetWithText(FilledButton, '让出'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('让出：owner 同机时生成证明并立即审批，失败文案可读', (tester) async {
    final bridge = FakeHandoffApi([
      _state(state: 'idle', from: null),
      _state(state: 'relinquished', to: 'b', finalSequence: 2),
    ]);
    await _pump(tester, bridge, candidates: [_candidate('b')]);
    expect(bridge.yieldCalls, 0);
    expect(find.text('本机状态：未参与移交。'), findsOneWidget);

    await _yieldViaDropdown(tester, '设备 b');

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 1, reason: 'owner 同机时必须自动审批');
    expect(bridge.yieldedTo, 'b');
    expect(bridge.approvedProof, isNotNull);
    expect(find.text('本机已让出 AI 分析职责，等待目标设备确认接管。'), findsOneWidget);
    expect(
      find.byType(DropdownButtonFormField<String>),
      findsNothing,
      reason: '让出后不再重复让出',
    );
  });

  testWidgets('让出被静默防线拦下时展示可读文案且不审批', (tester) async {
    final bridge = FakeHandoffApi([_state(state: 'idle', from: null)]);
    bridge.yieldError = Exception(
      'sync_background_processing_must_finish_before_enable',
    );
    await _pump(tester, bridge, candidates: [_candidate('b')]);

    await _yieldViaDropdown(tester, '设备 b');

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 0, reason: '证明生成失败绝不能继续审批');
    expect(find.text('本机仍有进行中的后台任务，必须等它们完成后才能让出 AI 分析职责。'), findsOneWidget);
  });

  testWidgets('非主设备本机让出后不自动审批并提示等待主设备', (tester) async {
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
              candidates: [_candidate('b')],
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await _yieldViaDropdown(tester, '设备 b');

    expect(bridge.yieldCalls, 1);
    expect(bridge.approveCalls, 0, reason: '非 owner 本机绝不能自动审批');
    expect(find.text('等待主设备审批移交证明。'), findsOneWidget);
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
    expect(find.text('本机正在负责 AI 分析（此前的一次接管已完成）。'), findsOneWidget);
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
