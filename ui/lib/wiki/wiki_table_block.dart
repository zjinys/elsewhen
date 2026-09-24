import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../theme/app_theme.dart';

/// 表格块「只读降级」组件（浏览态排版修复）。
///
/// ## 背景
/// vendor 的表格行高同步依赖 `updateRowHeight` → `EditorState.apply`，
/// 而 `apply` 第 406 行 `if (!editable || isDisposed) return;` —— **只读模式
/// 下行列高永不回写**：每个 cell 的 `height` attribute 保持空值，回落到
/// `minHeight: rowHeight(40)`，长文本内容把 cell 撑到各自的实际高度
/// （16~208px 不等）。`TableView` 以「每列一个 `Column`」布局，列高度不同时
/// 外层 `Row` 默认 `crossAxisAlignment.center` 垂直居中 → 列整体错位。
///
/// ## 方案
/// 只读模式下不依赖 vendor 的行高回写，改用 Flutter `Table` 重新布局：
/// - `TableRow` 天然「行内等高」——同一行的所有 cell 高度取该行最大值，
///   行列必然对齐，彻底绕开 vendor 只读失效的回写机制；
/// - 列宽随内容自适应：短列自然收窄、长列按需放宽，单列上限 320（防无空格
///   超长文本/路径把整个表格撑出阅读栏，超限内容在格内换行）；
/// - cell 无底色（透明）：背景与正文一致，只在 surface3 边框上区分单元格，
///   避免「表格底 ≠ 页面底」的割裂观感（AppFlowy 桌面端同款）；
/// - 外层 `SizedBox(width: double.infinity)` 让块占满内容区整宽：否则
///   `PageBlockComponent` 的 `Center(Container(maxWidth, padding))` 会把
///   内容窄的表格块收缩并水平居中（左缘偏移 64px），与宽表格贴左观感割裂；
/// - 每个 cell 内部仍走 `editorState.renderer.build` 渲染该格的 paragraph，
///   行内 code / href / wikilink 等样式与正文完全一致（textStyleConfiguration
///   与 textSpanDecorator 全局生效）。
///
/// 编辑模式（`editable == true`）下 `apply` 生效、行高同步正常，直接委托
/// vendor 的 `TableBlockComponentBuilder`，交互行为零改动。
class WikiReadonlyTableBlockComponentBuilder extends BlockComponentBuilder {
  WikiReadonlyTableBlockComponentBuilder({super.configuration});

  final TableBlockComponentBuilder _vendor = TableBlockComponentBuilder();

  @override
  BlockComponentWidget build(BlockComponentContext blockComponentContext) {
    final editable = Provider.of<EditorState>(
      blockComponentContext.buildContext,
      listen: false,
    ).editable;
    if (editable) {
      return _vendor.build(blockComponentContext);
    }
    final node = blockComponentContext.node;
    return WikiReadonlyTableBlockComponent(
      key: node.key,
      node: node,
      showActions: showActions(node),
      configuration: configuration,
      actionBuilder: (context, state) =>
          actionBuilder(blockComponentContext, state),
      actionTrailingBuilder: (context, state) =>
          actionTrailingBuilder(blockComponentContext, state),
    );
  }

  @override
  BlockComponentValidate get validate => (node) {
    if (node.type != TableBlockKeys.type || node.attributes.isEmpty) {
      return false;
    }
    final colsLen = node.attributes[TableBlockKeys.colsLen];
    final rowsLen = node.attributes[TableBlockKeys.rowsLen];
    return colsLen is int &&
        rowsLen is int &&
        node.children.length == colsLen * rowsLen;
  };
}

class WikiReadonlyTableBlockComponent extends BlockComponentStatefulWidget {
  const WikiReadonlyTableBlockComponent({
    super.key,
    required super.node,
    super.showActions,
    super.actionBuilder,
    super.actionTrailingBuilder,
    super.configuration = const BlockComponentConfiguration(),
  });

  @override
  State<WikiReadonlyTableBlockComponent> createState() =>
      _WikiReadonlyTableBlockComponentState();
}

