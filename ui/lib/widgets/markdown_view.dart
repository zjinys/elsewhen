import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

/// 轻量 markdown 渲染器（面向 wiki 页正文，零依赖）
///
/// 支持：标题(#/##/###)、无序/有序列表、引用(>)、代码块(```)、
/// 行内粗体(**x**)、行内代码(`x`)、wikilink([[slug]])。
/// 未覆盖的语法按段落文本降级展示。
class MarkdownView extends StatelessWidget {
  final String markdown;
  final TextStyle? baseStyle;

  const MarkdownView({
    super.key,
    required this.markdown,
    this.baseStyle,
  });

  @override
  Widget build(BuildContext context) {
    final lines = markdown.split('\n');

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

      // 标题
      final headingMatch = RegExp(r'^(#{1,6})\s+(.*)$').firstMatch(line);
      if (headingMatch != null) {
        blocks.add(_Heading(
          level: headingMatch.group(1)!.length,
          text: headingMatch.group(2)!.trim(),
        ));
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

      // 无序列表
      if (RegExp(r'^\s*[-*]\s+').hasMatch(line)) {
        final buf = <String>[];
        while (i < lines.length && RegExp(r'^\s*[-*]\s+').hasMatch(lines[i])) {
          buf.add(lines[i].replaceFirst(RegExp(r'^\s*[-*]\s+'), ''));
          i++;
        }
        blocks.add(_List(items: buf, ordered: false));
        continue;
      }

      // 有序列表
      if (RegExp(r'^\s*\d+[.)]\s+').hasMatch(line)) {
        final buf = <String>[];
        while (i < lines.length && RegExp(r'^\s*\d+[.)]\s+').hasMatch(lines[i])) {
          buf.add(lines[i].replaceFirst(RegExp(r'^\s*\d+[.)]\s+'), ''));
          i++;
        }
        blocks.add(_List(items: buf, ordered: true));
        continue;
      }

      // 空行：段落分隔
      if (line.trim().isEmpty) {
        i++;
        continue;
      }

      // 普通段落（合并连续非空行）
      final buf = <String>[];
      while (i < lines.length && lines[i].trim().isNotEmpty) {
        buf.add(lines[i]);
        i++;
      }
      blocks.add(_Paragraph(text: buf.join(' ')));
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final block in blocks) _BlockWidget(block: block),
      ],
    );
  }
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

class _BlockWidget extends StatelessWidget {
  final _Block block;

  const _BlockWidget({required this.block});

  @override
  Widget build(BuildContext context) {
    return switch (block) {
      _Heading(:final level, :final text) => Padding(
        padding: const EdgeInsets.only(top: AppTheme.space4, bottom: AppTheme.space2),
        child: Text(
          text,
          style: TextStyle(
            fontSize: level == 1 ? 22 : level == 2 ? 18 : 16,
            fontWeight: FontWeight.w700,
            color: AppTheme.textPrimary,
            height: 1.4,
          ),
        ),
      ),
      _Paragraph(:final text) => Padding(
        padding: const EdgeInsets.only(bottom: AppTheme.space3),
        child: RichText(
          text: _inlineSpans(text, TextStyle(
            fontSize: 14,
            color: AppTheme.textSecondary,
            height: 1.7,
          )),
        ),
      ),
      _List(:final items, :final ordered) => Padding(
        padding: const EdgeInsets.only(bottom: AppTheme.space3),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (var idx = 0; idx < items.length; idx++)
              Padding(
                padding: const EdgeInsets.only(bottom: AppTheme.space1),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(
                      width: 20,
                      child: Text(
                        ordered ? '${idx + 1}.' : '•',
                        style: TextStyle(
                          fontSize: 14,
                          color: AppTheme.accentPrimary,
                          height: 1.7,
                        ),
                      ),
                    ),
                    Expanded(
                      child: RichText(
                        text: _inlineSpans(items[idx], TextStyle(
                          fontSize: 14,
                          color: AppTheme.textSecondary,
                          height: 1.7,
                        )),
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
      _Quote(:final text) => Container(
        margin: const EdgeInsets.only(bottom: AppTheme.space3),
        padding: const EdgeInsets.symmetric(
          horizontal: AppTheme.space3,
          vertical: AppTheme.space2,
        ),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          border: Border(
            left: BorderSide(
              color: AppTheme.accentMuted,
              width: 3,
            ),
          ),
        ),
        child: RichText(
          text: _inlineSpans(text, TextStyle(
            fontSize: 13,
            color: AppTheme.textSecondary,
            height: 1.6,
          )),
        ),
      ),
      _CodeBlock(:final code) => Container(
        width: double.infinity,
        margin: const EdgeInsets.only(bottom: AppTheme.space3),
        padding: const EdgeInsets.all(AppTheme.space3),
        decoration: BoxDecoration(
          color: AppTheme.surface2.withValues(alpha: 0.7),
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        ),
        child: SelectableText(
          code,
          style: TextStyle(
            fontSize: 12,
            color: AppTheme.textPrimary,
            fontFamily: 'monospace',
            height: 1.5,
          ),
        ),
      ),
    };
  }

  /// 行内解析：**粗体**、`代码`、[[wikilink]]
  TextSpan _inlineSpans(String text, TextStyle base) {
    const pattern = r'(\*\*.+?\*\*|`[^`]+`|\[\[[^\]]+\]\])';
    final spans = <TextSpan>[];
    final re = RegExp(pattern);
    var last = 0;

    for (final m in re.allMatches(text)) {
      if (m.start > last) {
        spans.add(TextSpan(text: text.substring(last, m.start)));
      }
      final raw = m.group(0)!;
      if (raw.startsWith('**') && raw.endsWith('**')) {
        spans.add(TextSpan(
          text: raw.substring(2, raw.length - 2),
          style: base.copyWith(fontWeight: FontWeight.w700),
        ));
      } else if (raw.startsWith('`')) {
        spans.add(TextSpan(
          text: raw.substring(1, raw.length - 1),
          style: base.copyWith(
            fontFamily: 'monospace',
            color: AppTheme.accentPrimary,
          ),
        ));
      } else if (raw.startsWith('[[')) {
        spans.add(TextSpan(
          text: raw.substring(2, raw.length - 2),
          style: base.copyWith(
            color: AppTheme.accentPrimary,
            decoration: TextDecoration.underline,
            fontWeight: FontWeight.w600,
          ),
        ));
      }
      last = m.end;
    }
    if (last < text.length) {
      spans.add(TextSpan(text: text.substring(last)));
    }
    return TextSpan(style: base, children: spans);
  }
}