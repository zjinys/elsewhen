import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';

import '../theme/app_theme.dart';

/// 自定义 block「代码块」降级组件（设计 §11 Q4）。
///
/// vendor 01eccc6 无 code 块组件，`code` 节点默认渲染为 30px placeholder 占位。
/// 这里注册一个**只读降级块**：等宽字体 + 深色底 + 语言角标 + 复制按钮；
/// 不升级 vendor（避免引入代码块组件的体积与行为漂移）。编解码管线已保真
/// （往返测试覆盖），此组件只负责展示与复制。
class WikiCodeBlockKeys {
  const WikiCodeBlockKeys._();

  static const String type = 'code';

  /// 节点上的语言属性（markdown 围栏语言，如 `dart`）
  static const String languageAttribute = 'language';
}

class WikiCodeBlockComponentBuilder extends BlockComponentBuilder {
  WikiCodeBlockComponentBuilder({super.configuration});

  @override
  BlockComponentWidget build(BlockComponentContext blockComponentContext) {
    final node = blockComponentContext.node;

    return WikiCodeBlockComponent(
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
  BlockComponentValidate get validate => (node) => node.children.isEmpty;
}

class WikiCodeBlockComponent extends BlockComponentStatefulWidget {
  const WikiCodeBlockComponent({
    super.key,
    required super.node,
    super.showActions,
    super.actionBuilder,
    super.actionTrailingBuilder,
    super.configuration = const BlockComponentConfiguration(),
  });

  @override
  State<WikiCodeBlockComponent> createState() => _WikiCodeBlockComponentState();
}

class _WikiCodeBlockComponentState extends State<WikiCodeBlockComponent>
    with SelectableMixin, BlockComponentConfigurable {
  @override
  BlockComponentConfiguration get configuration => widget.configuration;

  @override
  Node get node => widget.node;

  final _panelKey = GlobalKey();

  RenderBox? get _renderBox => context.findRenderObject() as RenderBox?;

  /// 代码内容只读展示，不参与 editable 开关（禁用兜底时仍可复制）。
  @override
  Widget build(BuildContext context) {
    final code = node.delta?.toPlainText() ?? '';
    final language =
        node.attributes[WikiCodeBlockKeys.languageAttribute]?.toString() ?? '';

    Widget child = Container(
      key: _panelKey,
      margin: const EdgeInsets.symmetric(vertical: AppTheme.space2),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3, width: 1),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          // 头部：语言角标 + 复制按钮
          Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(
              horizontal: AppTheme.space3,
              vertical: AppTheme.space1,
            ),
            color: AppTheme.surface3,
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    language.isEmpty ? '代码' : language,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                      fontFamily: 'monospace',
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                _buildCopyButton(code),
              ],
            ),
          ),
          // 代码体：等宽、不折行（横向滚动），结尾无多余换行
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.all(AppTheme.space3),
            reverse: true,
            child: Text(
              code,
              style: TextStyle(
                fontFamily: 'monospace',
                fontSize: 13,
                height: 1.5,
                color: AppTheme.textPrimary,
              ),
            ),
          ),
        ],
      ),
    );

    return BlockSelectionContainer(
      node: node,
      delegate: this,
      listenable: context.read<EditorState>().selectionNotifier,
      remoteSelection: null,
      blockColor: context.read<EditorState>().editorStyle.selectionColor,
      cursorColor: context.read<EditorState>().editorStyle.cursorColor,
      selectionColor: context.read<EditorState>().editorStyle.selectionColor,
      supportTypes: const [
        BlockSelectionType.block,
        BlockSelectionType.cursor,
        BlockSelectionType.selection,
      ],
      child: child,
    );
  }

  Widget _buildCopyButton(String code) {
    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: code));
        ScaffoldMessenger.of(context)
          ..hideCurrentSnackBar()
          ..showSnackBar(
            const SnackBar(
              content: Text('代码已复制'),
              duration: Duration(seconds: 1),
            ),
          );
      },
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AppTheme.space2,
          vertical: AppTheme.space1,
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.copy_rounded,
              size: 14,
              color: AppTheme.textSecondary,
            ),
            const SizedBox(width: AppTheme.space1),
            Text(
              '复制',
              style: TextStyle(
                fontSize: 11,
                color: AppTheme.textSecondary,
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ── SelectableMixin：块级占位选区（仅支撑聚焦/整体选中，不逐字选） ──

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
    if (_renderBox == null) return null;
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
    if (_renderBox == null) return [];
    final panelBox = _panelKey.currentContext?.findRenderObject();
    if (panelBox is RenderBox) {
      return [Offset.zero & panelBox.size];
    }
    return [Offset.zero & _renderBox!.size];
  }

  @override
  Selection getSelectionInRange(Offset start, Offset end) => Selection.single(
        path: widget.node.path,
        startOffset: 0,
        endOffset: 1,
      );

  @override
  Offset localToGlobal(
    Offset offset, {
    bool shiftWithBaseOffset = false,
  }) =>
      _renderBox!.localToGlobal(offset);

  @override
  TextDirection textDirection() => TextDirection.ltr;
}