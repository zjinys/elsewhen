import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:url_launcher/url_launcher.dart';

import '../theme/app_theme.dart';
import '../theme/content_font.dart';

/// 轻量 markdown 渲染器（面向 wiki 页正文，零依赖）
///
/// 支持：标题(#/##/###)、无序/有序列表（含 [ ]/[x] 复选）、引用(>)、
/// 代码块(```)、分隔线(---)、GFM 表格(|...| 且需分隔行)、行内粗体/斜体
/// (**x** / *x*)、删除线(~~x~~)、行内代码(`x`)、wikilink([[slug]])、
/// markdown 链接([文字](url))。
/// 未覆盖的语法按段落文本降级展示。
///
/// 排版取向：正文用高对比正文色 + 宽松行高，标题分级带留白，尽量接近
/// 「可长时间阅读」的观感，而不是日志式的堆叠。
/// UI 边界兜底：工具协议残留绝不渲染（原则2在界面层的最终 enforcement）。
///
/// 即使 Rust 侧某次没拦住（历史脏数据/未来新方言），界面也不显示协议文本。
/// 只处理非代码围栏区——用户在代码块里讨论协议文本是合法内容，原样保留。
String stripToolProtocolForDisplay(String markdown) {
  final segments = <_DisplaySegment>[];
  final buf = <String>[];
  var inFence = false;
  void flush() {
    if (buf.isEmpty) return;
    segments.add(_DisplaySegment(text: buf.join('\n'), code: inFence));
    buf.clear();
  }

  for (final line in markdown.split('\n')) {
    if (line.trimLeft().startsWith('```')) {
      flush();
      inFence = !inFence;
      segments.add(_DisplaySegment(text: line, code: true));
      continue;
    }
    buf.add(line);
  }
  flush();
  return segments
      .map((s) => s.code ? s.text : _stripProtocolFromProse(s.text))
      .join('\n');
}

class _DisplaySegment {
  final String text;
  final bool code;
  const _DisplaySegment({required this.text, required this.code});
}

/// 去掉正文段里的整段 `<tool_call>…</tool_call>` 与协议残留行。
String _stripProtocolFromProse(String prose) {
  var cur = prose.replaceAll(
    RegExp(r'<tool_call>.*?</tool_call>', dotAll: true),
    '',
  );
  const markers = [
    '<tool_call',
    '</tool_call>',
    '<arg_key',
    '<arg_value',
    '[工具调用]',
    'invoke name=',
    '<|invoke',
    'parameter name=',
  ];
  final kept = <String>[];
  for (final line in cur.split('\n')) {
    if (markers.any(line.contains)) continue;
    kept.add(line);
  }
  return kept.join('\n').trim();
}

class MarkdownView extends StatefulWidget {
  final String markdown;
  final TextStyle? baseStyle;

  const MarkdownView({super.key, required this.markdown, this.baseStyle});

  @override
  State<MarkdownView> createState() => _MarkdownViewState();
}

/// 分块结果按正文缓存（P12）：父级任意一次重建都会把同一份正文带进来，
/// 而「清洗 + 逐行分块」全是正则与字符串拼接的纯 CPU 活。只在正文真正变化
/// 时重算，命中则复用上一次的分块——block 只承载文本与结构、样式在 build
/// 期才叠加，所以按正文做键是安全的。
class _MarkdownViewState extends State<MarkdownView> {
  String? _parsedSource;
  List<_Block>? _parsedBlocks;

  List<_Block> _blocksOf(String markdown) {
    final cached = _parsedBlocks;
    if (cached != null && _parsedSource == markdown) return cached;
    final parsed = _parseBlocks(markdown);
    _parsedBlocks = parsed;
    _parsedSource = markdown;
    return parsed;
  }

  @override
  Widget build(BuildContext context) {
    // UI 边界清洗：协议残留到此为止，不进渲染。
    final baseStyle =
        widget.baseStyle ??
        TextStyle(color: AppTheme.textPrimary, fontSize: 15, height: 1.8);
    // Markdown 渲染的都是内容（AI 回复 / 知识库 / 草稿预览）→ 内容字体。
    // 块内多用 RichText（不继承 DefaultTextStyle），需并进 base 显式传递。
    final contentFont = ContentFont.styleOf(context);
    final base = contentFont == null ? baseStyle : baseStyle.merge(contentFont);
    final blocks = _blocksOf(widget.markdown);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final block in blocks) _BlockWidget(block: block, base: base),
      ],
    );
  }
}

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

class _BlockWidget extends StatefulWidget {
  final _Block block;
  final TextStyle base;

  const _BlockWidget({required this.block, required this.base});

  @override
  State<_BlockWidget> createState() => _BlockWidgetState();
}

/// 链接 span 的 [TapGestureRecognizer] 会把手势竞技场注册留在这份列表里，
/// 必须随 element 一起释放。此前识别器在解析函数里就地 new、从不 dispose，
/// 每一次重建都往竞技场多挂一份，长文页尤甚（P12）。
class _BlockWidgetState extends State<_BlockWidget> {
  final List<TapGestureRecognizer> _recognizers = [];

