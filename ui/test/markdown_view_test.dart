import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';

/// MarkdownView 渲染回归测试（纯本地，无桥接/网络）。
/// 覆盖知识库正文常用的：标题、段落、行内样式、复选/有序列表、引用、代码块、分隔线。
void main() {
  const sample = '''
# 一级标题

这是一个**加粗**段落，含 `code` 与 [[wiki-link]]，还有[链接文字](https://example.com)。

## 二级标题

- [x] 已完成事项
- [ ] 未完成事项

1. 有序一
2. 有序二

---

> 一段引用
''';

  testWidgets('markdown 渲染：标题/段落/列表/引用/行内样式', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(markdown: sample),
          ),
        ),
      ),
    );

    // 标题是 Text，可直接匹配
    expect(find.text('一级标题'), findsOneWidget);
    expect(find.text('二级标题'), findsOneWidget);

    // 正文/引用是 RichText（承载行内样式），用纯文本断言
    expect(_richTextContaining('这是一个'), findsWidgets);
    expect(_richTextContaining('一段引用'), findsOneWidget);

    // 行内样式：**加粗** 应产生 w700 的 span
    expect(_hasBoldFor('加粗'), isTrue, reason: '**加粗** 应渲染为粗体');
    // 行内代码 / wikilink / markdown 链接都保留可读文字
    expect(_richTextContaining('code'), findsWidgets);
    expect(_richTextContaining('wiki-link'), findsWidgets);
    expect(_richTextContaining('链接文字'), findsWidgets);

    // 复选列表：完成 / 未完成两种图标
    expect(find.byIcon(Icons.check_box_outlined), findsOneWidget);
    expect(find.byIcon(Icons.check_box_outline_blank), findsOneWidget);

    // 有序列表序号
    expect(find.text('1.'), findsOneWidget);
    expect(find.text('2.'), findsOneWidget);
  });
}

Finder _richTextContaining(String needle) => find.byWidgetPredicate(
      (w) => w is RichText && w.text.toPlainText().contains(needle),
    );

bool _hasBoldFor(String needle) {
  for (final element in find.byType(RichText).evaluate()) {
    final rt = element.widget as RichText;
    if (!rt.text.toPlainText().contains(needle)) continue;
    final span = rt.text;
    if (span is TextSpan && _walkBold(span, needle)) return true;
  }
  return false;
}

bool _walkBold(TextSpan span, String needle) {
  final t = span.text;
  if (t != null &&
      t.contains(needle) &&
      span.style?.fontWeight == FontWeight.w700) {
    return true;
  }
  for (final child in span.children ?? const <InlineSpan>[]) {
    if (child is TextSpan && _walkBold(child, needle)) return true;
  }
  return false;
}
