// 记事本单页语义测试。
//
// ⚠️ 测试基建约束：testWidgets 运行在 FakeAsync zone，真桥 FFI 的 future
// （Dart port 回调完成）在该 zone 永不完成——真桥断言必须走裸 test()
// （与其他真桥测试同口径），配合 ProviderContainer 读 provider；
// widget 呈现层用 override notepadDetailProvider 的纯假数据测试，不触碰 FFI。
//
//  1. ensure「先查后建」：首次静默建档；再次读取绝不 create
//     （create 对已存在页是 upsert 覆盖正文，会清掉快记历史）——
//     以「重建 container 后快记内容仍在」做行为级断言；
//  2. 全库只允许一张 memo 页；
//  3. 面板呈现：内容渲染、空态文案（纯 widget，假数据）。
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/notepad_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

ProviderContainer _notepadContainer(RustBridgeRepository repo) =>
    ProviderContainer(overrides: [
      storageRepositoryProvider.overrideWithValue(repo),
    ]);

void main() {
  test('ensure 先查后建：快记追加后再次打开复用同页、不被覆盖', () async {
    final repo = await createIsolatedBridge();

    // 首次读取：静默建档
    final first = await _notepadContainer(repo).read(notepadDetailProvider.future);
    expect(first, isNotNull, reason: '首次打开应静默建出唯一记事页');
    expect(first!.slug, kNotepadSlug);

    // 快记追加
    await repo.appendWikiPageContent(slug: kNotepadSlug, appendMd: '买牛奶');

    // 重建 container（模拟下次点击「记事」重新 open）：ensure 必须走「先查」，
    // 不得再 create 覆盖正文。
    final reopened =
        await _notepadContainer(repo).read(notepadDetailProvider.future);
    expect(reopened!.contentMd, contains('买牛奶'),
        reason: '再次打开同一面板必须复用已有页，快记不得被清空');
    expect(reopened.id, first.id, reason: '必须复用同一页（同 slug）');

    // 记事本全库只允许一张 memo 页
    final metas = await repo.listWikiPageMetas();
    expect(metas.where((m) => m.kind == 'memo').length, 1,
        reason: '记事本只允许一张 memo 页');
  });

  testWidgets('面板呈现：正文渲染（假数据，不走 FFI）', (tester) async {
    await tester.pumpWidget(_panel('买牛奶'));
    await tester.pumpAndSettle();
    expect(find.textContaining('买牛奶'), findsOneWidget);
  });

  testWidgets('面板呈现：空态文案（假数据，不走 FFI）', (tester) async {
    await tester.pumpWidget(_panel(''));
    await tester.pumpAndSettle();
    expect(find.text('（空，下面快记第一条）'), findsOneWidget);
  });
}

WikiPage _page(String contentMd) => WikiPage(
      id: 'p1',
      slug: kNotepadSlug,
      kind: 'memo',
      title: '记事本',
      summary: '',
      contentMd: contentMd,
      tags: [],
      sourceEventIds: [],
      evidenceCount: 0,
      firstSeenAt: DateTime.utc(2026, 1, 1),
      lastSeenAt: DateTime.utc(2026, 1, 1),
      status: 'active',
      createdAt: DateTime.utc(2026, 1, 1),
      updatedAt: DateTime.utc(2026, 1, 1),
      area: '',
    );

Widget _panel(String contentMd) => ProviderScope(
      overrides: [
        notepadDetailProvider.overrideWith((ref) async => _page(contentMd)),
        // 不按快记键就不会触达 repo；给个未初始化实例满足类型即可
        storageRepositoryProvider
            .overrideWithValue(RustBridgeRepository(databasePath: '/unused')),
      ],
      child: const MaterialApp(home: Scaffold(body: NotepadView())),
    );
