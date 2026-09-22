import 'package:markdown/markdown.dart' as md;

/// 匹配 `[[target]]` / `[[target|alias]]` 的 wikilink 行内语法（§5.2）。
///
/// 产出 `<wikilink>` 元素：`attributes['wikilink']` 存跳转目标 slug，
/// 子文本节点为展示文本（alias）。vendored DeltaMarkdownDecoder 据此生成
/// delta 行内属性；DeltaMarkdownEncoder 再由该属性还原 `[[target|alias]]`。
///
/// 只匹配合法形态：target 与 alias 都必须非空、最多一个 `|`。未闭合
/// `[[`、空 target/alias、多余 `|` 的形态不匹配，按原样文本保留
/// （保证无 wikilink 解析的旧样例仍然保真）。
///
/// ⚠️ `onMatch` 永不返回 false：markdown 包中只要正则匹配 `tryMatch` 即
/// 返回 true，`onMatch` 返回 false 会让 parse 循环不复位位置 → 死循环。
class WikilinkInlineSyntax extends md.InlineSyntax {
  WikilinkInlineSyntax()
      : super(r'\[\[([^\[\]|]+(?:\|[^\[\]|]+)?)\]\]');

  @override
  bool onMatch(md.InlineParser parser, Match match) {
    final raw = match.group(1);
    if (raw == null) {
      // 防御分支：原样保留匹配文本，绝不返回 false
      parser.addNode(md.Text(match.group(0)!));
      return true;
    }
    final sep = raw.indexOf('|');
    final target = sep == -1 ? raw : raw.substring(0, sep);
    final alias = sep == -1 ? raw : raw.substring(sep + 1);
    final element = md.Element('wikilink', [md.Text(alias)]);
    element.attributes['wikilink'] = target;
    parser.addNode(element);
    return true;
  }
}