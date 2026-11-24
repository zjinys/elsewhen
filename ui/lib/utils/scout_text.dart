import 'package:html/dom.dart' as dom;
import 'package:html/parser.dart' as html;

const _blocks = {
  'p',
  'div',
  'li',
  'br',
  'section',
  'article',
  'h1',
  'h2',
  'h3',
  'tr',
};
const _hidden = {'script', 'style', 'template', 'noscript'};
const _navigation = {'discussion', 'link', 'comments'};

/// Display-only normalization. Original evidence payloads remain unchanged.
String cleanScoutText(Object? value, {bool feedNavigation = false}) {
  if (value is! String) return '';
  var text = value;
  for (var layer = 0; layer < 4; layer++) {
    final output = StringBuffer();
    void visit(dom.Node node) {
      if (node is dom.Text) {
        output.write(node.data);
        return;
      }
      final tag = node is dom.Element ? node.localName : null;
      if (_hidden.contains(tag)) return;
      if (feedNavigation &&
          node is dom.Element &&
          tag == 'a' &&
          _navigation.contains(node.text.trim().toLowerCase())) {
        return;
      }
      if (_blocks.contains(tag)) output.write(' ');
      for (final child in node.nodes) {
        visit(child);
      }
      if (_blocks.contains(tag)) output.write(' ');
    }

    visit(html.parseFragment(text));
    final cleaned = output.toString().replaceAll(RegExp(r'\s+'), ' ').trim();
    if (cleaned == text) break;
    text = cleaned;
  }
  if (feedNavigation) {
    text = text
        .replaceFirst(
          RegExp(
            r'(?:\s*\|\s*(?:Discussion|Link|Comments))+\s*$',
            caseSensitive: false,
          ),
          '',
        )
        .replaceAll(RegExp(r'(?:\s*\|\s*)+$'), '')
        .trim();
  }
  return text;
}
