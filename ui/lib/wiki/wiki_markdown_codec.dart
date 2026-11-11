import 'package:appflowy_editor/appflowy_editor.dart';

import 'wikilink_syntax.dart';
import 'wiki_markdown_normalize.dart';

/// wiki 正文 markdown ⇄ AppFlowy Document 的生产 codec 包装（§5.2）。
///
/// 解码：注册 [WikilinkInlineSyntax]，`[[target]]` / `[[target|alias]]`
/// 在块级 markdown 解析即产出 `<wikilink>` 元素，再由 vendored
/// DeltaMarkdownDecoder 映射为 delta 行内 `wikilink` 属性。
/// 编码：vendored DeltaMarkdownEncoder 把该属性还原回 `[[target|alias]]`，
/// 无需自定义 NodeParser。
///
/// 解码前先跑 [normalizeTableBlanks]：粘贴/导入链路会把表格行之间垫出
/// 空行，GFM 要求表头行与分隔行相邻，隔断后整张表退化成段落。
Document wikiMarkdownToDocument(String markdown) {
  return markdownToDocument(
    normalizeTableBlanks(markdown),
    inlineSyntaxes: [WikilinkInlineSyntax()],
  );
}

String wikiDocumentToMarkdown(Document document) {
  return documentToMarkdown(document);
}
