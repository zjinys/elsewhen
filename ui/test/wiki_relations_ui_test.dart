import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/main_screen.dart';

import 'support/isolated_bridge.dart';

/// 人物关系功能测试（真实 Rust 桥接）：
/// 1) 桥接往返：addRelation → listRelationsForPage（双向可见）→ deleteRelation；
/// 2) UI：自种「人物 + 项目」两个页面并加一条关系，在知识库详情页头部应出现
///    「人物关系」区块与关系 chip。
void main() {
  late RustBridgeRepository repo;

  setUpAll(() async {
    repo = await createIsolatedBridge();
  });

  String stamp() => DateTime.now().microsecondsSinceEpoch.toString();

  test('bridge relation roundtrip (add/list both directions/delete)', () async {
    final personTitle = '桥接人物${stamp()}';
    final projectTitle = '桥接项目${stamp()}';
    final person = await repo.saveTextPage(text: '简介', title: personTitle);
    final project = await repo.saveTextPage(text: '说明', title: projectTitle);
    final rel = await repo.addRelation(
      fromSlug: person.slug,
      toSlug: project.slug,
      relation: '参与',
      note: '测试',
    );
    try {
      // 从人物方、项目方都能查到这条关系
      final fromPerson = await repo.listRelationsForPage(person.slug);
      final fromProject = await repo.listRelationsForPage(project.slug);
      expect(fromPerson.length, 1);
      expect(fromProject.length, 1);
      expect(fromPerson.first.relation, '参与');
      expect(fromPerson.first.fromSlug, person.slug);
      expect(fromPerson.first.toSlug, project.slug);
      expect(fromProject.first.fromSlug, person.slug, reason: '项目页应能看到人物方');
      expect(fromProject.first.toSlug, project.slug);

      // 删除后两侧都看不到
      expect(await repo.deleteRelation(rel.id), isTrue);
      expect(await repo.listRelationsForPage(person.slug), isEmpty);
      expect(await repo.listRelationsForPage(project.slug), isEmpty);
    } finally {
      await repo.deleteRelation(rel.id);
    }
  });

  testWidgets('wiki detail page shows people relations section', (
    tester,
  ) async {
    // 接近真实主窗口的测试画布，避免小屏把详情正文挤出视口
    tester.view.physicalSize = const Size(1600, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    // 自种数据：人物页 + 项目页 + 一条关系（标题带时间戳避免与真实数据混淆）
    final personTitle = 'UI人物${stamp()}';
    final projectTitle = 'UI项目${stamp()}';
    final person = await tester.runAsync(
      () => repo.saveTextPage(text: '$personTitle 的简介', title: personTitle),
    );
    final project = await tester.runAsync(
      () => repo.saveTextPage(text: '$projectTitle 的说明', title: projectTitle),
    );
    final relation = await tester.runAsync(
      () => repo.addRelation(
        fromSlug: person!.slug,
        toSlug: project!.slug,
        relation: '负责',
        note: '测试关系',
      ),
    );
    addTearDown(() async {
      await tester.runAsync(() async {
        await repo.deleteRelation(relation!.id);
      });
    });

    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
        child: const MaterialApp(home: MainScreen()),
      ),
    );
    await tester.pump();

    // 切到知识库 tab
    await tester.tap(find.text('知识库'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 500)),
    );
    await tester.pump();

    // 滚动左侧列表直到「人物」页可见（列表按 kind 分组，新页在 topic 组附近）
    final sidebarList = find
        .byWidgetPredicate(
          (w) => w is ListView && w.scrollDirection == Axis.vertical,
        )
        .first;
    for (var i = 0; i < 12; i++) {
      if (find.text(personTitle).evaluate().isNotEmpty) break;
      await tester.drag(sidebarList, const Offset(0, -250));
      await tester.pump();
    }
    expect(
      find.text(personTitle),
      findsWidgets,
      reason: '滚动后应能看到人物页 $personTitle',
    );

    // 打开人物页详情
    await tester.tap(find.text(personTitle).first);
    await tester.pump();
    // 多轮「真实异步窗口 + pump」：页面 provider 与关系 provider 都依赖真实桥接往返
    for (var i = 0; i < 3; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 300)),
      );
      await tester.pump();
    }

    // 详情头部应有「人物关系」区块 + 关系类型 chip
    expect(find.text('人物关系'), findsOneWidget, reason: '详情头应有「人物关系」区块');
    expect(find.text('负责'), findsWidgets, reason: '关系 chip 应显示关系类型');
  });
}
