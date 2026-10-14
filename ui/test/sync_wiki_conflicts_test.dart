import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_wiki_conflicts_controller.dart';
import 'package:elsewhen_ui/widgets/sync_wiki_conflicts_settings.dart';

api.WikiPageConflictDto _conflict({
  required String opId,
  String reason = 'wiki_page_deleted',
  String localUpdatedAt = '2026-01-02T08:00:00+00:00',
  bool withLocal = true,
}) => api.WikiPageConflictDto(
  opId: opId,
  objectId: 'page-1',
  reason: reason,
  senderDeviceId: 'b',
  senderDeviceName: '设备 b',
  incomingSlug: 'topic/项目',
  incomingTitle: '项目（对端版）',
  incomingDeleted: false,
  incomingRenamedFrom: reason == 'wiki_page_rename_diverged'
      ? 'topic/旧路径'
      : null,
  incomingSummary: '对端摘要',
  incomingContentMdLen: 120,
  incomingUpdatedAt: '2026-01-02T09:00:00+00:00',
  local: withLocal
      ? api.WikiPageConflictSideDto(
          slug: 'topic/项目',
          title: '项目（本端版）',
          contentMdLen: 90,
          updatedAt: localUpdatedAt,
        )
      : null,
  localHeadDeleted: false,
);

class FakeWikiConflictApi implements SyncWikiConflictApi {
  FakeWikiConflictApi(this.initial);
  List<api.WikiPageConflictDto> initial;
  Object? listError;
  Object? resolveError;
  final resolutions =
      <({String opId, String? expectedLocalUpdatedAt, bool acceptRemote})>[];

  @override
  Future<List<api.WikiPageConflictDto>> listConflicts() async {
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
  testWidgets('展示双端状态与裁决语义文案，采纳后冲突消失', (tester) async {
    final api_ = FakeWikiConflictApi([_conflict(opId: 'stream-a:12')]);
    final controller = SyncWikiConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [SyncWikiConflictsSettings(controller: controller)],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('知识库同步冲突'), findsOneWidget);
    expect(find.text('来自 设备 b 的改动'), findsOneWidget);
    expect(find.text('对端已删除本页，而本端之后仍有改动要送达'), findsOneWidget);
    expect(find.text('项目（本端版）'), findsOneWidget);
    expect(find.text('项目（对端版）'), findsOneWidget);
    expect(find.text('正文 90 字 · 更新 2026-01-02T08:00:00+00:00'), findsOneWidget);
    expect(find.text('保留本端'), findsOneWidget);
    expect(find.text('采纳对端'), findsOneWidget);

    await tester.tap(find.text('采纳对端'));
    await tester.pumpAndSettle();

    expect(api_.resolutions.single.opId, 'stream-a:12');
    expect(
      api_.resolutions.single.expectedLocalUpdatedAt,
      '2026-01-02T08:00:00+00:00',
      reason: '裁决必须原样回传审阅时展示的本端 updated_at',
    );
    expect(api_.resolutions.single.acceptRemote, isTrue);
    expect(find.text('没有待审阅的知识库同步冲突。'), findsOneWidget);
  });

  testWidgets('裁决失败保留旧视图并展示错误', (tester) async {
    final api_ = FakeWikiConflictApi([_conflict(opId: 'stream-a:12')]);
    final controller = SyncWikiConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();
    api_.resolveError = Exception('wiki_conflict_local_changed');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [SyncWikiConflictsSettings(controller: controller)],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('保留本端'));
    await tester.pumpAndSettle();

    expect(controller.conflicts, hasLength(1), reason: '失败不清列表');
    expect(find.text('本端页面已变化，请刷新后重新审阅。'), findsOneWidget);
    // 本端行仍在，未做字面量乐观移除。
    expect(find.text('项目（本端版）'), findsOneWidget);
  });

  test('列表读取失败保留旧视图且展示错误', () async {
    final api_ = FakeWikiConflictApi([_conflict(opId: 'stream-a:12')]);
    final controller = SyncWikiConflictsController(bridge: api_);
    addTearDown(controller.dispose);
    await controller.refresh();
    api_.listError = Exception('locked');
    await controller.refresh();
    expect(controller.conflicts, hasLength(1), reason: '失败保留旧视图');
    expect(controller.error, contains('操作失败'));
  });
}
