import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:collection/collection.dart';
import 'package:markdown/markdown.dart' as md;

/// 围栏/缩进代码块 → `code` 节点（type: 'code'，attrs: { delta, language }）。
///
/// elsewhen M2 补丁：上游 01eccc6 只有 encode 侧的 [CodeBlockNodeParser]，
/// decode 侧缺失，导致 ``` 代码块在往返时被整体丢弃。这里补上 `<pre><code>`
/// 的解析，language 取自 `<code class="language-xxx">`。
class MarkdownCodeBlockParserV2 extends CustomMarkdownParser {
  const MarkdownCodeBlockParserV2();

  static const String codeBlockType = 'code';

  @override
  List<Node> transform(
    md.Node element,
    List<CustomMarkdownParser> parsers, {
    MarkdownListType listType = MarkdownListType.unknown,
    int? startNumber,
  }) {
    if (element is! md.Element || element.tag != 'pre') {
      return [];
    }

    final codeElement = element.children
        ?.whereType<md.Element>()
        .firstWhereOrNull((child) => child.tag == 'code');
    if (codeElement == null) {
      return [];
    }

    final languageClass = codeElement.attributes['class'] ?? '';
    var language = '';
    if (languageClass.startsWith('language-')) {
      language = languageClass.substring('language-'.length);
    }

    var delta = DeltaMarkdownDecoder().convertNodes(codeElement.children);
    // 围栏/缩进的代码内容尾带一个结构换行，剔掉避免往返多出空行。
    final plain = delta.toPlainText();
    if (plain.endsWith('\n')) {
      delta = Delta()
        ..insert(plain.substring(0, plain.length - 1));
    }

    return [
      Node(
        type: codeBlockType,
        attributes: {
          'delta': delta.toJson(),
          'language': language,
        },
      ),
    ];
  }
}