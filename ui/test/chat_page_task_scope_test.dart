import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/widgets/chat_page_task_scope.dart';

WikiChatMaterialDto material(String content, String version) =>
    WikiChatMaterialDto(
      slug: 'topic/test',
      title: 'Displayed material',
      contentMd: content,
      basisJson: jsonEncode({
        'slug': 'topic/test',
        'revision': {'hash': version},
      }),
    );

class MaterialRepository extends RustBridgeRepository {
  int reads = 0;
  @override
  Future<WikiChatMaterialDto> getWikiChatMaterial(String slug) async {
    reads++;
    return material('明确刷新后的原文', 'v2');
  }
}

void main() {
  test('reopening restores task identity and selection without new tokens', () async {
    final original = material('😀原文尾部', 'v1');
    final task = ChatPageTask('local_edit', [original], startChar: 1, endChar: 3);
    final restored = await ChatPageTask.restore(task.json, (_) async => original);
    expect(jsonDecode(restored.json), jsonDecode(task.json));
    expect(restored.startChar, 1);
    expect(restored.endChar, 3);
  });

  test('reopening compare preserves both bases and rejects a changed second page', () async {
    final first = material('first', 'v1');
    final second = WikiChatMaterialDto(
      slug: 'topic/second', title: 'Second', contentMd: 'second',
      basisJson: jsonEncode({'slug': 'topic/second', 'revision': {'hash': 'v1'}}),
    );
    final task = ChatPageTask('compare', [first, second]);
    final restored = await ChatPageTask.restore(task.json,
        (slug) async => slug == first.slug ? first : second);
    expect(jsonDecode(restored.json), jsonDecode(task.json));
    await expectLater(ChatPageTask.restore(task.json, (slug) async {
      if (slug == first.slug) return first;
      return WikiChatMaterialDto(slug: second.slug, title: second.title,
        contentMd: 'changed',
        basisJson: jsonEncode({'slug': second.slug, 'revision': {'hash': 'v2'}}));
    }), throwsStateError);
  });

  test('reopening independent task does not load an old page', () async {
    final task = ChatPageTask('none', []);
    final restored = await ChatPageTask.restore(task.json, (_) async {
      fail('independent task must not acquire page evidence');
    });
    expect(restored.kind, 'none');
    expect(restored.materials, isEmpty);
  });

  testWidgets(
    'task selection binds the displayed version until explicit refresh',
    (tester) async {
      final repo = MaterialRepository();
      ChatPageTask? chosen;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ChatPageTaskScope(
              repository: repo,
              task: ChatPageTask('local_question', [material('😀原文尾部', 'v1')]),
              onChanged: (value) => chosen = value,
            ),
          ),
        ),
      );
      await tester.tap(find.text('查看 / 选择材料'));
      await tester.pumpAndSettle();
      expect(find.text('😀原文尾部'), findsOneWidget);
      expect(repo.reads, 0);
      await tester.tap(find.byType(DropdownButton<String>));
      await tester.pumpAndSettle();
      await tester.tap(find.text('全文总结').last);
      await tester.pumpAndSettle();
      await tester.tap(find.text('使用显示的版本'));
      await tester.pumpAndSettle();
      expect(chosen!.kind, 'full_summary');
      expect(jsonDecode(chosen!.json)['pages'][0]['revision']['hash'], 'v1');
      expect(repo.reads, 0);
      await tester.tap(find.text('查看 / 选择材料'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('明确刷新版本'));
      await tester.pumpAndSettle();
      expect(find.text('明确刷新后的原文'), findsOneWidget);
      await tester.tap(find.text('使用显示的版本'));
      await tester.pumpAndSettle();
      expect(jsonDecode(chosen!.json)['pages'][0]['revision']['hash'], 'v2');
    },
  );

  test('independent task carries no page evidence', () {
    final task = ChatPageTask('none', [material('old evidence', 'v1')]);
    expect(jsonDecode(task.json)['pages'], isEmpty);
  });
}