class _WikiReadonlyTableBlockComponentState
    extends State<WikiReadonlyTableBlockComponent>
    with SelectableMixin, BlockComponentConfigurable {
  @override
  BlockComponentConfiguration get configuration => widget.configuration;

  @override
  Node get node => widget.node;

  @override
  Widget build(BuildContext context) {
    final editorState = context.read<EditorState>();
    final tableNode = TableNode(node: node);
    final colsLen = tableNode.colsLen;
    final rowsLen = tableNode.rowsLen;

    if (colsLen == 0 || rowsLen == 0) {
      return const SizedBox.shrink();
    }

    Widget child = SizedBox(
      width: double.infinity,
      child: SingleChildScrollView(
        // 列多或窄视口时横向滚动兜底，不挤压正文阅读栏
        scrollDirection: Axis.horizontal,
        child: Table(
          // 列宽随内容自适应：短列（如「能力域」）自然收窄、长列（如「已具备
          // 的内容」）按需要放宽；上限 320 防止单个超长文本（无空格英文/路径）
          // 把表格撑出阅读栏，超限内容在格内换行。
          defaultColumnWidth: const MinColumnWidth(
            IntrinsicColumnWidth(),
            FixedColumnWidth(320),
          ),
          border: TableBorder.all(color: AppTheme.surface3, width: 1),
          children: List.generate(rowsLen, (row) {
            return TableRow(
              children: List.generate(colsLen, (col) {
                final cellNode = tableNode.getCell(col, row);
                final paragraph = cellNode.children.firstOrNull;
                // cell 无底色（透明）：只在边框上与单元格区分，背景与正文一致，
                // 避免「表格底 ≠ 页面底」的割裂观感（AppFlowy 桌面端同款）。
                return Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 6,
                  ),
                  child: Align(
                    alignment: Alignment.topLeft,
                    child: paragraph == null
                        ? const SizedBox.shrink()
                        : editorState.renderer.build(context, paragraph),
                  ),
                );
              }),
            );
          }),
        ),
      ),
    );

    return BlockSelectionContainer(
      node: node,
      delegate: this,
      listenable: editorState.selectionNotifier,
      remoteSelection: null,
      blockColor: editorState.editorStyle.selectionColor,
      cursorColor: editorState.editorStyle.cursorColor,
      selectionColor: editorState.editorStyle.selectionColor,
      supportTypes: const [
        BlockSelectionType.block,
        BlockSelectionType.cursor,
        BlockSelectionType.selection,
      ],
      child: child,
    );
  }

  // ── SelectableMixin：块级整体选区（只读态仅支撑聚焦/点击，不逐字选） ──

  @override
  Position start() => Position(path: widget.node.path);

  @override
  Position end() => Position(path: widget.node.path, offset: 1);

  @override
  Position getPositionInOffset(Offset start) => end();

  @override
  bool get shouldCursorBlink => false;

  @override
  CursorStyle get cursorStyle => CursorStyle.cover;

  @override
  Rect getBlockRect({bool shiftWithBaseOffset = false}) {
    return getRectsInSelection(Selection.invalid()).first;
  }

  @override
  Rect? getCursorRectInPosition(
    Position position, {
    bool shiftWithBaseOffset = false,
  }) {
    final box = context.findRenderObject();
    if (box is! RenderBox) return null;
    return getRectsInSelection(
      Selection.collapsed(position),
      shiftWithBaseOffset: shiftWithBaseOffset,
    ).firstOrNull;
  }

  @override
  List<Rect> getRectsInSelection(
    Selection selection, {
    bool shiftWithBaseOffset = false,
  }) {
    final box = context.findRenderObject();
    if (box is! RenderBox) return const [];
    return [Offset.zero & box.size];
  }

  @override
  Selection getSelectionInRange(Offset start, Offset end) =>
      Selection.single(path: widget.node.path, startOffset: 0, endOffset: 1);

  @override
  Offset localToGlobal(Offset offset, {bool shiftWithBaseOffset = false}) {
    final box = context.findRenderObject();
    if (box is! RenderBox) return Offset.zero;
    return box.localToGlobal(offset);
  }

  @override
  TextDirection textDirection() => TextDirection.ltr;
}
