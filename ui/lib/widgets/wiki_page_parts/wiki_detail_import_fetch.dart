part of '../wiki_page_detail_view.dart';

//! 导入抓取 tab（URL/tweet 抓取与提交）

// ─────────────────────────────────────────────
// 任意网址预览 tab：内容 + 保存（点保存才入库）
// ─────────────────────────────────────────────

class _ImportFetchTabBody extends ConsumerStatefulWidget {
  final ImportFetch fetch;

  const _ImportFetchTabBody({required this.fetch});

  @override
  ConsumerState<_ImportFetchTabBody> createState() =>
      _ImportFetchTabBodyState();
}

class _ImportFetchTabBodyState extends ConsumerState<_ImportFetchTabBody> {
  bool _saving = false;
  bool _saved = false;
  String? _saveError;
  final _tagsController = TextEditingController();

  @override
  void dispose() {
    _tagsController.dispose();
    super.dispose();
  }

  /// 解析标签输入：空格 / 逗号 / 顿号分隔，去掉 # 前缀
  List<String> _parsedTags() => _tagsController.text
      .split(RegExp(r'[\s,，、]+'))
      .map((t) => t.trim().replaceFirst(RegExp(r'^#+'), '').trim())
      .where((t) => t.isNotEmpty)
      .toList();

  Future<void> _save() async {
    if (_saving || _saved) return;
    setState(() {
      _saving = true;
      _saveError = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final page = await saveKnowledgeImport(
        context,
        ref,
        widget.fetch,
        _parsedTags(),
      );
      if (page == null) return;
      final inputRecordId = widget.fetch.inputRecordId;
      if (inputRecordId != null) {
        await repo.finishUrlInput(inputRecordId, wikiPageSlug: page.slug);
      }
      if (!mounted) return;
      setState(() => _saved = true);
      ref.invalidate(wikiPagesProvider);
      openWikiPageTab(ref, page);
      closeWikiTab(ref, wikiImportTabId(widget.fetch));
    } catch (e) {
      if (!mounted) return;
      setState(() => _saveError = e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final fetch = widget.fetch;
    final title = fetch.displayTitle;
    final host = Uri.tryParse(fetch.sourceUrl)?.host ?? fetch.sourceUrl;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // 头部：标题 + 来源 + 保存
        Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space6,
            AppTheme.space4,
            AppTheme.space6,
            AppTheme.space4,
          ),
          decoration: BoxDecoration(
            border: Border(
              bottom: BorderSide(color: AppTheme.surface3, width: 1),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: fetch.isTweet
                          ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                          : AppTheme.surface3,
                      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                    ),
                    child: Text(
                      fetch.isTweet ? '推文' : '网页',
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: fetch.isTweet
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                        color: AppTheme.textPrimary,
                        height: 1.3,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AppTheme.space2),
              Row(
                children: [
                  Icon(Icons.link, size: 12, color: AppTheme.textTertiary),
                  const SizedBox(width: 4),
                  Expanded(
                    child: Text(
                      host,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              TextField(
                controller: _tagsController,
                enabled: !_saving && !_saved,
                style: TextStyle(fontSize: 13, color: AppTheme.textPrimary),
                cursorColor: AppTheme.accentPrimary,
                decoration: InputDecoration(
                  hintText: '标签（可选，空格分隔，例如：方法 投资）',
                  hintStyle: TextStyle(
                    fontSize: 12.5,
                    color: AppTheme.textTertiary,
                  ),
                  prefixIcon: Icon(Icons.sell_outlined, size: 15),
                  prefixIconConstraints: const BoxConstraints(
                    minWidth: 32,
                    minHeight: 32,
                  ),
                  isDense: true,
                  contentPadding: const EdgeInsets.symmetric(vertical: 8),
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
                    borderSide: BorderSide(
                      color: AppTheme.accentPrimary,
                      width: 1.5,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: AppTheme.space2),
              Row(
                children: [
                  const Spacer(),
                  if (_saved)
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 6,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.accentPrimary.withValues(alpha: 0.14),
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusMedium,
                        ),
                      ),
                      child: Row(
                        children: [
                          Icon(
                            Icons.check,
                            size: 14,
                            color: AppTheme.accentPrimary,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            '已保存到知识库',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.accentPrimary,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ],
                      ),
                    )
                  else
                    FilledButton.icon(
                      onPressed: _saving ? null : _save,
                      icon: _saving
                          ? const SizedBox(
                              width: 14,
                              height: 14,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.bookmark_add_outlined, size: 16),
                      label: Text(_saving ? '保存中…' : '保存到知识库'),
                    ),
                ],
              ),
              if (_saveError != null) ...[
                const SizedBox(height: AppTheme.space2),
                Text(
                  _saveError!,
                  style: TextStyle(fontSize: 12, color: AppTheme.error),
                ),
              ],
            ],
          ),
        ),
        // 正文预览
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(AppTheme.space4),
            child: ContentFontScope(
              child: SelectableText(
                fetch.contentMd,
                style: TextStyle(
                  fontSize: 13.5,
                  color: AppTheme.textPrimary,
                  height: 1.7,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
