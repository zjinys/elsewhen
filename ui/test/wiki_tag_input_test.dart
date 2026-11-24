// 创建主题对话框的标签框：点选已有标签 / 前缀补全，且提交值仍是同一串文本。
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';

import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_tag_input.dart';

import 'support/localizations.dart';

WikiPageMeta _meta(String slug, List<String> tags) => WikiPageMeta(
  id: slug,
  slug: slug,
  kind: 'topic',
  title: slug,
  summary: '',
  tags: tags,
  evidenceCount: 0,
  firstSeenAt: DateTime(2026, 1, 1),
  lastSeenAt: DateTime(2026, 1, 1),
  status: 'active',
  createdAt: DateTime(2026, 1, 1),
  updatedAt: DateTime(2026, 1, 1),
  sourceUrl: null,
  area: 'insight',
  opinion: null,
);

/// 用假词表替掉 wikiPagesProvider：真实 provider 会走 Rust bridge。
final _fakePages = [
  _meta('a', ['编译知识', 'Rust', '小红书']),
  _meta('b', ['编译知识', 'Rust', '闲鱼']),
  _meta('c', ['编译知识', 'lens-3']),
  _meta('d', ['烟丝']),
];

Widget _harness(TextEditingController controller) => ProviderScope(
  overrides: [wikiPagesProvider.overrideWith((ref) async => _fakePages)],
  child: MaterialApp(
    locale: const Locale('zh'),
    supportedLocales: const [Locale('zh'), Locale('en')],
    localizationsDelegates: appTestDelegates(),
    home: Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: WikiTagInput(controller: controller, hintText: '标签（可选，空格或逗号分隔）'),
      ),
    ),
  ),
);

void main() {
  testWidgets('鼠标按下候选后停留再松开仍补全中文标签', (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(_harness(controller));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '烟');
    await tester.pumpAndSettle();
    expect(find.text('烟丝'), findsOneWidget);
    final pointer = await tester.startGesture(
      tester.getCenter(find.text('烟丝')),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump(const Duration(milliseconds: 300));
    await pointer.up();
    await tester.pumpAndSettle();
    expect(controller.text, '烟丝');
    expect(find.text('已有标签，点击填入'), findsNothing);
    expect(
      tester.widget<TextField>(find.byType(TextField)).focusNode!.hasFocus,
      isTrue,
    );
  });

  testWidgets('聚焦后列出已有标签，点一条即填入', (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(_harness(controller));
    await tester.pumpAndSettle();

    // 点进输入框 → 出现候选面板（空 token 也给候选，方便直接挑）。
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    expect(find.text('已有标签，点击填入'), findsOneWidget);
    expect(find.text('编译知识'), findsOneWidget);
    expect(find.text('Rust'), findsOneWidget);

    // 点选后文本进控制器，面板收起。
    await tester.tap(find.text('编译知识'));
    await tester.pumpAndSettle();
    expect(controller.text, '编译知识');
    expect(find.text('已有标签，点击填入'), findsNothing);
  });

  testWidgets('前缀补全只替换光标所在的那个 token', (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(_harness(controller));
    await tester.pumpAndSettle();

    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    // 已经写了「工作」，接着打 ru —— 应只补全后半段。
    await tester.enterText(find.byType(TextField), '工作 ru');
    // '工作 ru' == 5 个字符，光标停在末尾 → 当前 token 是 'ru'。
    controller.selection = const TextSelection.collapsed(offset: 5);
    await tester.pumpAndSettle();

    expect(find.text('已有标签，点击填入'), findsOneWidget);
    await tester.tap(find.text('Rust'));
    await tester.pumpAndSettle();
    expect(controller.text, '工作 Rust');
  });

  testWidgets('已写过的标签不再重复推荐', (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(_harness(controller));
    await tester.pumpAndSettle();

    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Rust ');
    controller.selection = const TextSelection.collapsed(offset: 5);
    await tester.pumpAndSettle();

    // 面板还在（新 token 空 → 给候选），但里面不该再有 Rust。
    expect(find.text('已有标签，点击填入'), findsOneWidget);
    expect(find.text('Rust'), findsNothing);
    expect(find.text('编译知识'), findsOneWidget);
  });

  testWidgets('同一 token 的前缀改变时即时刷新候选，点外部收起', (tester) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(_harness(controller));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'r');
    await tester.pumpAndSettle();
    expect(find.text('Rust'), findsOneWidget);
    await tester.enterText(find.byType(TextField), '烟');
    await tester.pumpAndSettle();
    expect(find.text('Rust'), findsNothing);
    expect(find.text('烟丝'), findsOneWidget);
    await tester.tapAt(const Offset(750, 560));
    await tester.pumpAndSettle();
    expect(find.text('已有标签，点击填入'), findsNothing);
    expect(controller.text, '烟');
  });

  test('词表按使用频次排序，频次相同按字典序', () {
    final container = ProviderContainer(
      overrides: [wikiPagesProvider.overrideWith((ref) async => _fakePages)],
    );
    addTearDown(container.dispose);
    // 先让 FutureProvider 落值。
    container.listen(wikiPagesProvider, (_, _) {});
    return Future<void>.delayed(Duration.zero, () {
      final tags = container.read(wikiTagVocabularyProvider);
      expect(tags.first, '编译知识'); // ×3
      expect(tags.take(3).toList()..sort(), ['Rust', 'lens-3', '编译知识']..sort());
    });
  });
}
