import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

/// 轻量 markdown 渲染器（面向 wiki 页正文，零依赖）
///
/// 支持：标题(#/##/###)、无序/有序列表（含 [ ]/[x] 复选）、引用(>)、
/// 代码块(```)、分隔线(---)、行内粗体/斜体(**x** / *x*)、行内代码(`x`)、
/// wikilink([[slug]])、markdown 链接([文字](url))。
/// 未覆盖的语法按段落文本降级展示。
///
/// 排版取向：正文用高对比正文色 + 宽松行高，标题分级带留白，尽量接近
/// 「可长时间阅读」的观感，而不是日志式的堆叠。
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
    final base = baseStyle ??
        TextStyle(color: AppTheme.textPrimary, fontSize: 15, height: 1.8);

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

      // 分隔线
      if (RegExp(r'^\s*([-*_])\s*(\1\s*){2,}$').hasMatch(line)) {
        blocks.add(_Rule());
        i++;
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
        for (final block in blocks) _BlockWidget(block: block, base: base),
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

class _Rule extends _Block {}

class _BlockWidget extends StatelessWidget {
  final _Block block;
  final TextStyle base;

  const _BlockWidget({required this.block, required this.base});

  @override
  Widget build(BuildContext context) {
    return switch (block) {
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

  /// 行内解析：**粗体**、*斜体*、`代码`、[[wikilink]]、[文字](url)
  TextSpan _inlineSpans(String text, TextStyle base) {
    const pattern =
        r'(\*\*.+?\*\*|`[^`]+`|\[\[[^\]]+\]\]|\[[^\]]+\]\([^)]+\)|\*[^*]+\*)';
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
            fontSize: (base.fontSize ?? 15) - 1,
            color: AppTheme.accentPrimary,
            backgroundColor: AppTheme.surface3,
          ),
        ));
      } else if (raw.startsWith('[[')) {
        spans.add(TextSpan(
          text: raw.substring(2, raw.length - 2),
          style: base.copyWith(
            color: AppTheme.accentPrimary,
            decoration: TextDecoration.underline,
            decorationColor: AppTheme.accentMuted,
            fontWeight: FontWeight.w600,
          ),
        ));
      } else if (raw.startsWith('[')) {
        // [文字](url) → 只展示文字
        final label = raw.substring(1, raw.indexOf(']'));
        spans.add(TextSpan(
          text: label,
          style: base.copyWith(
            color: AppTheme.accentPrimary,
            decoration: TextDecoration.underline,
            decorationColor: AppTheme.accentMuted,
          ),
        ));
      } else if (raw.startsWith('*') && raw.endsWith('*')) {
        spans.add(TextSpan(
          text: raw.substring(1, raw.length - 1),
          style: base.copyWith(fontStyle: FontStyle.italic),
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
