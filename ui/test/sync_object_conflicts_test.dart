import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_object_conflicts_controller.dart';
import 'package:elsewhen_ui/widgets/sync_object_conflicts_settings.dart';

api.ObjectConflictDto _conflict({
  required String opId,
  String kind = 'todo',
  String reason = 'base_revision_changed',
  String localUpdatedAt = '2026-01-02T08:00:00+00:00',
  bool withLocal = true,
}) => api.ObjectConflictDto(
  opId: opId,
  objectKind: kind,
  objectId: 'obj-1',
  reason: reason,
  senderDeviceId: 'b',
  senderDeviceName: '设备 b',
  incomingSummary: '对端版主文本',
  incomingStatus: 'active',
  incomingDeleted: false,
  incomingUpdatedAt: '2026-01-02T09:00:00+00:00',
  localSummary: withLocal ? '本端版主文本' : null,
  localStatus: withLocal ? 'open' : null,
  localUpdatedAt: withLocal ? localUpdatedAt : null,
  localHeadDeleted: false,
);

class FakeObjectConflictApi implements SyncObjectConflictApi {
  FakeObjectConflictApi(this.initial);
  List<api.ObjectConflictDto> initial;
  Object? listError;
  Object? resolveError;
  final resolutions =
      <({String opId, String? expectedLocalUpdatedAt, bool acceptRemote})>[];

  @override
  Future<List<api.ObjectConflictDto>> listConflicts() async {
    if (listError != null) throw listError!;
    return initial;
  }

  @override
  Future<void> resolve({
    required String opId,
    required String? expectedLocalUpdatedAt,
    required bool acceptRemote,
  }) async {
    resolutions.add((
      opId: opId,
      expectedLocalUpdatedAt: expectedLocalUpdatedAt,
      acceptRemote: acceptRemote,
    ));
    if (resolveError != null) throw resolveError!;
    initial = initial.where((c) => c.opId != opId).toList();
  }
}

void main() {
  testWidgets('三对象并列展示双端状态并按对象种类标注，采纳后冲突消失', (tester) async {
    final api_ = FakeObjectConflictApi([
      _conflict(opId: 'stream-a:12'),
      _conflict(opId: 'stream-a:13', kind: 'goal', reason: 'goal_active_limit'),
    ]);
    final controller = SyncObjectConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [SyncObjectConflictsSettings(controller: controller)],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('待办与目标同步冲突'), findsOneWidget);
    expect(find.text('待办 · 来自 设备 b 的改动'), findsOneWidget);
    expect(find.text('目标 · 来自 设备 b 的改动'), findsOneWidget);
    expect(find.text('两端都修改了同一条，各自基于旧版本'), findsOneWidget);
    expect(find.text('本端版主文本'), findsNWidgets(2));
    expect(find.text('对端版主文本'), findsNWidgets(2));
    expect(find.text('保留本端'), findsNWidgets(2));
    expect(find.text('采纳对端'), findsNWidgets(2));

    await tester.tap(find.text('采纳对端').first);
    await tester.pumpAndSettle();

    expect(api_.resolutions.single.opId, 'stream-a:12');
    expect(
      api_.resolutions.single.expectedLocalUpdatedAt,
      '2026-01-02T08:00:00+00:00',
      reason: '裁决必须原样回传审阅时展示的本端 updated_at',
    );
    expect(api_.resolutions.single.acceptRemote, isTrue);
    expect(find.text('待办 · 来自 设备 b 的改动'), findsNothing);
    expect(find.text('目标 · 来自 设备 b 的改动'), findsOneWidget);
    expect(find.text('本端版主文本'), findsOneWidget);
  });

  testWidgets('裁决失败保留旧视图并展示错误（含活跃上限的专属提示）', (tester) async {
    final api_ = FakeObjectConflictApi([
      _conflict(opId: 'stream-a:14', kind: 'goal', reason: 'goal_active_limit'),
    ]);
    final controller = SyncObjectConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();
    api_.resolveError = Exception('goal_active_limit');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [SyncObjectConflictsSettings(controller: controller)],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('对端目标会使本端活跃目标超出上限'), findsOneWidget);
    await tester.tap(find.text('保留本端'));
    await tester.pumpAndSettle();

    expect(controller.conflicts, hasLength(1), reason: '失败不清列表');
    expect(find.text('本端活跃目标已满，无法直接采纳对端；请先归档目标或选择保留本端。'), findsOneWidget);
    expect(find.text('本端版主文本'), findsOneWidget);
  });

  test('列表读取失败保留旧视图且展示错误', () async {
    final api_ = FakeObjectConflictApi([_conflict(opId: 'stream-a:12')]);
    final controller = SyncObjectConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();
    api_.listError = Exception('locked');
    await controller.refresh();
    expect(controller.conflicts, hasLength(1), reason: '失败保留旧视图');
    expect(controller.error, contains('操作失败'));
  });
}
