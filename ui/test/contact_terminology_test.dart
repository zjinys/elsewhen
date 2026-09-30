import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/widgets/event_input.dart';

void main() {
  test('person data retains its identity and displays contact labels', () {
    final now = DateTime(2026, 9, 30);
    final page = WikiPage(
      id: 'contact-fixture',
      slug: 'person/contact-fixture',
      kind: 'person',
      title: '测试联系人',
      summary: '',
      contentMd: '',
      tags: const [],
      sourceEventIds: const [],
      evidenceCount: 0,
      firstSeenAt: now,
      lastSeenAt: now,
      status: 'active',
      createdAt: now,
      updatedAt: now,
      area: 'network',
    );
    expect(page.kindLabel, '联系人');
    expect(page.areaLabel, '联系人/项目');
    expect(page.kind, 'person');
    expect(page.slug, 'person/contact-fixture');
  });

  testWidgets('recording help uses contact terminology', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: EventInput(onSubmit: (_) {})),
      ),
    );
    expect(find.textContaining('标注联系人'), findsOneWidget);
    await tester.tap(find.byTooltip('对话格式'));
    await tester.pumpAndSettle();
    expect(find.textContaining('联系人关系'), findsOneWidget);
    expect(find.textContaining('人物'), findsNothing);
  });
}
