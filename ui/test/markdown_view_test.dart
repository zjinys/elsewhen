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
          body: SingleChildScrollView(child: MarkdownView(markdown: sample)),
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

  const tableSample = '''
对照表：

| 名称 | 数量 | 说明 |
| --- | ---: | :---: |
| 苹果 | **3** | ~~售罄~~ 补货中 |
| 香蕉 | 5 | `code` 单元格 |

后续段落。
''';

  testWidgets('markdown 渲染：表格与删除线', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(markdown: tableSample),
          ),
        ),
      ),
    );

    // 表格出现，且表头/单元格内容都保留可读文字
    expect(find.byType(Table), findsOneWidget);
    expect(_richTextContaining('名称'), findsOneWidget);
    expect(_richTextContaining('补货中'), findsOneWidget);
    expect(_richTextContaining('后续段落。'), findsOneWidget);

    // 表头单元格加粗（w600），单元格内 **3** 再叠 w700
    expect(_hasWeightFor('名称', FontWeight.w600), isTrue, reason: '表头应加粗');
    expect(_hasBoldFor('3'), isTrue, reason: '单元格内行内粗体应生效');

    // 删除线：~~售罄~~ 应产生 lineThrough 的 span
    expect(_hasStrikeFor('售罄'), isTrue, reason: '~~售罄~~ 应渲染为删除线');
  });

  testWidgets('markdown 渲染：空行分隔的列表仍连续编号', (tester) async {
    // AI 回复常见形态：每个列表项前留空行排版
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(
              markdown: '**现在的状态：**\n\n1. **Topic 页** — 草稿已建\n\n2. **挂到 anvil 页下** — 还没做\n\n3. **规则** — 还没建\n\n后续段落。',
            ),
          ),
        ),
      ),
    );
    // 序号应连续 1./2./3.，而不是三个 1.
    expect(find.text('1.'), findsOneWidget);
    expect(find.text('2.'), findsOneWidget);
    expect(find.text('3.'), findsOneWidget);
    expect(_richTextContaining('Topic 页'), findsOneWidget);
    expect(_richTextContaining('后续段落。'), findsOneWidget);
  });

  testWidgets('markdown 渲染：加粗引导行紧贴列表仍各自成块', (tester) async {
    // AI 回复常见形态：「**能用的部分：**」下一行直接是列表，中间无空行
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(markdown: '**能用的部分：**\n- 甲\n- 乙\n\n后续段落。'),
          ),
        ),
      ),
    );
    // 加粗引导行应是独立段落、加粗生效
    expect(_hasBoldFor('能用的部分：'), isTrue, reason: '引导行应保留加粗');
    // 列表应独立成块（两个 bullet）
    expect(find.text('•'), findsNWidgets(2));
    expect(_richTextContaining('甲'), findsOneWidget);
    expect(_richTextContaining('后续段落。'), findsOneWidget);
  });

  testWidgets('markdown 渲染：表格紧贴前一行文字也能识别', (tester) async {
    // AI 回复常见形态：引导句后直接跟表格，中间没有空行
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(
              markdown:
                  '结论如下：\n| 名称 | 数量 |\n| --- | --- |\n| 苹果 | 3 |\n\n后续段落。',
            ),
          ),
        ),
      ),
    );
    expect(find.byType(Table), findsOneWidget);
    expect(_richTextContaining('结论如下：'), findsOneWidget);
    expect(_richTextContaining('名称'), findsOneWidget);
    expect(_richTextContaining('后续段落。'), findsOneWidget);
  });

  testWidgets('markdown 渲染：竖线行缺分隔行时按段落降级', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: MarkdownView(markdown: '| 不是 | 表格 |'),
          ),
        ),
      ),
    );
    expect(find.byType(Table), findsNothing);
    expect(_richTextContaining('不是'), findsOneWidget);
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

/// 在 span 树里找含 needle 的文本对应的样式：优先命中 text 非空的叶子
/// span（覆盖样式），否则回落到携带它的容器 span（如根 span 无 text 时，
/// 其样式对所有 children 生效）。
TextStyle? _styleOf(InlineSpan span, String needle) {
  if (span is! TextSpan) return null;
  final t = span.text;
  if (t != null && t.isNotEmpty && t.contains(needle)) return span.style;
  for (final child in span.children ?? const <InlineSpan>[]) {
    final hit = _styleOf(child, needle);
    if (hit != null) return hit;
  }
  // 走到这里说明 needle 不在任何叶子 text 上，而是整棵树的纯文本（容器
  // span 的样式对所有 children 生效，返回它作为兜底）。
  if (t == null && span.toPlainText().contains(needle)) return span.style;
  return null;
}

bool _hasWeightFor(String needle, FontWeight weight) {
  for (final element in find.byType(RichText).evaluate()) {
    final rt = element.widget as RichText;
    if (_styleOf(rt.text, needle)?.fontWeight == weight) return true;
  }
  return false;
}

bool _hasStrikeFor(String needle) {
  for (final element in find.byType(RichText).evaluate()) {
    final rt = element.widget as RichText;
    if (_styleOf(rt.text, needle)?.decoration == TextDecoration.lineThrough) {
      return true;
    }
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
