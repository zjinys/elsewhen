part of 'wiki_page_detail_view.dart';

// ─────────────────────────────────────────────
// Tab 1（固定）：导入（网址 / 文本）
// ─────────────────────────────────────────────

class _ImportTab extends ConsumerStatefulWidget {
  final bool initialUrlMode;
  final VoidCallback? onCompleted;

  const _ImportTab({this.initialUrlMode = true, this.onCompleted});

  @override
  ConsumerState<_ImportTab> createState() => _ImportTabState();
}

class _ImportTabState extends ConsumerState<_ImportTab> {
  final _urlController = TextEditingController();
  final _textController = TextEditingController();
  final _titleController = TextEditingController();
  final _tagsController = TextEditingController();
  bool _busy = false;
  String? _error;
  late bool _isUrlMode;

  @override
  void initState() {
    super.initState();
    _isUrlMode = widget.initialUrlMode;
  }

  @override
  void dispose() {
    _urlController.dispose();
    _textController.dispose();
    _titleController.dispose();
    _tagsController.dispose();
    super.dispose();
  }

  /// 解析标签输入：空格 / 逗号 / 顿号分隔，去掉 # 前缀
  List<String> _parsedTags() => _tagsController.text
      .split(RegExp(r'[\s,，、]+'))
      .map((t) => t.trim().replaceFirst(RegExp(r'^#+'), '').trim())
      .where((t) => t.isNotEmpty)
      .toList();

  /// URL 模式：抓取推文/网页内容，成功后新开预览 tab
  Future<void> _fetchUrl() async {
    final url = _urlController.text.trim();
    if (url.isEmpty) {
      setState(() => _error = '请先粘贴一个链接');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      // 先判断 URL 类型（推文走专用 API，普通链接走网页提取）；同时校验 http/https
      final kind = await repo.guessImportKind(url);
      if (kind == 'tweet') {
        // 重复导入仍抓取最新原文，保存时对照旧版本并由用户确认。
        final fetch = await repo.fetchTweet(url);
        if (!mounted) return;
        _urlController.clear();
        openWikiTweetTab(ref, fetch);
        widget.onCompleted?.call();
      } else {
        // 通用网页 / 非推文链接
        final fetch = await repo.fetchImportUrl(url);
        if (!mounted) return;
        _urlController.clear();
        openWikiImportFetchTab(ref, fetch);
        widget.onCompleted?.call();
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString().replaceFirst('Exception: ', ''));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 文本模式：直接保存到知识库
  Future<void> _saveText() async {
    final text = _textController.text.trim();
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
      // 标签随保存一次写入（Rust 侧会保留「note」锚点标签并去重）
      var page = await repo.saveTextPage(
        text: text,
        title: title.isEmpty ? null : title,
        tags: _parsedTags(),
      );
      if (!mounted) return;
      _textController.clear();
      _titleController.clear();
      _tagsController.clear();
      ref.invalidate(wikiPagesProvider);
      openWikiPageTab(ref, page);
      widget.onCompleted?.call();
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
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (_isUrlMode) ...[_buildUrlInput()] else ...[_buildTextInput()],
          if (_error != null) ...[
            const SizedBox(height: AppTheme.space2),
            Text(
              _error!,
              style: TextStyle(fontSize: 12, color: AppTheme.error),
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildModeSwitch() {
    return Container(
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      padding: const EdgeInsets.all(3),
      child: Row(
        children: [
          Expanded(
            child: _ModeBtn(
              label: '🌐 网址导入',
              active: _isUrlMode,
              onTap: () => setState(() {
                _isUrlMode = true;
                _error = null;
              }),
            ),
          ),
          Expanded(
            child: _ModeBtn(
              label: '📝 直接文本',
              active: !_isUrlMode,
              onTap: () => setState(() {
                _isUrlMode = false;
                _error = null;
              }),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildUrlInput() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '粘贴任意网址：x.com/twitter.com 推文或其他网页链接，也支持文章型推文',
          style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
        ),
        const SizedBox(height: AppTheme.space2),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _urlController,
                enabled: !_busy,
                decoration: InputDecoration(
                  hintText: 'https://x.com/… 或 https://example.com/…',
                  hintStyle: TextStyle(
                    fontSize: 13,
                    color: AppTheme.textTertiary,
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
                    borderSide: BorderSide(
                      color: AppTheme.accentPrimary,
                      width: 1.5,
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: AppTheme.space2),
            FilledButton.icon(
              onPressed: _busy ? null : _fetchUrl,
              icon: _busy
                  ? const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.link, size: 16),
              label: Text(_busy ? '抓取中…' : '抓取'),
            ),
          ],
        ),
        const SizedBox(height: AppTheme.space3),
        // 快捷入口提示
        Container(
          padding: const EdgeInsets.all(AppTheme.space3),
          decoration: BoxDecoration(
            color: AppTheme.surface2,
            borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          ),
          child: Row(
            children: [
              Icon(
                Icons.lightbulb_outline,
                size: 14,
                color: AppTheme.accentPrimary,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  '抓取后会新开预览 tab，预览内容后点「保存到知识库」才会入库',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildTextInput() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        TextField(
          controller: _titleController,
          enabled: !_busy,
          textAlignVertical: TextAlignVertical.top,
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
          textAlignVertical: TextAlignVertical.top,
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
        const SizedBox(height: AppTheme.space2),
        SizedBox(
          height: 260,
          child: TextField(
            controller: _textController,
            enabled: !_busy,
            maxLines: null,
            expands: true,
            textAlignVertical: TextAlignVertical.top,
            decoration: InputDecoration(
              hintText: '粘贴或输入要保存的内容（Markdown 语法支持）',
              hintStyle: TextStyle(fontSize: 13, color: AppTheme.textTertiary),
              filled: true,
              fillColor: AppTheme.surface2,
              contentPadding: const EdgeInsets.all(12),
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
        ),
        const SizedBox(height: AppTheme.space3),
        Row(
          children: [
            const Spacer(),
            FilledButton.icon(
              onPressed: _busy ? null : _saveText,
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
    );
  }
}

/// 模式切换按钮
class _ModeBtn extends StatelessWidget {
  final String label;
  final bool active;
  final VoidCallback onTap;

  const _ModeBtn({
    required this.label,
    required this.active,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Material(
      color: active ? AppTheme.surface3 : Colors.transparent,
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Center(
            child: Text(
              label,
              style: TextStyle(
                fontSize: 12.5,
                fontWeight: active ? FontWeight.w600 : FontWeight.w400,
                color: active ? AppTheme.textPrimary : AppTheme.textSecondary,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

