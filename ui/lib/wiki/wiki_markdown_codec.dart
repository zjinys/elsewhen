import 'package:appflowy_editor/appflowy_editor.dart';

import 'wikilink_syntax.dart';

/// wiki 正文 markdown ⇄ AppFlowy Document 的生产 codec 包装（§5.2）。
///
/// 解码：注册 [WikilinkInlineSyntax]，`[[target]]` / `[[target|alias]]`
/// 在块级 markdown 解析即产出 `<wikilink>` 元素，再由 vendored
/// DeltaMarkdownDecoder 映射为 delta 行内 `wikilink` 属性。
/// 编码：vendored DeltaMarkdownEncoder 把该属性还原回 `[[target|alias]]`，
/// 无需自定义 NodeParser。
Document wikiMarkdownToDocument(String markdown) {
  return markdownToDocument(markdown, inlineSyntaxes: [WikilinkInlineSyntax()]);
}

String wikiDocumentToMarkdown(Document document) {
  return documentToMarkdown(document);
}
