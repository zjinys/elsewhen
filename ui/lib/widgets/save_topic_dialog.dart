import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';

/// 「存为主题」对话框：复用知识库「文本导入」的字段与交互（标题 / 标签 / 正文），
/// 但保存为 topic 页。点消息气泡上的书签按钮弹出，预填从消息提取的内容，
/// 用户可改后点「保存到主题库」入库——这个确认框本身就是把关，不绕开用户。
///
/// 内容提取规则（规则提取，不调 AI，即时、离线可用）：
/// * 消息若含 AI 「提交草稿」约定结构（`**标题**：…`、`**正文**：…`），精准抽出
///   标题与正文，去掉「你说得对…」「（提交中……）」这类对话废话；
/// * 普通消息没有该结构 → 标题留空（保存时取正文首行前 60 字）、正文取整条原文，
///   由用户在对话框里自行整理。
class SaveTopicDialog extends ConsumerStatefulWidget {
  /// 消息原文（用于提取）。
  final String messageContent;

  const SaveTopicDialog({super.key, required this.messageContent});

  /// 弹出对话框；保存成功后返回 true，否则 null/false。
  static Future<bool?> show(BuildContext context, String messageContent) {
    return showDialog<bool>(
      context: context,
      builder: (_) => SaveTopicDialog(messageContent: messageContent),
    );
  }

  @override
  ConsumerState<SaveTopicDialog> createState() => _SaveTopicDialogState();
}

class _SaveTopicDialogState extends ConsumerState<SaveTopicDialog> {
  late final TextEditingController _titleController;
  late final TextEditingController _textController;
  final _tagsController = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    final extracted = _extractTopicContent(widget.messageContent);
    _titleController = TextEditingController(text: extracted.title ?? '');
    _textController = TextEditingController(text: extracted.body);
  }

  @override
  void dispose() {
    _titleController.dispose();
    _textController.dispose();
    _tagsController.dispose();
    super.dispose();
  }

  List<String> _parsedTags() => _tagsController.text
      .split(RegExp(r'[\s,，、]+'))
      .map((t) => t.trim().replaceAll(RegExp(r'^#+'), ''))
      .where((t) => t.isNotEmpty)
      .toList();

  Future<void> _save() async {
    final text = _textController.text.trim();
    if (text.isEmpty) {
      setState(() => _error = '正文不能为空');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final title = _titleController.text.trim();
      final page = await repo.saveTopicPage(
        text: text,
        title: title.isEmpty ? null : title,
        tags: _parsedTags(),
      );
      if (!mounted) return;
      ref.invalidate(wikiPagesProvider);
      openWikiPageTab(ref, page);
      Navigator.of(context).pop(true);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('已存为主题：《${page.title}》'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppTheme.surface0,
      title: Text(
        '存为主题',
        style: TextStyle(
          fontSize: 16,
          fontWeight: FontWeight.w600,
          color: AppTheme.textPrimary,
        ),
      ),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              TextField(
                controller: _titleController,
                enabled: !_busy,
                decoration: _deco('标题（可选，留空自动取正文首行前 60 字）'),
              ),
              const SizedBox(height: AppTheme.space3),
              TextField(
                controller: _tagsController,
                enabled: !_busy,
                style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary),
                decoration: _deco(
                  '标签（可选，空格或逗号分隔，例如：理财 副业）',
                  prefix: Icon(Icons.sell_outlined, size: 16),
                ),
              ),
              const SizedBox(height: AppTheme.space3),
              SizedBox(
                height: 300,
                child: TextField(
                  controller: _textController,
                  enabled: !_busy,
                  maxLines: null,
                  expands: true,
                  textAlignVertical: TextAlignVertical.top,
                  style: TextStyle(color: AppTheme.textPrimary),
                  decoration: _deco('正文（Markdown 支持）'),
                ),
              ),
              if (_error != null) ...[
                const SizedBox(height: AppTheme.space2),
                Text(
                  _error!,
                  style: TextStyle(fontSize: 12, color: AppTheme.error),
                ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(false),
          child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
        ),
        FilledButton.icon(
          onPressed: _busy ? null : _save,
          icon: _busy
              ? const SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.bookmark_add_outlined, size: 16),
          label: Text(_busy ? '保存中…' : '保存到主题库'),
          style: FilledButton.styleFrom(
            backgroundColor: AppTheme.accentPrimary,
          ),
        ),
      ],
    );
  }

  InputDecoration _deco(String hint, {Widget? prefix}) {
    return InputDecoration(
      hintText: hint,
      hintStyle: TextStyle(fontSize: 13, color: AppTheme.textTertiary),
      prefixIcon: prefix,
      prefixIconConstraints: prefix != null
          ? const BoxConstraints(minWidth: 36, minHeight: 36)
          : null,
      isDense: true,
      contentPadding: const EdgeInsets.all(12),
      filled: true,
      fillColor: AppTheme.surface2,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        borderSide: BorderSide(color: AppTheme.surface3),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        borderSide: BorderSide(color: AppTheme.surface3),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        borderSide: BorderSide(color: AppTheme.accentPrimary, width: 1.5),
      ),
    );
  }
}

