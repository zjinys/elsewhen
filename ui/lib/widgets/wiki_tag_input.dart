//! 知识库标签输入框：在「手打自由文本」之上叠一层「已有标签可点选 / 前缀补齐」。
//!
//! 背景：标签是自由文本，全库已有 180 个不同标签（`编译知识` / `闲鱼` / `小红书`
//! / `lens-3` …）。纯 TextField 要求用户记住拼法，实际结果是同一个概念被写成
//! 多个变体，标签退化成不可聚合的噪声。
//!
//! 设计取舍：
//! - 输入控制器仍是一整串 `空格/逗号` 分隔的文本，提交方
//!   （创建主题 / 导入保存 / 编辑标签）的 `_parsedTags()` 一行都不用改；
//!   规范化 tag 词表由 Rust/SQLite 独立维护。
//! - **补全只碰「光标所在的这一个 token」**。前缀匹配、替换，前后文原样保留，
//!   所以「工作 Rust」里补全 `Rust` 不会动 `工作`。
//! - **点空白收起建议**，不抢焦点：候选面板是输入框下方的 `Material` 浮层，
//!   键盘仍留在输入框里，选完可继续打下一个标签。

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';
import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';

/// 空格 / 逗号 / 顿号 —— 与各提交方 `_parsedTags()` 的分隔符保持一致。
final _tagSeparator = RegExp(r'[\s,，、]');

/// Canonical tag vocabulary. The page metadata fallback keeps older databases
/// and bridge test doubles usable while the normalized tag endpoint is absent.
final wikiTagsProvider = FutureProvider<List<String>>((ref) async {
  try {
    final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final tags = await bridge.listWikiTags();
    if (tags.isNotEmpty) return tags;
  } catch (_) {
    // Fall back to the compatibility page metadata below.
  }
  final pages = await ref.watch(wikiPagesProvider.future);
  return _sortPageTags(pages);
});

List<String> _sortPageTags(List<WikiPageMeta> pages) {
  final counts = <String, int>{};
  for (final page in pages) {
    for (final raw in page.tags) {
      final tag = raw.trim();
      if (tag.isEmpty) continue;
      counts[tag] = (counts[tag] ?? 0) + 1;
    }
  }
  final sorted = counts.keys.toList()
    ..sort((a, b) {
      final byCount = counts[b]!.compareTo(counts[a]!);
      // 频次相同时按字典序，保证候选顺序稳定（否则每次重建都会跳位）。
      return byCount != 0 ? byCount : a.compareTo(b);
    });
  return List.unmodifiable(sorted);
}

/// Synchronous view used by the input widget while the vocabulary loads.
final wikiTagVocabularyProvider = Provider<List<String>>((ref) {
  final canonical = ref.watch(wikiTagsProvider).value;
  if (canonical != null && canonical.isNotEmpty) return canonical;
  return _sortPageTags(ref.watch(wikiPagesProvider).value ?? const []);
});

/// 光标所在 token 的边界：向前 / 向后扫到最近的分隔符。
({int start, int end, String token}) _tokenAt(String text, int caret) {
  var start = caret;
  while (start > 0 && !_tagSeparator.hasMatch(text[start - 1])) {
    start--;
  }
  var end = caret;
  while (end < text.length && !_tagSeparator.hasMatch(text[end])) {
    end++;
  }
  return (start: start, end: end, token: text.substring(start, end));
}

/// 去掉 `#` 前缀后的 token —— 用户可能按习惯打 `#工作`，补全应照样命中。
String _normalizeToken(String token) =>
    token.replaceFirst(RegExp(r'^#+'), '').trim();

class WikiTagInput extends ConsumerStatefulWidget {
  const WikiTagInput({
    super.key,
    required this.controller,
    required this.hintText,
    this.enabled = true,
    this.style,
    this.prefixIcon,
    this.maxSuggestions = 8,
    this.onChanged,
  });

  final TextEditingController controller;
  final String hintText;
  final bool enabled;
  final TextStyle? style;
  final Widget? prefixIcon;

  /// 候选面板最多显示多少条。默认 8：手机上一个浮层超过 8 行就会盖到
  /// 提交按钮上。
  final int maxSuggestions;
  final ValueChanged<String>? onChanged;

  @override
  ConsumerState<WikiTagInput> createState() => _WikiTagInputState();
}

class _WikiTagInputState extends ConsumerState<WikiTagInput> {
  final _focusNode = FocusNode();
  final _scrollController = ScrollController();
  final _tapRegionGroup = Object();

