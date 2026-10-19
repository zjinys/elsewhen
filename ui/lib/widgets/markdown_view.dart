import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:url_launcher/url_launcher.dart';

import '../theme/app_theme.dart';
import '../theme/content_font.dart';

part 'markdown_view_parts/markdown_parse.dart';
part 'markdown_view_parts/markdown_blocks.dart';
// ── markdown 渲染按解析/块渲染拆分（State 留主类，零行为变化）──

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
