import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import '../wiki/wiki_content_editor.dart';
import '../wiki/wiki_markdown_codec.dart';

/// 「创建主题」tab body：标题/标签/正文一步完成的知识页创建表单。
/// 保存成功后关闭本 tab 并打开新建的知识页。
class CreateTopicBody extends ConsumerStatefulWidget {
  /// 保存成功后的回调（关闭 tab 等）
  final VoidCallback? onSaved;

  const CreateTopicBody({super.key, this.onSaved});

  @override
  ConsumerState<CreateTopicBody> createState() => _CreateTopicBodyState();
}

class _CreateTopicBodyState extends ConsumerState<CreateTopicBody> {
  final _titleController = TextEditingController();
  final _tagsController = TextEditingController();
  final _editorKey = GlobalKey<WikiContentEditorState>();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _titleController.dispose();
    _tagsController.dispose();
    super.dispose();
  }

  List<String> _parsedTags() => _tagsController.text
      .split(RegExp(r'[\s,，、]+'))
      .map((t) => t.trim().replaceAll(RegExp(r'^#+'), ''))
      .where((t) => t.isNotEmpty)
      .toList();

  Future<void> _save() async {
    final editor = _editorKey.currentState;
    final text = editor == null
        ? ''
        : wikiDocumentToMarkdown(editor.editorState.document).trim();
    if (text.isEmpty) {
      setState(() => _error = '请先输入要保存的内容');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final title = _titleController.text.trim();
      final page = await repo.saveTextPage(
        text: text,
        title: title.isEmpty ? null : title,
        tags: _parsedTags(),
      );
      if (!mounted) return;
      _titleController.clear();
      _tagsController.clear();
      ref.invalidate(wikiPagesProvider);
      widget.onSaved?.call();
      openWikiPageTab(ref, page);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('已保存到知识库：《${page.title}》'),
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
    return Padding(
      padding: const EdgeInsets.all(AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildFields(),
          const SizedBox(height: AppTheme.space3),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.surface3),
              ),
              clipBehavior: Clip.antiAlias,
              child: WikiContentEditor(
                key: _editorKey,
                slug: '',
                contentMd: '',
                editable: !_busy,
                shrinkWrap: false,
                enableSaveShortcut: false,
                onWikiLinkTap: (_) {},
                onSave: (_) async {},
              ),
            ),
          ),
          if (_error != null) ...[
            const SizedBox(height: AppTheme.space2),
            Text(
              _error!,
              style: TextStyle(fontSize: 12, color: AppTheme.error),
            ),
          ],
          const SizedBox(height: AppTheme.space3),
          Row(
            children: [
              const Spacer(),
              FilledButton.icon(
                onPressed: _busy ? null : _save,
                icon: _busy
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.bookmark_add_outlined, size: 16),
                label: Text(_busy ? '保存中…' : '保存到知识库'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildFields() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        TextField(
          controller: _titleController,
          enabled: !_busy,
          decoration: InputDecoration(
            hintText: '标题（可选，留空自动取前 60 字）',
            hintStyle: TextStyle(fontSize: 13, color: AppTheme.textTertiary),
            isDense: true,
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 12,
              vertical: 10,
            ),
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
          ),
        ),
        const SizedBox(height: AppTheme.space2),
        TextField(
          controller: _tagsController,
          enabled: !_busy,
          style: TextStyle(fontSize: 13.5, color: AppTheme.textPrimary),
          cursorColor: AppTheme.accentPrimary,
          decoration: InputDecoration(
            hintText: '标签（可选，空格或逗号分隔，例如：工作 投资）',
            hintStyle: TextStyle(fontSize: 13, color: AppTheme.textTertiary),
            prefixIcon: Icon(Icons.sell_outlined, size: 16),
            prefixIconConstraints: const BoxConstraints(
              minWidth: 36,
              minHeight: 36,
            ),
            isDense: true,
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 12,
              vertical: 10,
            ),
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
          ),
        ),
      ],
    );
  }
}
