part of '../markdown_view.dart';

//! 清洗后正文 → 渲染块解析 + 块模型

/// 把清洗后的正文切成渲染块（标题/列表/引用/代码块/分隔线/段落）。
List<_Block> _parseBlocks(String markdown) {
  final lines = stripToolProtocolForDisplay(markdown).split('\n');

  final blocks = <_Block>[];
  var i = 0;
  while (i < lines.length) {
    final line = lines[i];

    // 代码块
    if (line.trimLeft().startsWith('```')) {
      final buf = <String>[];
      i++;
      while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
        buf.add(lines[i]);
        i++;
      }
      i++; // 跳过闭合 ```
      blocks.add(_CodeBlock(code: buf.join('\n')));
      continue;
    }

    // 分隔线
    if (RegExp(r'^\s*([-*_])\s*(\1\s*){2,}$').hasMatch(line)) {
      blocks.add(_Rule());
      i++;
      continue;
    }

    // 标题
    final headingMatch = RegExp(r'^(#{1,6})\s+(.*)$').firstMatch(line);
    if (headingMatch != null) {
      blocks.add(
        _Heading(
          level: headingMatch.group(1)!.length,
          text: headingMatch.group(2)!.trim(),
        ),
      );
      i++;
      continue;
    }

    // 引用
    if (line.trimLeft().startsWith('>')) {
      final buf = <String>[line.trimLeft().substring(1).trim()];
      i++;
      while (i < lines.length && lines[i].trimLeft().startsWith('>')) {
        buf.add(lines[i].trimLeft().substring(1).trim());
        i++;
      }
      blocks.add(_Quote(text: buf.join('\n')));
      continue;
    }

    // 无序列表：项之间允许空行，遇到下一个列表项就续上。
    if (RegExp(r'^\s*[-*]\s+').hasMatch(line)) {
      final buf = <String>[];
      while (i < lines.length) {
        if (!RegExp(r'^\s*[-*]\s+').hasMatch(lines[i])) {
          if (lines[i].trim().isEmpty) {
            var next = i + 1;
            while (next < lines.length && lines[next].trim().isEmpty) {
              next++;
            }
            if (next < lines.length &&
                RegExp(r'^\s*[-*]\s+').hasMatch(lines[next])) {
              i = next;
              continue;
            }
          }
          break;
        }
        buf.add(lines[i].replaceFirst(RegExp(r'^\s*[-*]\s+'), ''));
        i++;
      }
      blocks.add(_List(items: buf, ordered: false));
      continue;
    }

    // 有序列表：项之间允许空行（AI 常在每项前留空行排版），遇到下一个
    // 编号项就续上；空行后面不是列表项才真正结束。
    if (RegExp(r'^\s*\d+[.)]\s+').hasMatch(line)) {
      final buf = <String>[];
      while (i < lines.length) {
        final itemMatch = RegExp(r'^\s*\d+[.)]\s+').firstMatch(lines[i]);
        if (itemMatch == null) {
          // 空行：窥看下一非空行是否还是列表项，是则跳过空行续上
          if (lines[i].trim().isEmpty) {
            var next = i + 1;
            while (next < lines.length && lines[next].trim().isEmpty) {
              next++;
            }
            if (next < lines.length &&
                RegExp(r'^\s*\d+[.)]\s+').hasMatch(lines[next])) {
              i = next;
              continue;
            }
          }
          break;
        }
        buf.add(lines[i].replaceFirst(RegExp(r'^\s*\d+[.)]\s+'), ''));
        i++;
      }
      blocks.add(_List(items: buf, ordered: true));
      continue;
    }

    // 表格（GFM）：表头行 + 分隔行（| --- |）才认定为表格，
    // 单独一行竖线文本按普通段落降级。
    if (_isTableStart(lines, i)) {
      final header = _splitTableRow(lines[i]);
      final aligns = [
        for (final cell in _splitTableRow(lines[i + 1])) _tableAlignOf(cell),
      ];
      i += 2;
      final rows = <List<String>>[];
      while (i < lines.length && lines[i].trimLeft().startsWith('|')) {
        rows.add(_splitTableRow(lines[i]));
        i++;
      }
      blocks.add(_TableBlock(header: header, aligns: aligns, rows: rows));
      continue;
    }

    // 空行：段落分隔
    if (line.trim().isEmpty) {
      i++;
      continue;
    }

    // 普通段落（合并连续非空行；AI 常把块级元素直接贴在引导句后面，
    // 此处遇到任何块级起始都要提前收手，交给下轮解析）
    final buf = <String>[];
    while (i < lines.length && lines[i].trim().isNotEmpty) {
      if (buf.isNotEmpty && _isBlockStart(lines, i)) break;
      buf.add(lines[i]);
      i++;
    }
    blocks.add(_Paragraph(text: buf.join(' ')));
  }

  return blocks;
}