/// 提取结果。
class _Extracted {
  final String? title; // 提取不到则 null（标题留空）
  final String body;
  const _Extracted({this.title, required this.body});
}

/// 从 AI 消息提取「标题 + 正文」。
///
/// 支持 AI 提交草稿的约定结构（markdown 加粗或纯文本标记均可）：
///   **标题**：卖古籍水浒（冲短期目标）
///   **类型**：topic
///   **正文**：
///   > ## 是什么 …
/// 命中则精准抽出标题与正文（正文去掉引用前缀 `> `、去掉对话前后缀废话）；
/// 不命中则返回整条原文、标题留空。
_Extracted _extractTopicContent(String content) {
  // 标题行：可选 ** 包裹的「标题」二字，后接 ：或 :，取到行尾。
  final titleRe = RegExp(
    r'^\*{0,2}标题\*{0,2}\s*[:：]\s*(.+?)\s*$',
    multiLine: true,
  );
  final titleMatch = titleRe.firstMatch(content);

  // 正文标记：可选 ** 包裹的「正文」二字，后接 ：或 :。
  final bodyMarkRe = RegExp(r'\*{0,2}正文\*{0,2}\s*[:：]');
  final bodyMatch = bodyMarkRe.firstMatch(content);

  // 有「正文」标记 → 取其后的内容为正文。
  if (bodyMatch != null) {
    var body = content.substring(bodyMatch.end).trim();
    // 去掉结尾常见的「（提交中……）」「（提交中...）」等占位。
    body = body.replaceAll(RegExp(r'[（(]\s*提交中[…\.。]*\s*[)）]\s*$'), '').trim();
    // 去掉每行行首的引用前缀「> 」（AI 常用 blockquote 包正文）。
    body = body
        .split('\n')
        .map((l) => l.replaceFirst(RegExp(r'^\s*>\s?'), ''))
        .join('\n')
        .trim();
    final title = titleMatch?.group(1)?.trim();
    return _Extracted(
      title: (title == null || title.isEmpty) ? null : title,
      body: body,
    );
  }

  // 无「正文」标记但有「标题」→ 标题取标记，正文取整条（去掉标题/类型标记行）。
  if (titleMatch != null) {
    final title = titleMatch.group(1)?.trim();
    final body = content
        .split('\n')
        .where((l) => !RegExp(r'^\s*\*{0,2}(标题|类型)\*{0,2}\s*[:：]').hasMatch(l))
        .join('\n')
        .trim();
    return _Extracted(
      title: (title == null || title.isEmpty) ? null : title,
      body: body.isEmpty ? content.trim() : body,
    );
  }

  // 普通消息：标题留空、正文取整条原文。
  return _Extracted(body: content.trim());
}
