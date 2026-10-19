part of '../markdown_view.dart';

//! 块级渲染组件（_BlockWidget，图片竞技场兜底）

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
