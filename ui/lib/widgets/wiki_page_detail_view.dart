import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../bridge/rust_bridge_repository.dart';
import '../models/relation.dart';
import '../models/tweet_fetch.dart';
import '../models/import_fetch.dart';
import '../models/wiki_page.dart';
import '../models/conversation.dart';
import '../providers/wiki_provider.dart';
import '../theme/app_theme.dart';
import 'markdown_view.dart';

/// 右侧知识库面板：多 tab。
/// - 固定 Tab 1：导入（粘贴链接抓取或直接文本保存）
/// - 抓取成功后新开「预览」tab：内容 + 保存按钮（点保存才入库）
/// - 从左侧点击知识库页时新开「页面详情」tab（含 AI 处理面板）
class WikiPageDetailView extends ConsumerWidget {
  const WikiPageDetailView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tabs = ref.watch(wikiOpenTabsProvider);
    final activeId = ref.watch(wikiActiveTabIdProvider);
    final active = tabs.firstWhere(
      (t) => t.id == activeId,
      orElse: () => tabs.first,
    );

    return Column(
      children: [
        _WikiTabBar(tabs: tabs, activeId: active.id),
        Expanded(
          child: switch (active) {
            ImportTabEntry() => const _ImportTab(),
            PageTabEntry() => _PageTabBody(slug: active.slug),
            TweetTabEntry() => _TweetTabBody(fetch: active.fetch),
            ImportFetchTabEntry() => _ImportFetchTabBody(fetch: active.fetch),
          },
        ),
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 顶部 tab 条
// ─────────────────────────────────────────────

class _WikiTabBar extends ConsumerWidget {
  final List<WikiTabEntry> tabs;
  final String activeId;

  const _WikiTabBar({required this.tabs, required this.activeId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Container(
      height: 42,
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space3,
        vertical: 5,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          bottom: BorderSide(color: AppTheme.surface3, width: 1),
        ),
      ),
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        itemCount: tabs.length,
        separatorBuilder: (_, _) => const SizedBox(width: 6),
        itemBuilder: (context, index) {
          final tab = tabs[index];
          return _WikiTabChip(
            tab: tab,
            active: tab.id == activeId,
            onTap: () {
              ref.read(wikiActiveTabIdProvider.notifier).state = tab.id;
            },
            onClose: tab.closable ? () => closeWikiTab(ref, tab.id) : null,
          );
        },
      ),
    );
  }
}

class _WikiTabChip extends StatelessWidget {
  final WikiTabEntry tab;
  final bool active;
  final VoidCallback onTap;
  final VoidCallback? onClose;

  const _WikiTabChip({
    required this.tab,
    required this.active,
    required this.onTap,
    this.onClose,
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
          padding: const EdgeInsets.only(left: 12, right: 4),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 150),
                child: Text(
                  tab.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 12.5,
                    fontWeight: active ? FontWeight.w600 : FontWeight.w400,
                    color: active ? AppTheme.textPrimary : AppTheme.textSecondary,
                  ),
                ),
              ),
              if (onClose != null) ...[
                const SizedBox(width: 2),
                _buildCloseButton(),
              ] else
                const SizedBox(width: 8),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCloseButton() {
    return InkWell(
      onTap: onClose,
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      child: Padding(
        padding: EdgeInsets.all(4),
        child: Icon(
          Icons.close,
          size: 13,
          color: AppTheme.textTertiary,
        ),
      ),
    );
  }
}

// ─────────────────────────────────────────────
// Tab 1（固定）：导入（网址 / 文本）
// ─────────────────────────────────────────────

