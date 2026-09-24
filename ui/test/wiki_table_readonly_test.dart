import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_table_block.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 表格浏览态（只读）渲染回归测试。
///
/// 背景：vendor 的表格行高同步依赖 `EditorState.apply`，而 `apply` 在
/// `editable == false` 时直接 return（`!editable || isDisposed`），导致只读
/// 模式下 cell 的 `height` attribute 永不写回，各列按自身内容高度堆叠后
/// 又被 `TableView` 外层 `Row` 垂直居中 → 表格「完全错位」。
/// 修复：只读态使用 Flutter `Table`（TableRow 行内天然等高）重新布局。
class _Harness extends StatelessWidget {
  const _Harness({required this.md, required this.editable});

  final String md;
  final bool editable;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      theme: AppTheme.darkTheme,
      home: Scaffold(
        body: Padding(
          padding: const EdgeInsets.fromLTRB(32, 16, 32, 0),
          child: Align(
            alignment: Alignment.topLeft,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 960),
              child: WikiContentEditor(
                key: UniqueKey(),
                slug: 'test/table',
                contentMd: md,
                editable: editable,
                fontSize: 16,
                lineHeight: 1.5,
                onWikiLinkTap: (_) {},
                onSave: (_) async {},
              ),
            ),
          ),
        ),
      ),
    );
  }
}

const _kTableMd = '''
2. 能力与可复用价值

| 能力域 | 已具备或可复用的内容 | 适合解决的问题 | 证据 |
|---|---|---|---|
| 规格与需求治理 | FR 驱动开发体系；FR 唯一 ID、版本、Acceptance、冲突优先级 `constraints > architecture > feature` | 让长期演进的个人资产有稳定规格入口 | `docs/requirements/README.md` |
| 八字与历法排盘 | `lunar-javascript` 同时支持农历 | 处理公历/农历出生信息 | `CLAUDE.md` |
| 紫微排盘与交叉验证 | `iztro` 中州派配置；封装紫微排盘 | 把八字和紫微输出合并为双系统结论 | `CLAUDE.md` |
''';

/// 窄表格（内容宽度远小于阅读栏，如「资产证据」表）：历史上会被
/// page_block_component 的 `Center(Container(…))` 居中（左缘偏移 64），
/// 与宽表格贴左不一致——回归场景。
const _kNarrowTableMd = '''
| 结论 | 证据路径 | 类型 |
|---|---|---|
| 一些结论内容 | `CLAUDE.md` | 事实 |
| 另一条结论 | `docs/deployment.md` | 未知 |
''';

