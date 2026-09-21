import 'package:appflowy_editor/appflowy_editor.dart';

class QuoteNodeParser extends NodeParser {
  const QuoteNodeParser();

  @override
  String get id => QuoteBlockKeys.type;

  @override
  String transform(Node node, DocumentMarkdownEncoder? encoder) {
    final delta = node.delta ?? Delta()
      ..insert('');
    final children = encoder?.convertNodes(node.children, withIndent: true);
    // elsewhen: 多行引用（delta 内嵌 \n）必须逐行打 `>`，否则续行会
    // 在往返时滑出引用块。
    final text = DeltaMarkdownEncoder().convert(delta);
    final lines = text.split('\n');
    String markdown = lines.map((line) => line.isEmpty ? '>' : '> $line').join('\n');
    if (children != null && children.isNotEmpty) {
      markdown = '$markdown\n$children';
    } else {
      markdown = '$markdown\n';
    }

    return markdown;
  }
}