class _ImportTab extends ConsumerStatefulWidget {
  const _ImportTab();

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
  bool _isUrlMode = true;

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
        // 推文走专用路径（支持 author 展示 + slug 去重）
        final existing = await repo.findTweetSourcePage(url);
        if (!mounted) return;
        if (existing != null) {
          _urlController.clear();
          openWikiPageTab(ref, existing);
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('知识库中已保存过该推文，已直接打开')),
          );
          return;
        }
        final fetch = await repo.fetchTweet(url);
        if (!mounted) return;
        _urlController.clear();
        openWikiTweetTab(ref, fetch);
      } else {
        // 通用网页 / 非推文链接
        final fetch = await repo.fetchImportUrl(url);
        if (!mounted) return;
        _urlController.clear();
        openWikiImportFetchTab(ref, fetch);
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
          // 模式切换
          _buildModeSwitch(),
          const SizedBox(height: AppTheme.space4),
          if (_isUrlMode) ...[
            _buildUrlInput(),
          ] else ...[
            _buildTextInput(),
          ],
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
              Icon(Icons.lightbulb_outline, size: 14, color: AppTheme.accentPrimary),
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
              borderSide: BorderSide(
                color: AppTheme.accentPrimary,
                width: 1.5,
              ),
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
              borderSide: BorderSide(
                color: AppTheme.accentPrimary,
                width: 1.5,
              ),
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

// ─────────────────────────────────────────────
// 页面详情 tab
// ─────────────────────────────────────────────

class _PageTabBody extends ConsumerWidget {
  final String slug;

  const _PageTabBody({required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pageAsync = ref.watch(wikiPageProvider(slug));
    return pageAsync.when(
      data: (page) {
        if (page == null) {
          return Center(
            child: Text(
              '页面不存在或已被删除',
              style: TextStyle(color: AppTheme.textSecondary),
            ),
          );
        }
        return _WikiPageBody(page: page);
      },
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (error, stack) => Center(
        child: Text(
          '加载失败: $error',
          style: TextStyle(color: AppTheme.textSecondary),
        ),
      ),
    );
  }
}

/// 阅读栏最大宽度：正文与头部共用，保证长文行宽舒适、视线不来回扫。
const double _kReadingMaxWidth = 760;

class _WikiPageBody extends ConsumerWidget {
  final WikiPage page;

  const _WikiPageBody({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildHeader(context, ref),

        // 正文：居中阅读栏（限制行宽 + 宽松留白）
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space6,
              AppTheme.space4,
              AppTheme.space6,
              AppTheme.space6,
            ),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
                child: SelectableRegion(
                  focusNode: FocusNode(),
                  selectionControls: materialTextSelectionControls,
                  child: MarkdownView(markdown: page.contentMd),
                ),
              ),
            ),
          ),
        ),

        // 溯源脚注 + AI 处理入口
        _buildFooter(),
      ],
    );
  }

  Widget _buildHeader(BuildContext context, WidgetRef ref) {
    return Container(
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
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // 元数据行：类型 / slug / 证据 / 更新 / 来源
              Wrap(
                spacing: AppTheme.space2,
                runSpacing: AppTheme.space2,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                    ),
                    child: Text(
                      page.kindLabel,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.accentPrimary,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 320),
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 8,
                        vertical: 3,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.surface3,
                        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                      ),
                      child: Text(
                        page.slug,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textTertiary,
                          fontFamily: 'monospace',
                        ),
                      ),
                    ),
                  ),
                  Text(
                    '证据 ${page.evidenceCount} · 更新 ${_fmtDate(page.updatedAt)}',
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  if (page.sourceUrl != null) _SourceChip(url: page.sourceUrl!),
                ],
              ),
              const SizedBox(height: AppTheme.space4),
              Text(
                page.title,
                style: TextStyle(
                  fontSize: 26,
                  fontWeight: FontWeight.w700,
                  color: AppTheme.textPrimary,
                  height: 1.3,
                  letterSpacing: -0.2,
                ),
              ),
              // 摘要：让阅读者先扫到这一页在讲什么，再决定是否细读
              if (_hasSummary)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: Text(
                    page.summary.trim(),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 13,
                      height: 1.6,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
              const SizedBox(height: AppTheme.space3),
              _buildTagRow(context, ref),
              const SizedBox(height: AppTheme.space3),
              _buildRelationsRow(context, ref),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildFooter() {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space2,
        AppTheme.space6,
        AppTheme.space3,
      ),
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(color: AppTheme.surface3, width: 1),
        ),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '源于 ${page.sourceEventIds.isEmpty ? "尚无事件溯源" : "${page.sourceEventIds.length} 条事件"}',
                style: TextStyle(
                  fontSize: 11,
                  color: AppTheme.textTertiary,
                ),
              ),
              const SizedBox(height: AppTheme.space2),
              _PageAiChatPanel(slug: page.slug),
            ],
          ),
        ),
      ),
    );
  }

  /// 标签行：标签 chips + 编辑入口（标签是用户组织知识库的主要元数据）
  Widget _buildTagRow(BuildContext context, WidgetRef ref) {
    return Wrap(
      spacing: 6,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final tag in page.tags)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              border: Border.all(color: AppTheme.surface3),
            ),
            child: Text(
              '#$tag',
              style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
            ),
          ),
        InkWell(
          onTap: () => _editTags(context, ref),
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  page.tags.isEmpty ? Icons.add : Icons.edit_outlined,
                  size: 12,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 4),
                Text(
                  page.tags.isEmpty ? '添加标签' : '编辑标签',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  /// 人物关系区块：AI 从对话识别、用户确认后保存的「人物 ↔ 事情/项目」。
  /// 双侧方向都以当前页为中心展示（出→ 人·事；入← 人·事）。
  Widget _buildRelationsRow(BuildContext context, WidgetRef ref) {
    final relationsAsync = ref.watch(pageRelationsProvider(page.slug));
    final relations = relationsAsync.valueOrNull ?? const [];
    if (relations.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '人物关系',
          style: TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            color: AppTheme.textTertiary,
            letterSpacing: 0.5,
          ),
        ),
        const SizedBox(height: AppTheme.space2),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final r in relations) _RelationChip(relation: r, pageSlug: page.slug),
          ],
        ),
      ],
    );
  }

  /// 编辑标签：空格 / 逗号分隔，留空即清空。保存后刷新页面与列表。
  Future<void> _editTags(BuildContext context, WidgetRef ref) async {
    final controller = TextEditingController(text: page.tags.join(' '));
    final submitted = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppTheme.surface1,
        title: Text(
          '编辑标签',
          style: TextStyle(color: AppTheme.textPrimary, fontSize: 16),
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '用空格或逗号分隔多个标签，留空即清空',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: AppTheme.space3),
            TextField(
              controller: controller,
              autofocus: true,
              style: TextStyle(color: AppTheme.textPrimary),
              cursorColor: AppTheme.accentPrimary,
              decoration: InputDecoration(
                hintText: '例如：工作 Rust 投资',
                hintStyle: TextStyle(color: AppTheme.textTertiary),
                enabledBorder: OutlineInputBorder(
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderSide: BorderSide(color: AppTheme.accentPrimary),
                ),
              ),
              onSubmitted: (value) => Navigator.of(dialogContext).pop(value),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: AppTheme.accentPrimary),
            onPressed: () => Navigator.of(dialogContext).pop(controller.text),
            child: const Text('保存'),
          ),
        ],
      ),
    );
    if (submitted == null || !context.mounted) return;

    final tags = submitted
        .split(RegExp(r'[\s,，、]+'))
        .map((t) => t.trim().replaceFirst(RegExp(r'^#+'), '').trim())
        .where((t) => t.isNotEmpty)
        .toList();

    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.updateWikiTags(slug: page.slug, tags: tags);
      if (!context.mounted) return;
      ref.invalidate(wikiPageProvider(page.slug));
      ref.invalidate(pageRelationsProvider(page.slug));
      ref.invalidate(wikiPagesProvider);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('标签已更新'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('标签更新失败：${e.toString().replaceFirst('Exception: ', '')}'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }

  String _fmtDate(DateTime t) {
    return '${t.year}-${t.month.toString().padLeft(2, '0')}-${t.day.toString().padLeft(2, '0')}';
  }

  /// 摘要是否有展示价值：非空、且不是标题的重复
  bool get _hasSummary {
    final s = page.summary.trim();
    if (s.isEmpty) return false;
    return page.title.trim() != s;
  }
}

