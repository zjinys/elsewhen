import 'dart:convert';

import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:collection/collection.dart';

class DocumentMarkdownEncoder extends Converter<Document, String> {
  DocumentMarkdownEncoder({
    this.parsers = const [],
    this.lineBreak = '',
  });

  final List<NodeParser> parsers;
  final String lineBreak;

  @override
  String convert(Document input) {
    final buffer = StringBuffer();
    final children = input.root.children;
    String? prevType;
    for (final node in children) {
      final NodeParser? parser = parsers.firstWhereOrNull(
        (element) => element.id == node.type,
      );
      if (parser != null) {
        final text = parser.transform(node, this);
        // elsewhen: 块级 markdown 语义需要块间空行（否则「段落\n---」会被
        // 重解析成 setext 二级标题、引用后段落会掉进引用、分隔列表会合并）。
        // 相邻列表项属于同一个 markdown 列表，不加空行。
        if (buffer.isNotEmpty && _needsBlankLineBetween(prevType, node.type)) {
          buffer.write('\n');
        }
        buffer.write(text);
        prevType = node.type;
        if (lineBreak.isNotEmpty && node.id != children.last.id) {
          buffer.write(lineBreak);
        }
      }
    }

    return buffer.toString();
  }

  static const _listItemTypes = <String>{
    'bulleted_list',
    'numbered_list',
    'todo_list',
  };

  static bool _needsBlankLineBetween(String? prevType, String nextType) {
    if (prevType == null) {
      return false;
    }
    // 同一 markdown 列表的相邻条目之间不加空行。
    if (_listItemTypes.contains(prevType) && _listItemTypes.contains(nextType)) {
      return false;
    }
    return true;
  }

  String convertNodes(
    List<Node> nodes, {
    bool withIndent = false,
  }) {
    final result = convert(
      Document(root: pageNode(children: nodes.map((n) => n.deepCopy()))),
    );
    if (result.isNotEmpty && withIndent) {
      // 嵌套缩进用 tab（=4 列）。2 空格对 `1. `（marker 宽 3）会滑出列表，不安全。
      return result
          .split('\n')
          .map((e) => e.isNotEmpty ? '\t$e' : e)
          .join('\n');
    }

    return result;
  }
}
