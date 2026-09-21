import 'package:appflowy_editor/appflowy_editor.dart';

class BulletedListNodeParser extends NodeParser {
  const BulletedListNodeParser();

  @override
  String get id => BulletedListBlockKeys.type;

  @override
  String transform(Node node, DocumentMarkdownEncoder? encoder) {
    final delta = node.delta ?? Delta()
      ..insert('');
    final children = encoder?.convertNodes(node.children, withIndent: true);
    // elsewhen: 统一输出 `- ` 列表符（源数据惯例），避免 `* ` 与 `- ` 混排。
    String markdown = '- ${DeltaMarkdownEncoder().convert(delta)}\n';
    if (children != null && children.isNotEmpty) {
      markdown += children;
    }

    return markdown;
  }
}
