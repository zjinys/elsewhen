import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:appflowy_editor/appflowy_editor.dart';

import '../lib/wiki/wiki_markdown_codec.dart';

/// GFM 表格在「行间被空行隔断」时的解码保真（回归：知识页表格不渲染）。
///
/// 背景：粘贴/导入链路（pastePlainText 每行一段落 + encoder 块间空行）
/// 产出形如 `| 表头 |\n\n|---|\n\n| 行 |` 的 markdown。GFM 要求表头行与
/// 分隔行相邻，隔断后整张表退化为 `|` 段落，知识页浏览态不再渲染表格。
/// codec 现在在解码前归一化表格块内空行（wiki_markdown_normalize.dart）。
void main() {
  group('表格块内空行归一化', () {
    test('行间空行的表格解码为 table 节点', () {
      const md = '''
| 痛点 | 典型症状 |

|---|---|

| r1 | s1 |

| r2 | s2 |
''';
      final doc = wikiMarkdownToDocument(md);
      final types = doc.root.children.map((n) => n.type).toList();
      expect(types, ['table']);
    });

    test('无空行的紧凑表格不受影响', () {
      const md = '''
| A | B |
|---|---|
| r1 | s1 |
''';
      final doc = wikiMarkdownToDocument(md);
      expect(doc.root.children.map((n) => n.type), ['table']);
    });

    test('真实知识页（english-doc-revision）五张表全部还原', () {
      // fixtures 里放归一化前的原始导入文本（含行间空行）。
      final raw = File('test/fixtures/wiki_md/table_blank_lines_import.md')
          .readAsStringSync();
      final doc = wikiMarkdownToDocument(raw);
      expect(doc.root.children.where((n) => n.type == 'table').length, 5);
      // 保存 → 重解析仍保真
      final out = wikiDocumentToMarkdown(doc);
      final reparsed = wikiMarkdownToDocument(out);
      expect(
        reparsed.root.children.where((n) => n.type == 'table').length,
        5,
      );
    });

    test('单独一行 | 文本不被误并入表格', () {
      const md = '引用：\n\n| 只是普通文本 |\n\n后续段落';
      final doc = wikiMarkdownToDocument(md);
      final types = doc.root.children.map((n) => n.type).toList();
      expect(types, everyElement('paragraph'));
      // 归一化后单行 | 文本与前段连为一个段落（"| " 行吞掉了后面的空行），
      // 但文本内容不丢，仍降级为段落展示。
      final text = doc.root.children.last.delta?.toPlainText();
      expect(text, contains('| 只是普通文本 |'));
      expect(text, contains('后续段落'));
    });

    test('代码围栏内的管道行与空行不动', () {
      const md = '```\n| a | b |\n\n| c | d |\n```\n\n正文';
      final doc = wikiMarkdownToDocument(md);
      expect(doc.root.children.first.type, 'code');
      expect(
        doc.root.children.first.delta?.toPlainText(),
        '| a | b |\n\n| c | d |',
      );
    });
  });
}