  /// 供下方解析方法取样式：block 的样式在每次 build 由父级重新下发。
  TextStyle get base => widget.base;

  void _releaseRecognizers() {
    if (_recognizers.isEmpty) return;
    for (final recognizer in _recognizers) {
      recognizer.dispose();
    }
    _recognizers.clear();
  }

  /// 链接用：登记到本 element 的识别器表，dispose 时统一释放。
  TapGestureRecognizer _linkRecognizer(String url) {
    final recognizer = TapGestureRecognizer()
      ..onTap = () => launchUrl(Uri.parse(url));
    _recognizers.add(recognizer);
    return recognizer;
  }

  @override
  void dispose() {
    _releaseRecognizers();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // 上一轮 build 产出的 span 已随本次更新整体失效，此刻回收其识别器，
    // 避免长驻 element 上按重建次数累积。
    _releaseRecognizers();
    final block = widget.block;
    return switch (block) {
      _TableBlock(:final header, :final aligns, :final rows) => _buildTable(
        header,
        aligns,
        rows,
      ),
      _Rule() => Padding(
        padding: const EdgeInsets.symmetric(vertical: AppTheme.space4),
        child: Container(height: 1, color: AppTheme.surface3),
      ),
      _Heading(:final level, :final text) => _buildHeading(level, text),
      _Paragraph(:final text) => Padding(
        padding: const EdgeInsets.only(bottom: AppTheme.space3),
        child: RichText(text: _inlineSpans(text, base)),
      ),
      _List(:final items, :final ordered) => Padding(
        padding: const EdgeInsets.only(bottom: AppTheme.space3),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (var idx = 0; idx < items.length; idx++)
              _buildListItem(items[idx], idx, ordered),
          ],
        ),
      ),
      _Quote(:final text) => Container(
        margin: const EdgeInsets.only(bottom: AppTheme.space3),
        padding: const EdgeInsets.fromLTRB(
          AppTheme.space3,
          AppTheme.space2,
          AppTheme.space3,
          AppTheme.space2,
        ),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          border: Border(
            left: BorderSide(color: AppTheme.accentPrimary, width: 3),
          ),
        ),
        child: RichText(
          text: _inlineSpans(
            text,
            base.copyWith(
              fontSize: 14,
              color: AppTheme.textSecondary,
              height: 1.7,
            ),
          ),
        ),
      ),
      _CodeBlock(:final code) => Container(
        width: double.infinity,
        margin: const EdgeInsets.only(bottom: AppTheme.space3),
        padding: const EdgeInsets.all(AppTheme.space3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: SelectableText(
            code,
            style: TextStyle(
              fontSize: 12.5,
              color: AppTheme.textPrimary,
              fontFamily: 'monospace',
              height: 1.6,
            ),
          ),
        ),
      ),
    };
  }

  Widget _buildHeading(int level, String text) {
    final size = switch (level) {
      1 => 24.0,
      2 => 19.0,
      3 => 16.5,
      4 => 15.0,
      5 => 14.0,
      _ => 13.5,
    };
    final top = switch (level) {
      1 => AppTheme.space6,
      2 => AppTheme.space6,
      3 => AppTheme.space4,
      _ => AppTheme.space3,
    };
    final bottom = level <= 2 ? AppTheme.space2 : AppTheme.space1;
    final widget = Padding(
      padding: EdgeInsets.only(top: top, bottom: bottom),
      child: Text(
        text,
        style: TextStyle(
          fontSize: size,
          fontWeight: level <= 3 ? FontWeight.w700 : FontWeight.w600,
          color: AppTheme.textPrimary,
          height: 1.35,
          letterSpacing: level == 1 ? -0.2 : 0,
        ),
      ),
    );
    // 一二级标题加一条细分隔线，帮助扫读时定位层级
    if (level <= 2) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          widget,
          Container(height: 1, color: AppTheme.surface3),
        ],
      );
    }
    return widget;
  }

  /// 表格：IntrinsicColumnWidth 按内容取列宽，超宽交给横向滚动
  /// （与代码块同一溢出策略）；单列内容上限 360，防止长句把整表撑爆。
  /// 列数不对齐的数据行按表头列数截断/补空，绝不越界。
  Widget _buildTable(
    List<String> header,
    List<TextAlign?> aligns,
    List<List<String>> rows,
  ) {
    final cols = header.length;
    final cellStyle = base.copyWith(fontSize: (base.fontSize ?? 15) - 1);
    final headerStyle = cellStyle.copyWith(fontWeight: FontWeight.w600);

    TextAlign? alignOf(int idx) => idx < aligns.length ? aligns[idx] : null;

    TableRow rowOf(List<String> cells, {required bool isHeader}) => TableRow(
      decoration: isHeader ? BoxDecoration(color: AppTheme.surface2) : null,
      children: [
        for (var idx = 0; idx < cols; idx++)
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: 10,
              vertical: AppTheme.space2,
            ),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 360),
              child: RichText(
                textAlign: alignOf(idx) ?? TextAlign.left,
                text: _inlineSpans(
                  idx < cells.length ? cells[idx] : '',
                  isHeader ? headerStyle : cellStyle,
                ),
              ),
            ),
          ),
      ],
    );

    return Container(
      margin: const EdgeInsets.only(bottom: AppTheme.space3),
      decoration: BoxDecoration(
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      ),
      clipBehavior: Clip.antiAlias,
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Table(
          defaultColumnWidth: const IntrinsicColumnWidth(),
          border: TableBorder(
            horizontalInside: BorderSide(color: AppTheme.surface3),
            verticalInside: BorderSide(color: AppTheme.surface3),
          ),
          children: [
            rowOf(header, isHeader: true),
            for (final row in rows) rowOf(row, isHeader: false),
          ],
        ),
      ),
    );
  }

  Widget _buildListItem(String item, int idx, bool ordered) {
    // 复选列表：- [ ] / - [x]
    final check = RegExp(r'^\[([ xX])\]\s+(.*)$').firstMatch(item.trim());
    final isCheckbox = check != null;
    final text = isCheckbox ? check.group(2)! : item;
    final checked = isCheckbox && check.group(1)!.toLowerCase() == 'x';

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space1),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 22,
            child: isCheckbox
                ? Padding(
                    padding: const EdgeInsets.only(top: 3),
                    child: Icon(
                      checked
                          ? Icons.check_box_outlined
                          : Icons.check_box_outline_blank,
                      size: 16,
                      color: checked
                          ? AppTheme.accentPrimary
                          : AppTheme.textTertiary,
                    ),
                  )
                : Padding(
                    padding: const EdgeInsets.only(top: 1),
                    child: Text(
                      ordered ? '${idx + 1}.' : '•',
                      style: TextStyle(
                        fontSize: base.fontSize,
                        color: AppTheme.accentPrimary,
                        height: base.height,
                      ),
                    ),
                  ),
          ),
          Expanded(
            child: RichText(
              text: _inlineSpans(
                text,
                base.copyWith(
                  color: checked ? AppTheme.textTertiary : base.color,
                  decoration: checked ? TextDecoration.lineThrough : null,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// 行内解析：~~删除线~~、**粗体**、*斜体*、`代码`、[[wikilink]]、[文字](url)
  TextSpan _inlineSpans(String text, TextStyle base) {
    const pattern =
        r'(~~.+?~~|\*\*.+?\*\*|`[^`]+`|\[\[[^\]]+\]\]|\[[^\]]+\]\([^)]+\)|\*[^*]+\*)';
    final spans = <TextSpan>[];
    final re = RegExp(pattern);
    var last = 0;

    for (final m in re.allMatches(text)) {
      if (m.start > last) {
        spans.add(TextSpan(text: text.substring(last, m.start)));
      }
      final raw = m.group(0)!;
      if (raw.startsWith('~~') && raw.endsWith('~~')) {
        spans.add(
          TextSpan(
            text: raw.substring(2, raw.length - 2),
            style: base.copyWith(
              decoration: TextDecoration.lineThrough,
              color: AppTheme.textSecondary,
            ),
          ),
        );
      } else if (raw.startsWith('**') && raw.endsWith('**')) {
        spans.add(
          TextSpan(
            text: raw.substring(2, raw.length - 2),
            style: base.copyWith(fontWeight: FontWeight.w700),
          ),
        );
      } else if (raw.startsWith('`')) {
        spans.add(
          TextSpan(
            text: raw.substring(1, raw.length - 1),
            style: base.copyWith(
              fontFamily: 'monospace',
              fontSize: (base.fontSize ?? 15) - 1,
              color: AppTheme.accentPrimary,
              backgroundColor: AppTheme.surface3,
            ),
          ),
        );
      } else if (raw.startsWith('[[')) {
        spans.add(
          TextSpan(
            text: raw.substring(2, raw.length - 2),
            style: base.copyWith(
              color: AppTheme.accentPrimary,
              decoration: TextDecoration.underline,
              decorationColor: AppTheme.accentMuted,
              fontWeight: FontWeight.w600,
            ),
          ),
        );
      } else if (raw.startsWith('[')) {
        // [文字](url) → 只展示文字
        final label = raw.substring(1, raw.indexOf(']'));
        final url = raw.substring(raw.indexOf('](') + 2, raw.length - 1);
        spans.add(
          TextSpan(
            text: label,
            style: base.copyWith(
              color: AppTheme.accentPrimary,
              decoration: TextDecoration.underline,
              decorationColor: AppTheme.accentMuted,
            ),
            recognizer: _linkRecognizer(url),
          ),
        );
      } else if (raw.startsWith('*') && raw.endsWith('*')) {
        spans.add(
          TextSpan(
            text: raw.substring(1, raw.length - 1),
            style: base.copyWith(fontStyle: FontStyle.italic),
          ),
        );
      }
      last = m.end;
    }
    if (last < text.length) {
      spans.add(TextSpan(text: text.substring(last)));
    }
    return TextSpan(style: base, children: spans);
  }
}