/// 块级起始判定：段落合并遇到这些行要收手。与 _parseBlocks 主循环的
/// 识别集合保持一致（代码块/分隔线/标题/引用/列表/表格）。
bool _isBlockStart(List<String> lines, int i) {
  final line = lines[i];
  if (line.trimLeft().startsWith('```')) return true;
  if (RegExp(r'^\s*([-*_])\s*(\1\s*){2,}$').hasMatch(line)) return true;
  if (RegExp(r'^#{1,6}\s+').hasMatch(line)) return true;
  if (line.trimLeft().startsWith('>')) return true;
  if (RegExp(r'^\s*[-*]\s+').hasMatch(line)) return true;
  if (RegExp(r'^\s*\d+[.)]\s+').hasMatch(line)) return true;
  return _isTableStart(lines, i);
}

/// GFM 表格起始判定：当前行以 `|` 开头，且下一行是合法分隔行
/// （每个单元格都是 `:?-+:?` 形态）。
bool _isTableStart(List<String> lines, int i) {
  if (i + 1 >= lines.length) return false;
  if (!lines[i].trimLeft().startsWith('|')) return false;
  if (!lines[i + 1].trimLeft().startsWith('|')) return false;
  final cells = _splitTableRow(lines[i + 1]);
  if (cells.isEmpty) return false;
  return cells.every((c) => RegExp(r'^:?-+:?$').hasMatch(c));
}

/// 切分一行表格文本为单元格：去掉首尾的 `|`，按未转义的 `|` 切分，
/// `\|` 还原为字面竖线。
List<String> _splitTableRow(String line) {
  var s = line.trim();
  if (s.startsWith('|')) s = s.substring(1);
  if (s.endsWith('|') && !s.endsWith(r'\|')) {
    s = s.substring(0, s.length - 1);
  }
  final cells = <String>[];
  final buf = StringBuffer();
  for (var idx = 0; idx < s.length; idx++) {
    final ch = s[idx];
    if (ch == r'\' && idx + 1 < s.length && s[idx + 1] == '|') {
      buf.write('|');
      idx++;
      continue;
    }
    if (ch == '|') {
      cells.add(buf.toString().trim());
      buf.clear();
      continue;
    }
    buf.write(ch);
  }
  cells.add(buf.toString().trim());
  return cells;
}

/// 分隔行单元格 → 列对齐：`:--:` 居中、`--:` 右对齐，其余默认左对齐。
TextAlign? _tableAlignOf(String cell) {
  final c = cell.trim();
  if (c.startsWith(':') && c.endsWith(':')) return TextAlign.center;
  if (c.endsWith(':')) return TextAlign.right;
  return null;
}

// ---- 内部块模型 ----

sealed class _Block {}

class _Heading extends _Block {
  final int level;
  final String text;
  _Heading({required this.level, required this.text});
}

class _Paragraph extends _Block {
  final String text;
  _Paragraph({required this.text});
}

class _List extends _Block {
  final List<String> items;
  final bool ordered;
  _List({required this.items, required this.ordered});
}

class _Quote extends _Block {
  final String text;
  _Quote({required this.text});
}

class _CodeBlock extends _Block {
  final String code;
  _CodeBlock({required this.code});
}

class _Rule extends _Block {}

class _TableBlock extends _Block {
  final List<String> header;
  final List<TextAlign?> aligns;
  final List<List<String>> rows;
  _TableBlock({required this.header, required this.aligns, required this.rows});
}