void main() {
  group('表格只读渲染：行列对齐（表头 + 数据行）', () {
    testWidgets('表头三格与各数据行 cell 顶部 y 一致', (tester) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: false));
      await tester.pumpAndSettle();

      const rows = [
        ['能力域', '已具备或可复用的内容', '适合解决的问题'],
        ['规格与需求治理', 'FR 驱动开发体系', '让长期演进的个人资产'],
        ['八字与历法排盘', 'lunar-javascript', '处理公历/农历出生信息'],
      ];
      for (final row in rows) {
        final ys = <double>[];
        for (final cell in row) {
          final f = find.textContaining(cell, findRichText: true).first;
          ys.add(tester.getTopLeft(f).dy);
        }
        expect(ys[1], closeTo(ys[0], 0.5), reason: '${row[0]} 行：第2列与第1列同行');
        expect(ys[2], closeTo(ys[0], 0.5), reason: '${row[0]} 行：第3列与第1列同行');
      }

      // 数据行应在表头下方（行间有间距），而非之前错位时穿插到表头上方
      final headerY = tester
          .getTopLeft(find.textContaining('能力域', findRichText: true).first)
          .dy;
      final row1Y = tester
          .getTopLeft(find.textContaining('规格与需求治理', findRichText: true).first)
          .dy;
      expect(row1Y, greaterThan(headerY));
    });

    testWidgets('列宽自适应 + 表格靠左', (tester) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: false));
      await tester.pumpAndSettle();

      // 「能力域」短列宽度应明显小于「已具备或可复用的内容」长列
      final shortFinder = find.textContaining('能力域', findRichText: true).first;
      final longFinder = find
          .textContaining('已具备或可复用的内容', findRichText: true)
          .first;
      final shortRect = tester.getRect(shortFinder);
      final longRect = tester.getRect(longFinder);
      expect(
        longRect.width,
        greaterThan(shortRect.width * 1.5),
        reason: '长列应比短列宽（自适应，而非固定 160 均分）',
      );

      // 表格靠左：表格左缘贴内容区左缘（页面 padding 32 + 编辑器 padding 24 = 56）
      final tableFinder = find.byType(Table);
      expect(tableFinder, findsOneWidget);
      final tableLeft = tester.getTopLeft(tableFinder).dx;
      expect(tableLeft, closeTo(56, 1), reason: '表格从内容区左缘起排，不居中');

      // 第二个元素紧跟第一列 cell 之后（列与列无居中缝隙）
      final colTexts = <double>[];
      for (final t in ['能力域', '已具备或可复用的内容', '适合解决的问题']) {
        colTexts.add(
          tester
              .getTopLeft(find.textContaining(t, findRichText: true).first)
              .dx,
        );
      }
      expect(colTexts[2], greaterThan(colTexts[1]));
      expect(colTexts[1], greaterThan(colTexts[0]));
    });

    testWidgets('cell 无底色（透明，背景与正文一致）', (tester) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: false));
      await tester.pumpAndSettle();

      // 表格区不应出现 surface1 色块（旧实现给每个 cell 上 surface1 底色）
      final paintFinder = find.byWidgetPredicate(
        (w) =>
            w is Container &&
            w.color == AppTheme.surface1 &&
            w.decoration == null,
      );
      expect(paintFinder, findsNothing, reason: 'cell 不再有独立底色，避免与页面背景割裂');
    });

    testWidgets('行内 code 样式随正文（monospace）', (tester) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: false));
      await tester.pumpAndSettle();

      // FR 行内的 `constraints > architecture > feature` 是行内 code
      final codeFinder = find.textContaining(
        'constraints > architecture > feature',
        findRichText: true,
      );
      expect(codeFinder, findsOneWidget);
      final sp = tester.widget<RichText>(codeFinder.first).text as TextSpan;
      final codeSpan = sp.children
          ?.whereType<TextSpan>()
          .where((s) => s.text?.contains('constraints') ?? false)
          .firstOrNull;
      expect(codeSpan, isNotNull);
      expect(
        codeSpan!.style?.fontFamily,
        'monospace',
        reason: '表格内行内 code 走等宽',
      );
    });

    testWidgets('窄表格同样贴左，不被 Center 居中', (tester) async {
      await tester.pumpWidget(
        const _Harness(md: _kNarrowTableMd, editable: false),
      );
      await tester.pumpAndSettle();

      // 表格左缘贴内容区左缘（页面 padding 32 + 编辑器 padding 24 = 56）。
      // 表内容窄时若不强制块占满整宽，会被 page_block_component 的
      // `Center(Container(maxWidth, padding))` 居中（左缘 ≈ 120）。
      final tableFinder = find.byType(Table);
      expect(tableFinder, findsOneWidget);
      final tableLeft = tester.getTopLeft(tableFinder).dx;
      expect(tableLeft, closeTo(56, 1), reason: '窄表格也应从内容区左缘起排，与宽表格一致');

      // 表内同数据行 3 列顶部 y 对齐（只读 Table 行内等高）
      const rowTexts = ['一些结论内容', 'CLAUDE.md', '事实'];
      final ys = <double>[];
      for (final t in rowTexts) {
        final f = find.descendant(
          of: tableFinder,
          matching: find.textContaining(t, findRichText: true),
        );
        ys.add(tester.getTopLeft(f.first).dy);
      }
      expect(ys[1], closeTo(ys[0], 0.5), reason: '窄表格同数据行第2列同行');
      expect(ys[2], closeTo(ys[0], 0.5), reason: '窄表格同数据行第3列同行');
    });
  });

  group('表格组件路由', () {
    testWidgets('只读态渲染 WikiReadonlyTableBlockComponent', (tester) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: false));
      await tester.pumpAndSettle();
      expect(
        find.byType(WikiReadonlyTableBlockComponent),
        findsOneWidget,
        reason: '只读态应使用自定义等高表格组件',
      );
    });

    testWidgets('编辑态不渲染只读表格组件（委托 vendor TableBlockComponentBuilder）', (
      tester,
    ) async {
      await tester.pumpWidget(const _Harness(md: _kTableMd, editable: true));
      await tester.pumpAndSettle();
      expect(
        find.byType(WikiReadonlyTableBlockComponent),
        findsNothing,
        reason: '编辑态应委托 vendor 表格组件（apply 生效，行高同步正常）',
      );
    });
  });
}