  /// 当前候选对应的 token 起始位置；null = 面板收起。
  int? _suggestStart;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onTextChanged);
    _focusNode.addListener(_onFocusChanged);
  }

  @override
  void didUpdateWidget(covariant WikiTagInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_onTextChanged);
      widget.controller.addListener(_onTextChanged);
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onTextChanged);
    _focusNode.removeListener(_onFocusChanged);
    _focusNode.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _onFocusChanged() {
    if (!_focusNode.hasFocus) {
      setState(() => _suggestStart = null);
    } else {
      _recomputeSuggestions();
    }
  }

  void _onTextChanged() {
    widget.onChanged?.call(widget.controller.text);
    _recomputeSuggestions();
  }

  void _recomputeSuggestions() {
    if (!mounted) return;
    final value = widget.controller.value;
    final caret = value.selection.isValid ? value.selection.baseOffset : -1;
    if (!widget.enabled || !_focusNode.hasFocus || caret < 0) {
      if (_suggestStart != null) setState(() => _suggestStart = null);
      return;
    }
    // 空 token 也给候选：刚打完分隔符时最想「挑一个已有标签」，
    // 这时不该让用户凭记忆打第一个字。
    final next = _tokenAt(value.text, caret).start;
    // A prefix may change while its token starts at the same offset.
    setState(() => _suggestStart = next);
  }

  /// 当前会话的候选列表（空 list 表示没有可选项）。
  List<String> _visibleSuggestions() {
    final start = _suggestStart;
    if (start == null) return const [];
    final value = widget.controller.value;
    final caret = value.selection.isValid ? value.selection.baseOffset : -1;
    if (caret < 0) return const [];
    final token = _tokenAt(value.text, caret);
    final query = _normalizeToken(token.token).toLowerCase();
    final vocabulary = ref.watch(wikiTagVocabularyProvider);

    // 输入框里其他位置已经写了的标签不再重复推荐。
    final already = <String>{};
    for (final part in value.text.split(_tagSeparator)) {
      final t = _normalizeToken(part).toLowerCase();
      if (t.isNotEmpty) already.add(t);
    }

    final matches = <String>[];
    for (final tag in vocabulary) {
      final lower = tag.toLowerCase();
      if (lower == query) continue; // 这一处已经输完整了
      if (already.contains(lower)) continue; // 别处已经写过了
      if (query.isEmpty || lower.startsWith(query)) matches.add(tag);
      if (matches.length >= widget.maxSuggestions) break;
    }
    return matches;
  }

  /// 用 [tag] 替换光标所在的 token，并把光标放到替换后的末尾。
  void _accept(String tag) {
    final start = _suggestStart;
    if (start == null) return;
    final value = widget.controller.value;
    final caret = value.selection.isValid ? value.selection.baseOffset : -1;
    if (caret < 0) return;
    final token = _tokenAt(value.text, caret);

    final before = value.text.substring(0, token.start);
    final after = value.text.substring(token.end);
    // 保留用户原本打的 `#` 习惯（`#` 在存储时会被剥掉，这里只是别让它消失）。
    final hadHash = token.token.trimLeft().startsWith('#');
    final inserted = hadHash ? '#$tag' : tag;
    final next = '$before$inserted$after';
    final cursor = before.length + inserted.length;

    widget.controller.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: cursor),
    );
    setState(() => _suggestStart = null);
    // 选完把焦点还给输入框，方便接着打下一个标签。
    _focusNode.requestFocus();
  }

  /// 点击输入框以外的地方收起面板。
  void _collapse() {
    if (_suggestStart != null) setState(() => _suggestStart = null);
  }

  @override
  Widget build(BuildContext context) {
    // 依赖 vocabulary，保证新增标签后候选立即刷新。
    ref.watch(wikiTagVocabularyProvider);
    final suggestions = _visibleSuggestions();
    final showPanel = _suggestStart != null && suggestions.isNotEmpty;

    // The text field's built-in outside-tap handler shares this group. Both
    // mouse and touch can press/scroll suggestions without losing the token.
    return TextFieldTapRegion(
      groupId: _tapRegionGroup,
      onTapOutside: (_) => _collapse(),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            groupId: _tapRegionGroup,
            controller: widget.controller,
            focusNode: _focusNode,
            enabled: widget.enabled,
            textAlignVertical: TextAlignVertical.top,
            style: widget.style ?? AppTheme.fieldTextStyle,
            cursorColor: AppTheme.accentPrimary,
            decoration: AppTheme.fieldDecoration(
              hintText: widget.hintText,
              prefixIcon:
                  widget.prefixIcon ??
                  Icon(
                    Icons.sell_outlined,
                    size: DesignTokens.iconMicro,
                    color: AppTheme.textTertiary,
                  ),
            ),
            onTap: _recomputeSuggestions,
          ),
          if (showPanel) ...[
            const SizedBox(height: 4),
            _SuggestionPanel(
              suggestions: suggestions,
              scrollController: _scrollController,
              onAccept: _accept,
            ),
          ],
        ],
      ),
    );
  }
}

/// 候选浮层。用 [Material] + 描边而不是 `DropdownMenu`：
/// 候选要能跟随输入框宽度、随文本变化重排，且点击后不关闭键盘。
class _SuggestionPanel extends StatelessWidget {
  const _SuggestionPanel({
    required this.suggestions,
    required this.scrollController,
    required this.onAccept,
  });

  /// 行高与表头高显式固定，面板限高取「表头 + 整数行」——
  /// 此前用 168 硬限高，行高 31.5 不整除，最后一行被从中间剖开只露 9px，
  /// 看着像渲染错位。要么整行在、要么整行不在。
  static const double _rowHeight = 32;
  static const double _headerHeight = 24;
  static const int _visibleRows = 5;

  final List<String> suggestions;
  final ScrollController scrollController;
  final ValueChanged<String> onAccept;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      child: Container(
        constraints: const BoxConstraints(
          maxHeight: _headerHeight + _visibleRows * _rowHeight,
        ),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(
              height: _headerHeight,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(10, 5, 10, 0),
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    context.l10n.wikiTagsExisting,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
              ),
            ),
            Flexible(
              child: ListView.builder(
                controller: scrollController,
                shrinkWrap: true,
                padding: EdgeInsets.zero,
                itemExtent: _rowHeight,
                itemCount: suggestions.length,
                itemBuilder: (context, index) {
                  final tag = suggestions[index];
                  return InkWell(
                    onTap: () => onAccept(tag),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                      child: Row(
                        children: [
                          Icon(
                            Icons.sell_outlined,
                            size: DesignTokens.iconMicro,
                            color: AppTheme.textTertiary,
                          ),
                          const SizedBox(width: 6),
                          Expanded(
                            child: Text(
                              tag,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 12,
                                color: AppTheme.textPrimary,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}
