import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:markdown/markdown.dart' as md;

class MarkdownOrderedListParserV2 extends CustomMarkdownParser {
  const MarkdownOrderedListParserV2();

  @override
  List<Node> transform(
    md.Node element,
    List<CustomMarkdownParser> parsers, {
    MarkdownListType listType = MarkdownListType.unknown,
    int? startNumber,
  }) {
    if (element is! md.Element) {
      return [];
    }

    if (element.tag != 'ol') {
      return [];
    }

    final startAttr = element.attributes['start'];
    final listStart = startAttr != null ? int.tryParse(startAttr) : null;

    // flatten the list，逐条赋号：start + i（无 start 时 1 起）。
    // 否则所有条目共用同一 start number，往返时列表编号塌缩成同号。
    final children = element.children ?? const [];
    final result = <Node>[];
    for (var i = 0; i < children.length; i++) {
      result.addAll(
        parseElementChildren(
          [children[i]],
          parsers,
          listType: MarkdownListType.ordered,
          startNumber: listStart != null ? listStart + i : i + 1,
        ),
      );
    }
    return result;
  }
}