/// 来源链接 chip：点击复制
class _SourceChip extends StatelessWidget {
  final String url;

  const _SourceChip({required this.url});

  @override
  Widget build(BuildContext context) {
    final host = Uri.tryParse(url)?.host ?? url;
    return InkWell(
      onTap: () {
        Clipboard.setData(ClipboardData(text: url));
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('来源链接已复制'),
            behavior: SnackBarBehavior.floating,
          ),
        );
      },
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.link, size: 11, color: AppTheme.textTertiary),
            const SizedBox(width: 4),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 180),
              child: Text(
                host,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
            const SizedBox(width: 4),
            Icon(Icons.copy, size: 10, color: AppTheme.textTertiary),
          ],
        ),
      ),
    );
  }
}

/// 人物关系 chip：以当前页为中心展示一条关系（出→ / 入←），点击跳到对端页面。
/// 对端标题从 wikiPageProvider 读取（避开仅 slug 的冷展示）。
class _RelationChip extends ConsumerWidget {
  final Relation relation;
  final String pageSlug;

  const _RelationChip({required this.relation, required this.pageSlug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final outbound = relation.fromSlug == pageSlug;
    final otherSlug = outbound ? relation.toSlug : relation.fromSlug;
    final otherAsync = ref.watch(wikiPageProvider(otherSlug));
    final other = otherAsync.valueOrNull;
    final otherTitle = other?.title ?? otherSlug;
    final arrow = outbound ? '→' : '←';

    return InkWell(
      onTap: () {
        final slug = otherSlug;
        final title = other?.title ?? slug;
        openWikiTab(ref, PageTabEntry(slug: slug, title: title));
      },
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              arrow,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
            const SizedBox(width: 4),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: Text(
                otherTitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
            const SizedBox(width: 4),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
              child: Text(
                relation.relation,
                style: TextStyle(
                  fontSize: 11,
                  color: AppTheme.accentPrimary,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 页内 AI 处理面板：围绕当前页面聊天（总结/补充/改写），
/// 需要改页时模型调用 save_wiki_revision，确认后才写库。
class _PageAiChatPanel extends ConsumerStatefulWidget {
  final String slug;

  const _PageAiChatPanel({required this.slug});

  @override
  ConsumerState<_PageAiChatPanel> createState() => _PageAiChatPanelState();
}

class _PageAiChatPanelState extends ConsumerState<_PageAiChatPanel> {
  String? _conversationId;
  List<Message> _messages = [];
  bool _ready = false;
  bool _busy = false;
  String? _error;
  final _inputController = TextEditingController();
  final _scrollController = ScrollController();

  /// 快捷指令：点击即把对应 prompt 发给本页 AI（结果需确认才写库）
  static const _quickPrompts = [
    _QuickPrompt(label: '总结', prompt: '给我总结一下这一页，用要点列出核心信息'),
    _QuickPrompt(label: '提取要点', prompt: '提取这一页的关键信息，按重要性列出'),
    _QuickPrompt(label: '写抖音文案', prompt: '根据这一页内容，写一段适合发抖音的文案'),
    _QuickPrompt(label: '翻译成英文', prompt: '把这一页翻译成英文'),
  ];

  @override
  void initState() {
    super.initState();
    _ensureConversation();
  }

  @override
  void dispose() {
    _inputController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _ensureConversation() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final conv = await repo.ensureWikiPageChat(widget.slug);
      if (!mounted) return;
      setState(() {
        _conversationId = conv.id;
        _ready = true;
      });
      await _loadMessages();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _ready = true;
        _error = '会话初始化失败：${e.toString().replaceFirst('Exception: ', '')}';
      });
    }
  }

  Future<void> _loadMessages() async {
    final id = _conversationId;
    if (id == null) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final messages = await repo.listMessages(id);
      if (!mounted) return;
      setState(() => _messages = messages);
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '消息加载失败：${e.toString().replaceFirst('Exception: ', '')}');
    }
  }

  Future<void> _send(String raw) async {
    final text = raw.trim();
    final id = _conversationId;
    if (text.isEmpty || id == null || _busy) return;
    setState(() {
      _messages.add(Message(
        id: 'local-${DateTime.now().microsecondsSinceEpoch}',
        conversationId: id,
        role: MessageRole.user,
        content: text,
        createdAt: DateTime.now(),
      ));
      _busy = true;
      _error = null;
    });
    _inputController.clear();
    _scrollToBottom();
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.sendMessage(id, 'user', text);
      // 追加 AI 回复
      final reply = await repo.generateReply(id);
      if (!mounted) return;
      setState(() {
        _messages.add(Message(
          id: 'ai-${DateTime.now().microsecondsSinceEpoch}',
          conversationId: id,
          role: MessageRole.assistant,
          content: reply,
          createdAt: DateTime.now(),
        ));
        _busy = false;
      });
      _scrollToBottom();
      // 页面内容可能被修订：让页面详情 provider 失效以刷新
      ref.invalidate(wikiPageProvider(widget.slug));
      ref.invalidate(pageRelationsProvider(widget.slug));
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = e.toString().replaceFirst('Exception: ', '');
      });
      _scrollToBottom();
    }
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      _scrollController.animateTo(
        _scrollController.position.maxScrollExtent,
        duration: const Duration(milliseconds: 200),
        curve: Curves.easeOut,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(top: AppTheme.space2),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // 头部
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space3,
              AppTheme.space2,
              AppTheme.space3,
              AppTheme.space2,
            ),
            child: Row(
              children: [
                Icon(Icons.auto_awesome, size: 14, color: AppTheme.accentPrimary),
                const SizedBox(width: 6),
                Text(
                  'AI 处理本页',
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textSecondary,
                  ),
                ),
                const Spacer(),
                if (!_ready)
                  const SizedBox(
                    width: 12,
                    height: 12,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
              ],
            ),
          ),
          Divider(height: 1, color: AppTheme.surface3),
          // 快捷指令：一键触发常见处理，结果需确认才写库
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space3,
              AppTheme.space2,
              AppTheme.space3,
              0,
            ),
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final p in _quickPrompts)
                  _QuickPromptChip(
                    label: p.label,
                    enabled: _ready && !_busy,
                    onTap: () => _send(p.prompt),
                  ),
              ],
            ),
          ),
          const SizedBox(height: AppTheme.space2),
          // 消息区
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 160),
            child: ListView(
              controller: _scrollController,
              shrinkWrap: true,
              padding: const EdgeInsets.all(AppTheme.space3),
              children: [
                _ChatBubble(
                  message: const ContentChatMessage(
                    role: 'assistant',
                    content: '👋 我可以帮你总结、提取要点、补充或改写这一页；需要写回知识库时会先给你确认。',
                  ),
                ),
                for (final m in _messages)
                  _ChatBubble(
                    message: ContentChatMessage(role: m.role.name, content: m.content),
                  ),
                if (_busy) const _ChatBubble.pending(),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: AppTheme.space2),
                    child: Text(
                      _error!,
                      style: TextStyle(fontSize: 12, color: AppTheme.error),
                    ),
                  ),
              ],
            ),
          ),
          // 输入区
          Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space3,
              0,
              AppTheme.space3,
              AppTheme.space3,
            ),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _inputController,
                    enabled: _ready && !_busy,
                    minLines: 1,
                    maxLines: 3,
                    onSubmitted: (_) => _send(_inputController.text),
                    decoration: InputDecoration(
                      hintText: '就这一页问问 AI…（回车发送）',
                      hintStyle: TextStyle(
                        fontSize: 12,
                        color: AppTheme.textTertiary,
                      ),
                      isDense: true,
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 8,
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
                IconButton.filled(
                  onPressed: (_ready && !_busy)
                      ? () => _send(_inputController.text)
                      : null,
                  style: IconButton.styleFrom(
                    backgroundColor: AppTheme.accentPrimary,
                    disabledBackgroundColor: AppTheme.surface3,
                  ),
                  icon: const Icon(Icons.arrow_upward, size: 16),
                  tooltip: '发送',
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// 页内 AI 快捷指令定义
class _QuickPrompt {
  final String label;
  final String prompt;

  const _QuickPrompt({required this.label, required this.prompt});
}

/// 快捷指令 chip：点击把对应 prompt 交给页面 AI
class _QuickPromptChip extends StatelessWidget {
  final String label;
  final bool enabled;
  final VoidCallback onTap;

  const _QuickPromptChip({
    required this.label,
    required this.enabled,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: enabled ? onTap : null,
      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          color: enabled
              ? AppTheme.accentPrimary.withValues(alpha: 0.10)
              : AppTheme.surface2,
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          border: Border.all(
            color: enabled
                ? AppTheme.accentPrimary.withValues(alpha: 0.35)
                : AppTheme.surface3,
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.bolt,
              size: 11,
              color: enabled ? AppTheme.accentPrimary : AppTheme.textTertiary,
            ),
            const SizedBox(width: 4),
            Text(
              label,
              style: TextStyle(
                fontSize: 11,
                color: enabled ? AppTheme.accentPrimary : AppTheme.textTertiary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ─────────────────────────────────────────────
// 任意网址预览 tab：内容 + 保存（点保存才入库）
// ─────────────────────────────────────────────

class _ImportFetchTabBody extends ConsumerStatefulWidget {
  final ImportFetch fetch;

  const _ImportFetchTabBody({required this.fetch});

  @override
  ConsumerState<_ImportFetchTabBody> createState() => _ImportFetchTabBodyState();
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
      final page = await repo.saveImportedPage(
        title: widget.fetch.displayTitle,
        contentMd: widget.fetch.contentMd,
        sourceUrl: widget.fetch.sourceUrl,
        sourceKind: widget.fetch.sourceKind,
        tags: _parsedTags(),
      );
      if (!mounted) return;
      setState(() => _saved = true);
      ref.invalidate(wikiPagesProvider);
      openWikiPageTab(ref, page);
      closeWikiTab(ref, widget.fetch.sourceUrl.hashCode.toString());
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
                        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                      ),
                      child: Row(
                        children: [
                          Icon(Icons.check, size: 14, color: AppTheme.accentPrimary),
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
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 推文预览 tab：内容 + 对话 + 保存
// ─────────────────────────────────────────────

class _TweetTabBody extends ConsumerStatefulWidget {
  final TweetFetch fetch;

  const _TweetTabBody({required this.fetch});

  @override
  ConsumerState<_TweetTabBody> createState() => _TweetTabBodyState();
}

class _TweetTabBodyState extends ConsumerState<_TweetTabBody> {
  bool _saving = false;
  bool _saved = false;
  String? _saveError;

  // 临时内容对话（不入库，直到点击保存；对话本身也不写库）
  final List<ContentChatMessage> _chat = [];
  bool _chatBusy = false;
  String? _chatError;
  final _chatController = TextEditingController();
  final _chatFocusNode = FocusNode();
  final _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    // 回车直接发送；Shift+回车换行（与 MessageArea 一致）
    _chatFocusNode.onKeyEvent = (node, event) {
      final isEnter = event.logicalKey == LogicalKeyboardKey.enter;
      final isShift = HardwareKeyboard.instance.isShiftPressed;
      if (event is KeyDownEvent && isEnter && !isShift) {
        _sendChat(_chatController.text);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    };
  }

  @override
  void dispose() {
    _chatController.dispose();
    _chatFocusNode.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_saving || _saved) return;
    setState(() {
      _saving = true;
      _saveError = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final page = await repo.saveTweetPage(
        tweetId: widget.fetch.tweetId,
        text: widget.fetch.text,
        title: widget.fetch.title,
        authorName: widget.fetch.authorName,
        screenName: widget.fetch.screenName,
      );
      if (!mounted) return;
      setState(() {
        _saving = false;
        _saved = true;
      });
      // 刷新左侧知识库列表，让「来源」分组出现新页
      ref.invalidate(wikiPagesProvider);
      // 本次『重新抓取+保存』更新的是同一 slug 的旧页：使页面内容 provider 失效，
      // 否则已缓存的旧内容会一直显示（比如旧版本存的纯链接）。
      ref.invalidate(wikiPageProvider(page.slug));
      ref.invalidate(pageRelationsProvider(page.slug));
      // 直接打开已保存的页面，立即看到最新内容，避免再点左侧列表却读到缓存旧页
      openWikiPageTab(ref, page);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('已保存到知识库：《${page.title}》'),
          behavior: SnackBarBehavior.floating,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(999)),
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _saveError = '保存失败：${e.toString().replaceFirst('Exception: ', '')}';
      });
    }
  }

  Future<void> _sendChat(String raw) async {
    final text = raw.trim();
    if (text.isEmpty || _chatBusy) return;
    setState(() {
      _chat.add(ContentChatMessage(role: 'user', content: text));
      _chatError = null;
      _chatBusy = true;
    });
    _chatController.clear();
    _scrollToBottom();
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      // 只传最近若干条，控制单次请求体积
      final history = _chat.length > 24
          ? _chat.sublist(_chat.length - 24)
          : List<ContentChatMessage>.from(_chat);
      final reply = await repo.generateContentChat(
        content: widget.fetch.fullContent,
        messages: history,
      );
      if (!mounted) return;
      setState(() {
        _chat.add(ContentChatMessage(role: 'assistant', content: reply));
        _chatBusy = false;
      });
      _scrollToBottom();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _chatBusy = false;
        _chatError = 'AI 回复失败：${e.toString().replaceFirst('Exception: ', '')}';
      });
      _scrollToBottom();
    }
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      _scrollController.animateTo(
        _scrollController.position.maxScrollExtent,
        duration: const Duration(milliseconds: 200),
        curve: Curves.easeOut,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _buildHeader(),
        Divider(height: 1, color: AppTheme.surface3),
        Expanded(
          child: ListView(
            controller: _scrollController,
            padding: const EdgeInsets.all(AppTheme.space4),
            children: [
              _buildTweetCard(),
              const SizedBox(height: AppTheme.space4),
              Row(
                children: [
                  Text(
                    '与 AI 讨论这篇推文',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                  const Spacer(),
                  if (_chat.isEmpty)
                    Text(
                      '保存前可先和 AI 过一遍，点「保存到知识库」才入库',
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                ],
              ),
              const SizedBox(height: AppTheme.space3),
              for (final msg in _chat) _ChatBubble(message: msg),
              if (_chatBusy) const _ChatBubble.pending(),
              if (_chatError != null)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: Text(
                    _chatError!,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.error,
                      height: 1.5,
                    ),
                  ),
                ),
              if (_saveError != null)
                Padding(
                  padding: const EdgeInsets.only(top: AppTheme.space2),
                  child: Text(
                    _saveError!,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.error,
                      height: 1.5,
                    ),
                  ),
                ),
            ],
          ),
        ),
        _buildChatInput(),
      ],
    );
  }

  Widget _buildHeader() {
    final fetch = widget.fetch;
    final author = fetch.authorName ??
        (fetch.screenName != null ? '@${fetch.screenName}' : '未知作者');
    // 文章型推文用文章标题，普通推文用「{作者} 的推文」
    final mainTitle = (fetch.title?.trim().isNotEmpty ?? false)
        ? fetch.title!.trim()
        : '$author 的推文';
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space3,
        AppTheme.space4,
        AppTheme.space3,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  mainTitle,
                  style: TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w700,
                    color: AppTheme.textPrimary,
                    height: 1.3,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  '@${fetch.screenName ?? '—'} · 推文 ${fetch.tweetId}',
                  style: TextStyle(
                    fontSize: 11,
                    color: AppTheme.textTertiary,
                  ),
                ),
              ],
            ),
          ),
          if (_saved)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.14),
                borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.check_circle_outline,
                    size: 14,
                    color: AppTheme.accentPrimary,
                  ),
                  SizedBox(width: 4),
                  Text(
                    '已保存',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.accentPrimary,
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
    );
  }

  Widget _buildTweetCard() {
    final fetch = widget.fetch;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.article_outlined,
                size: 14,
                color: AppTheme.textTertiary,
              ),
              const SizedBox(width: 6),
              Text(
                '抓取内容（原文）',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
              const Spacer(),
              InkWell(
                onTap: () {
                  Clipboard.setData(ClipboardData(text: fetch.url));
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(
                      content: Text('推文链接已复制'),
                      behavior: SnackBarBehavior.floating,
                    ),
                  );
                },
                borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                child: Padding(
                  padding: EdgeInsets.all(4),
                  child: Icon(
                    Icons.link,
                    size: 14,
                    color: AppTheme.textTertiary,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          SelectableText(
            fetch.text,
            style: TextStyle(
              fontSize: 14,
              height: 1.7,
              color: AppTheme.textPrimary,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildChatInput() {
    return Container(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space4,
        AppTheme.space3,
        AppTheme.space4,
        AppTheme.space4,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          top: BorderSide(color: AppTheme.surface3, width: 1),
        ),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Expanded(
            child: TextField(
              controller: _chatController,
              focusNode: _chatFocusNode,
              enabled: !_chatBusy,
              minLines: 1,
              maxLines: 4,
              decoration: InputDecoration(
                hintText: '就这篇推文问问 AI…（回车发送，Shift+回车换行）',
                hintStyle: TextStyle(
                  fontSize: 13,
                  color: AppTheme.textTertiary,
                ),
                filled: true,
                fillColor: AppTheme.surface2,
                isDense: true,
                contentPadding: EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 10,
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.all(Radius.circular(AppTheme.radiusMedium)),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(Radius.circular(AppTheme.radiusMedium)),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(Radius.circular(AppTheme.radiusMedium)),
                  borderSide: BorderSide(color: AppTheme.accentPrimary, width: 1.5),
                ),
              ),
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          IconButton.filled(
            onPressed: _chatBusy ? null : () => _sendChat(_chatController.text),
            style: IconButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
              disabledBackgroundColor: AppTheme.surface3,
            ),
            icon: const Icon(Icons.arrow_upward, size: 16),
            tooltip: '发送',
          ),
        ],
      ),
    );
  }
}

/// 内容对话的气泡（用户右对齐高亮，AI 左侧带头像）
class _ChatBubble extends StatelessWidget {
  final ContentChatMessage message;
  final bool pending;

  const _ChatBubble({required this.message}) : pending = false;

  const _ChatBubble.pending()
      : message = const ContentChatMessage(role: 'assistant', content: ''),
        pending = true;

  @override
  Widget build(BuildContext context) {
    final isUser = message.role == 'user';
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space3),
      child: Row(
        mainAxisAlignment:
            isUser ? MainAxisAlignment.end : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isUser) ...[
            _buildAvatar(),
            const SizedBox(width: 8),
          ] else
            const SizedBox(width: 48),
          Flexible(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: isUser
                    ? AppTheme.accentPrimary.withValues(alpha: 0.15)
                    : AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
              ),
              child: pending
                  ? Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        SizedBox(
                          width: 12,
                          height: 12,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                        SizedBox(width: 8),
                        Text(
                          '思考中…',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ],
                    )
                  : SelectableText(
                      message.content,
                      style: TextStyle(
                        fontSize: 13,
                        height: 1.55,
                        color: AppTheme.textPrimary,
                      ),
                    ),
            ),
          ),
          if (isUser) const SizedBox(width: 8),
        ],
      ),
    );
  }

  Widget _buildAvatar() {
    return Container(
      width: 26,
      height: 26,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: 0.18),
        shape: BoxShape.circle,
      ),
      child: Text(
        'EW',
        style: TextStyle(
          fontSize: 10,
          fontWeight: FontWeight.w700,
          color: AppTheme.accentPrimary,
        ),
      ),
    );
  }
}