import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../theme/app_theme.dart';
import '../widgets/wiki_ai_chat_panel.dart';

/// 自定义 block「AI 对话」（设计 §7 Form A：页尾对话块）。
///
/// 复用 [WikiAiChatPanel]（与页面详情页底部同一聊天 UI），以节点形式嵌在
/// 正文文档流末尾。聊天内容不进 markdown（编码器无对应 NodeParser，静默跳过），
/// 按页面 slug 走 `ensure/archive_wiki_page_chat` 独立持久化。
class WikiChatBlockKeys {
  const WikiChatBlockKeys._();

  static const String type = 'wiki_chat';

  /// 节点上的页面 slug：决定聊天会话落哪个页面
  static const String slugAttribute = 'slug';
}

/// 构造正文尾部的对话块节点（挂当前页面 slug）。
Node wikiChatNode({required String slug}) {
  return Node(
    type: WikiChatBlockKeys.type,
    attributes: {WikiChatBlockKeys.slugAttribute: slug},
  );
}

class WikiChatBlockComponentBuilder extends BlockComponentBuilder {
  WikiChatBlockComponentBuilder({super.configuration});

  @override
  BlockComponentWidget build(BlockComponentContext blockComponentContext) {
    final node = blockComponentContext.node;

    return WikiChatBlockComponent(
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
  BlockComponentValidate get validate =>
      (node) => node.children.isEmpty;
}

class WikiChatBlockComponent extends BlockComponentStatefulWidget {
  const WikiChatBlockComponent({
    super.key,
    required super.node,
    super.showActions,
    super.actionBuilder,
    super.actionTrailingBuilder,
    super.configuration = const BlockComponentConfiguration(),
  });

  @override
  State<WikiChatBlockComponent> createState() => _WikiChatBlockComponentState();
}

class _WikiChatBlockComponentState extends State<WikiChatBlockComponent>
    with SelectableMixin, BlockComponentConfigurable {
  @override
  BlockComponentConfiguration get configuration => widget.configuration;

  @override
  Node get node => widget.node;

  final _panelKey = GlobalKey();

  RenderBox? get _renderBox => context.findRenderObject() as RenderBox?;

  @override
  Widget build(BuildContext context) {
    final slug =
        widget.node.attributes[WikiChatBlockKeys.slugAttribute]?.toString() ??
        '';

    Widget child = Container(
      // 块自身带边距，避免与正文段落贴死；面板内部样式由 WikiAiChatPanel 负责
      margin: const EdgeInsets.only(top: AppTheme.space2),
      child: WikiAiChatPanel(slug: slug),
    );

    child = Padding(key: _panelKey, padding: padding, child: child);

    final editorState = context.read<EditorState>();

    child = BlockSelectionContainer(
      node: node,
      delegate: this,
      listenable: editorState.selectionNotifier,
      remoteSelection: editorState.remoteSelections,
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

    if (widget.showActions && widget.actionBuilder != null) {
      child = BlockComponentActionWrapper(
        node: node,
        actionBuilder: widget.actionBuilder!,
        actionTrailingBuilder: widget.actionTrailingBuilder,
        child: child,
      );
    }

    child = Padding(padding: margin, child: child);

    return child;
  }

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
    if (_renderBox == null) {
      return null;
    }
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
    if (_renderBox == null) {
      return [];
    }
    final parentBox = context.findRenderObject();
    final panelBox = _panelKey.currentContext?.findRenderObject();
    if (parentBox is RenderBox && panelBox is RenderBox) {
      return [
        (shiftWithBaseOffset
                ? panelBox.localToGlobal(Offset.zero, ancestor: parentBox)
                : Offset.zero) &
            panelBox.size,
      ];
    }

    return [Offset.zero & _renderBox!.size];
  }

  @override
  Selection getSelectionInRange(Offset start, Offset end) =>
      Selection.single(path: widget.node.path, startOffset: 0, endOffset: 1);

  @override
  Offset localToGlobal(Offset offset, {bool shiftWithBaseOffset = false}) =>
      _renderBox!.localToGlobal(offset);

  @override
  TextDirection textDirection() {
    return TextDirection.ltr;
  }
}
