import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_roster_controller.dart';
import 'package:elsewhen_ui/widgets/sync_members_settings.dart';

api.SyncRosterMemberDto _member({
  required String id,
  String name = '',
  bool active = true,
  bool isOwner = false,
  bool isProcessor = false,
  bool isLocal = false,
}) => api.SyncRosterMemberDto(
  deviceId: id,
  name: name.isEmpty ? '设备 $id' : name,
  platform: 'android',
  active: active,
  isOwner: isOwner,
  isProcessor: isProcessor,
  isLocal: isLocal,
);

api.SyncRosterDto _roster({
  required List<api.SyncRosterMemberDto> members,
  String local = 'a',
  String owner = 'a',
  String processor = 'a',
  int revision = 4,
}) => api.SyncRosterDto(
  revision: revision,
  datasetId: 'dataset',
  ownerDeviceId: owner,
  processorDeviceId: processor,
  localDeviceId: local,
  members: members,
);

class FakeRosterApi implements SyncRosterApi {
  FakeRosterApi(this.initial);
  api.SyncRosterDto initial;
  final List<api.SyncRosterDto> subsequent = [];
  Object? revokeError;
  final revocations = <({String deviceId, int revision})>[];

  api.SyncRosterDto get _next =>
      subsequent.isEmpty ? initial : subsequent.removeAt(0);

  @override
  Future<api.SyncRosterDto> getRoster() async => _next;

  @override
  Future<void> revokeMember({
    required String deviceId,
    required int expectedRevision,
  }) async {
    revocations.add((deviceId: deviceId, revision: expectedRevision));
    if (revokeError != null) throw revokeError!;
  }
}

Widget _host(SyncRosterController controller) => MaterialApp(
  home: Scaffold(
    body: SingleChildScrollView(
      child: SyncMembersSettings(controller: controller),
    ),
  ),
);

Future<SyncRosterController> _pump(
  WidgetTester tester,
  FakeRosterApi bridge,
) async {
  final controller = SyncRosterController(bridge: bridge);
  await tester.pumpWidget(_host(controller));
  await tester.pumpAndSettle();
  return controller;
}

Future<void> _revokeDevice(WidgetTester tester, String deviceName) async {
  await tester.tap(find.text('撤销').first);
  await tester.pumpAndSettle();
  // 确认对话框（Provider Key 边界披露）。
  expect(find.textContaining('请到对应 Provider 控制台吊销或轮换密钥'), findsOneWidget);
  await tester.tap(find.widgetWithText(FilledButton, '撤销').last);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('owner 本机：可见成员列表、加工设备不可撤销、普通成员可撤销', (tester) async {
    final bridge = FakeRosterApi(
      _roster(
        members: [
          _member(id: 'a', isOwner: true, isProcessor: true, isLocal: true),
          _member(id: 'b', isProcessor: true),
          _member(id: 'c'),
        ],
      ),
    );
    await _pump(tester, bridge);

    // 只有非 owner/非加工/非本机的设备 c 有撤销按钮。
    expect(find.text('撤销'), findsOneWidget);
    expect(find.text('需先移交 AI 分析职责后再撤销'), findsOneWidget);
    expect(find.text('设备 a（主设备 · 负责 AI 分析 · 本机）'), findsOneWidget);

    // 预置撤销成功后的名册（confirmRevoke 末尾的刷新会消费）。
    bridge.subsequent.add(
      _roster(
        members: [
          _member(id: 'a', isOwner: true, isProcessor: true, isLocal: true),
          _member(id: 'b', isProcessor: true),
          _member(id: 'c', active: false),
        ],
      ),
    );
    await _revokeDevice(tester, '设备 c');

    expect(bridge.revocations, hasLength(1));
    expect(bridge.revocations.single.deviceId, 'c');
    expect(bridge.revocations.single.revision, 4);
    // 已撤销成员不再显示。
    expect(find.textContaining('已撤销'), findsNothing);
    expect(find.text('撤销'), findsNothing);
  });
  testWidgets('非 owner 本机不可撤销任何设备', (tester) async {
    final bridge = FakeRosterApi(
      _roster(
        owner: 'a',
        processor: 'b',
        local: 'c',
        members: [
          _member(id: 'a', isOwner: true),
          _member(id: 'b', isProcessor: true),
          _member(id: 'c', isLocal: true),
          _member(id: 'd'),
        ],
      ),
    );
    await _pump(tester, bridge);
    expect(find.text('撤销'), findsNothing);
  });

  testWidgets('撤销被 sync_membership_handoff_required 拦下时展示可读文案', (tester) async {
    // UI 已隐藏加工设备的撤销按钮；此用例模拟竞态（审阅后设备升为加工端）。
    final bridge = FakeRosterApi(
      _roster(
        members: [
          _member(id: 'a', isOwner: true, isProcessor: true, isLocal: true),
          _member(id: 'c'),
        ],
      ),
    );
    bridge.revokeError = Exception('sync_membership_handoff_required');
    await _pump(tester, bridge);

    await _revokeDevice(tester, '设备 c');

    expect(bridge.revocations, hasLength(1));
    expect(find.text('该设备当前负责 AI 分析，必须先移交职责才能撤销。'), findsOneWidget);
    // 名册视图保留（不清空）。
    expect(find.text('设备 a（主设备 · 负责 AI 分析 · 本机）'), findsOneWidget);
  });

  testWidgets('修订号被并发更新时提示重新审阅', (tester) async {
    final bridge = FakeRosterApi(
      _roster(
        members: [
          _member(id: 'a', isOwner: true, isProcessor: true, isLocal: true),
          _member(id: 'c'),
        ],
      ),
    );
    bridge.revokeError = Exception('sync_membership_revision_changed');
    await _pump(tester, bridge);

    await _revokeDevice(tester, '设备 c');

    expect(find.text('名册刚被其他设备更新，请刷新后重新审阅撤销。'), findsOneWidget);
    expect(
      find.text('设备 a（主设备 · 负责 AI 分析 · 本机）'),
      findsOneWidget,
      reason: '失败不清空视图',
    );
  });
}
