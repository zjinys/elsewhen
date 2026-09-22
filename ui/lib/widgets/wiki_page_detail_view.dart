import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../bridge/generated.dart/api.dart' show EntityFactDto;
import '../models/relation.dart';
import '../models/tweet_fetch.dart';
import '../models/import_fetch.dart';
import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';
import '../providers/todo_provider.dart';
import '../models/todo.dart';
import '../theme/app_theme.dart';
import '../wiki/wiki_content_editor.dart';
import 'wiki_ai_chat_panel.dart';
import 'wiki_derivatives.dart';

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
          // IndexedStack：所有 tab 的子树常驻，切 tab 不销毁详情页编辑器状态
          // （未保存的编辑切走再切回仍在；§6.3 未保存保护的前提）
          child: IndexedStack(
            index: tabs.indexWhere((t) => t.id == active.id),
            children: [
              for (final tab in tabs)
                switch (tab) {
                  ImportTabEntry() => const _ImportTab(),
                  PageTabEntry(:final slug) => _PageTabBody(slug: slug),
                  TweetTabEntry(:final fetch) => _TweetTabBody(fetch: fetch),
                  ImportFetchTabEntry(:final fetch) =>
                    _ImportFetchTabBody(fetch: fetch),
                },
            ],
          ),
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
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
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
            onClose: tab.closable ? () => _closeTab(context, ref, tab) : null,
          );
        },
      ),
    );
  }

  /// 关闭 tab；若有未保存的编辑（§6.3），先弹确认再关（v1.5：三档）。
  Future<void> _closeTab(
    BuildContext context,
    WidgetRef ref,
    WikiTabEntry tab,
  ) async {
    final slug = switch (tab) {
      PageTabEntry(:final slug) => slug,
      _ => null,
    };
    final dirty = slug != null &&
        ref.read(wikiDirtyTabsProvider).contains(slug);
    if (!dirty) {
      closeWikiTab(ref, tab.id);
      return;
    }
    final action = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('关闭前确认'),
        content: const Text('该页面还有未保存的编辑，关闭将丢失这些修改。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('cancel'),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('discard'),
            child: const Text('放弃修改并关闭'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop('save'),
            child: const Text('保存并关闭'),
          ),
        ],
      ),
    );
    if (action == null || action == 'cancel') return;
    if (action == 'save') {
      // v1.5：先走页面注册的保存回调；保存失败/冲突取消则保持 tab 打开。
      final save = ref.read(wikiSaveCallbacksProvider)[slug];
      final ok = save == null ? false : await save();
      if (!ok) return;
    }
    ref.read(wikiDirtyTabsProvider.notifier).update(
      (set) => {...set}..remove(slug),
    );
    closeWikiTab(ref, tab.id);
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
    return Listener(
      // 鼠标中键点击 tab 关闭（浏览器/编辑器惯例），与「×」按钮同走
      // 关闭前脏检查（由上层传入的 onClose 承载确认逻辑）。
      onPointerDown: (event) {
        if (onClose != null && event.buttons == kMiddleMouseButton) {
          onClose!();
        }
      },
      child: Material(
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
                      color: active
                          ? AppTheme.textPrimary
                          : AppTheme.textSecondary,
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
      ),
    );
  }

  Widget _buildCloseButton() {
    return InkWell(
      onTap: onClose,
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      child: Padding(
        padding: EdgeInsets.all(4),
        child: Icon(Icons.close, size: 13, color: AppTheme.textTertiary),
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
          ScaffoldMessenger.of(context)
              .showSnackBar(const SnackBar(content: Text('知识库中已保存过该推文，已直接打开')));
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

/// 编辑冲突对话框中用户选择「取消/重新加载」时中止保存的内部信号：
/// 让 [WikiContentEditor.save] 返回 false（留在/退出编辑态由调用处控制），
/// 但不触发错误条（不是失败）。
class _SaveCancelled implements Exception {
  const _SaveCancelled();
}

class _WikiPageBody extends ConsumerStatefulWidget {
  final WikiPage page;

  const _WikiPageBody({required this.page});

  @override
  ConsumerState<_WikiPageBody> createState() => _WikiPageBodyState();
}

class _WikiPageBodyState extends ConsumerState<_WikiPageBody> {
  final _editorKey = GlobalKey<WikiContentEditorState>();

  /// 可编辑判定：素材页（source/note）只读，与 Rust 侧 `save_wiki_page_content`
  /// 保护一致（M1 素材保护）；人员/项目等 AI 档案页可人工修改。
  bool get _canEdit {
    final kind = widget.page.kind;
    return kind != 'source' && kind != 'note';
  }

  bool _editing = false;
  bool _saving = false;
  String? _editError;

  /// 保存回调注册表的 notifier 引用：dispose 后 `ref` 不可用，须提前缓存。
  late final StateController<Map<String, Future<bool> Function()>>
      _saveCallbacks;

  @override
  void initState() {
    super.initState();
    _saveCallbacks = ref.read(wikiSaveCallbacksProvider.notifier);
    // v1.5：向 tab bar 注册「保存」回调，关闭脏 tab 时可先保存再关。
    // 闭包惰性读 _editorKey.currentState，调用时机总在挂载之后。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _saveCallbacks.update(
        (map) => {...map, widget.page.slug: _finishEditing},
      );
    });
  }

  @override
  void dispose() {
    // dispose 发生在 widget tree 卸载期内，同步改 provider 会触发
    // 「Tried to modify a provider while the widget tree was building」；
    // 延迟到事件队列空闲时注销。notifier 已提前缓存，不依赖 ref。
    final notifier = _saveCallbacks;
    final slug = widget.page.slug;
    scheduleMicrotask(() {
      try {
        notifier.update((map) => {...map}..remove(slug));
      } on StateError {
        // provider 已随容器销毁（如测试 teardown）：无需注销
      }
    });
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildHeader(context),
        if (_editError != null) _buildEditErrorBar(),
        if (_canEdit) _buildEditorToolbar(),

        // 正文：编辑器自带滚动（含尾部对话块，§7 Form A；有限高约束，
        // vendor overlay 不允许无界父级），派生产物/事实/待办作为下方卡座。
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(
              AppTheme.space6,
              AppTheme.space3,
              AppTheme.space6,
              0,
            ),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (widget.page.basedOn != null)
                      WikiSourceLink(slug: widget.page.basedOn!),
                    _EntityMergeControls(page: widget.page),
                    const SizedBox(height: AppTheme.space3),
                    Expanded(
                      child: Center(
                        child: ConstrainedBox(
                          constraints: const BoxConstraints(
                            maxWidth: _kReadingMaxWidth,
                          ),
                          child: WikiContentEditor(
                            key: _editorKey,
                            slug: widget.page.slug,
                            contentMd: widget.page.contentMd,
                            editable: _editing,
                            onWikiLinkTap: _openWikiPageFromSlug,
                            onSave: _persistEdit,
                            onSaveError: (e) {
                              // 冲突对话框的取消/重载是用户主动选择，不是失败
                              if (!mounted || e is _SaveCancelled) return;
                              setState(() => _editError = _errText(e));
                            },
                            onDirtyChanged: _setDirty,
                          ),
                        ),
                      ),
                    ),
                    // 卡座：AI 派生产物 / 事实 / 相关待办（高度上限内自滚动）
                    _buildCardStrip(context),
                  ],
                ),
              ),
            ),
          ),
        ),

        // 溯源脚注（聊天入口已移入正文尾部对话块，§7 Form A）
        _buildFooter(),
      ],
    );
  }

  /// 编辑工具栏：入口「编辑正文」；编辑态「取消 · 完成（保存）」。
  Widget _buildEditorToolbar() {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space2,
        AppTheme.space6,
        AppTheme.space2,
      ),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  _editing
                      ? '编辑正文中 · Ctrl/⌘+S 保存'
                      : 'AI 生成正文，可人工修正',
                  style: TextStyle(
                    fontSize: 11,
                    color: AppTheme.textTertiary,
                  ),
                ),
              ),
              if (_editing) ...[
                TextButton(
                  onPressed: _saving ? null : _cancelEditing,
                  child: const Text('取消'),
                ),
                const SizedBox(width: 4),
                FilledButton(
                  onPressed: _saving ? null : _finishEditing,
                  child: _saving
                      ? const SizedBox(
                          width: 12,
                          height: 12,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('完成'),
                ),
              ] else
                FilledButton.tonalIcon(
                  onPressed: _enterEditing,
                  icon: const Icon(Icons.edit_outlined, size: 16),
                  label: const Text('编辑正文'),
                ),
            ],
          ),
        ),
      ),
    );
  }

  /// 卡座：AI 派生产物 / 事实 / 相关待办。
  /// 编辑器占据主滚动区后，这些区块落在下方，内部自滚动（上限约 42% 高度）。
  Widget _buildCardStrip(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final maxStrip = math.min(constraints.maxHeight * 0.42, 420.0);
        return ConstrainedBox(
          constraints: BoxConstraints(maxHeight: maxStrip),
          child: SingleChildScrollView(
            padding: const EdgeInsets.only(bottom: AppTheme.space4),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                WikiDerivatives(slug: widget.page.slug),
                _EntityFacts(slug: widget.page.slug),
                _EntityRelatedTodos(page: widget.page),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _buildEditErrorBar() {
    return Container(
      width: double.infinity,
      color: AppTheme.error.withValues(alpha: 0.08),
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space6,
        vertical: AppTheme.space2,
      ),
      child: Text(
        '保存失败：$_editError',
        style: TextStyle(fontSize: 12, color: AppTheme.error),
      ),
    );
  }

  /// 进入编辑态（素材页不显示入口，到不了这里）
  void _enterEditing() {
    setState(() {
      _editing = true;
      _saving = false;
      _editError = null;
    });
    _setDirty(_editorKey.currentState?.isDirty ?? false);
  }

  /// 「完成」：保存正文（无改动直接退出；保存失败留在编辑态，错误条已展示）。
  /// 返回是否保存成功/无改动——v1.5 关闭脏 tab 的「保存并关闭」复用此判定。
  Future<bool> _finishEditing() async {
    final state = _editorKey.currentState;
    if (state == null) return true; // 从未进入编辑态：视为无脏内容
    setState(() => _saving = true);
    final ok = await state.save();
    if (!mounted) return ok;
    setState(() {
      _saving = false;
      if (ok) _editing = false;
    });
    return ok;
  }

  /// 「取消」：放弃修改，从加载快照重建文档
  void _cancelEditing() {
    _editorKey.currentState?.discard();
    setState(() => _editing = false);
    _setDirty(false);
  }

  /// 保存链路（§6.2）：documentToMarkdown → saveWikiPageContent → 刷新 → 退出编辑态。
  /// 乐观锁（§11 Q3）：携带加载时的 updatedAt；编辑期间页面被后台更新则冲突，
  /// 弹「重新加载 / 强制覆盖 / 取消」三选；取消/重载抛 [_SaveCancelled]，
  /// 中止保存但不视为失败（不显示错误条）。
  Future<void> _persistEdit(String markdown) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.saveWikiPageContent(
        slug: widget.page.slug,
        contentMd: markdown,
        reason: 'GUI 编辑',
        expectedUpdatedAt: widget.page.updatedAt.toUtc().toIso8601String(),
      );
    } on Exception catch (e) {
      if (!mounted || !_isConflictError(e)) rethrow;
      // 保存尝试已被拒，先放下 saving 态再弹冲突对话框
      // （否则工具栏 spinner 常转，pumpAndSettle 永不收敛）。
      setState(() => _saving = false);
      final action = await _showConflictDialog();
      if (!mounted) throw const _SaveCancelled();
      switch (action) {
        case 'overwrite':
          // 强制覆盖：跳过乐观锁再存一次；仍失败则交给错误条
          await repo.saveWikiPageContent(
            slug: widget.page.slug,
            contentMd: markdown,
            reason: 'GUI 编辑（冲突后覆盖）',
            expectedUpdatedAt: null,
          );
        case 'reload':
          // 重新加载：放弃本地改动，刷新为最新页面内容
          ref.invalidate(wikiPageProvider(widget.page.slug));
          ref.invalidate(wikiPagesProvider);
          setState(() {
            _editing = false;
            _editError = null;
          });
          _setDirty(false);
          throw const _SaveCancelled();
        default:
          // 取消：留在编辑态，改动保留，不显示错误条
          throw const _SaveCancelled();
      }
    }
    ref.invalidate(wikiPageProvider(widget.page.slug));
    ref.invalidate(wikiPagesProvider);
    if (!mounted) return;
    setState(() {
      _editing = false;
      _editError = null;
    });
    _setDirty(false);
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        const SnackBar(
          content: Text('已保存到知识库'),
          behavior: SnackBarBehavior.floating,
          duration: Duration(seconds: 2),
        ),
      );
  }

  /// 冲突判定：Rust 侧乐观锁拒绝时的错误消息前缀
  static bool _isConflictError(Object e) =>
      e.toString().contains('编辑冲突');

  /// 编辑冲突三选：重新加载（弃本地）/ 强制覆盖（盖后台）/ 取消（继续编辑）
  Future<String?> _showConflictDialog() {
    return showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('页面已被后台更新'),
        content: const Text(
          '你编辑期间，该页面被后台 digest 更新过。\n'
          '「重新加载」放弃你的修改并查看最新内容；\n'
          '「强制覆盖」以你的修改覆盖后台更新（两个版本都留在修订历史里）。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('cancel'),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('reload'),
            child: const Text('重新加载'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop('overwrite'),
            child: const Text('强制覆盖'),
          ),
        ],
      ),
    );
  }

  /// 脏标记上报（未保存保护：tab 关闭前确认用）
  void _setDirty(bool dirty) {
    if (!mounted) return;
    ref.read(wikiDirtyTabsProvider.notifier).update((set) {
      if (dirty) return {...set, widget.page.slug};
      return {...set}..remove(widget.page.slug);
    });
  }

  /// wikilink 点击：slug → 查询目标页 → 打开/激活对应 tab
  Future<void> _openWikiPageFromSlug(String slug) async {
    try {
      final target = await ref.read(wikiPageProvider(slug).future);
      if (!mounted || target == null) return;
      openWikiPageTab(ref, target);
    } catch (_) {
      // 目标页不存在或查询失败：静默忽略，保持当前阅读位置
    }
  }

  String _errText(Object e) => e.toString().replaceFirst('Exception: ', '');

  Widget _buildHeader(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space4,
        AppTheme.space6,
        AppTheme.space4,
      ),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
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
                      widget.page.kindLabel,
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
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusFull,
                        ),
                      ),
                      child: Text(
                        widget.page.slug,
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
                    '证据 ${widget.page.evidenceCount} · 更新 ${_fmtDate(widget.page.updatedAt)}',
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  if (widget.page.sourceUrl != null)
                    _SourceChip(url: widget.page.sourceUrl!),
                ],
              ),
              const SizedBox(height: AppTheme.space4),
              Text(
                widget.page.title,
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
                    widget.page.summary.trim(),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 13,
                      height: 1.6,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
              if (widget.page.tags.contains('work-item')) ...[
                const SizedBox(height: AppTheme.space3),
                _WorkItemPanel(page: widget.page),
              ],
              const SizedBox(height: AppTheme.space3),
              _buildTagRow(context),
              _EntityAliases(slug: widget.page.slug),
              const SizedBox(height: AppTheme.space3),
              _buildRelationsRow(context),
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
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Row(
            children: [
              Icon(
                Icons.track_changes,
                size: 12,
                color: AppTheme.textTertiary,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  '源于 ${widget.page.sourceEventIds.isEmpty ? "尚无事件溯源" : "${widget.page.sourceEventIds.length} 条事件"}',
                  style: TextStyle(
                    fontSize: 11,
                    color: AppTheme.textTertiary,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

/// 标签行：标签 chips + 编辑入口（标签是用户组织知识库的主要元数据）
  Widget _buildTagRow(BuildContext context) {
    return Wrap(
      spacing: 6,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final tag in widget.page.tags)
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
          onTap: () => _editTags(context),
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  widget.page.tags.isEmpty ? Icons.add : Icons.edit_outlined,
                  size: 12,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 4),
                Text(
                  widget.page.tags.isEmpty ? '添加标签' : '编辑标签',
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
  Widget _buildRelationsRow(BuildContext context) {
    final relationsAsync = ref.watch(pageRelationsProvider(widget.page.slug));
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
            for (final r in relations)
              _RelationChip(relation: r, pageSlug: widget.page.slug),
          ],
        ),
      ],
    );
  }

  /// 编辑标签：空格 / 逗号分隔，留空即清空。保存后刷新页面与列表。
  Future<void> _editTags(BuildContext context) async {
    final controller = TextEditingController(text: widget.page.tags.join(' '));
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
            style: FilledButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
            ),
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
      await repo.updateWikiTags(slug: widget.page.slug, tags: tags);
      if (!context.mounted) return;
      ref.invalidate(wikiPageProvider(widget.page.slug));
      ref.invalidate(pageRelationsProvider(widget.page.slug));
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
          content: Text(
            '标签更新失败：${e.toString().replaceFirst('Exception: ', '')}',
          ),
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
    final s = widget.page.summary.trim();
    if (s.isEmpty) return false;
    return widget.page.title.trim() != s;
  }
}

class _WorkItemPanel extends ConsumerWidget {
  final WikiPage page;

  const _WorkItemPanel({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final todos = ref.watch(todosProvider).valueOrNull ?? const <Todo>[];
    Todo? todo;
    for (final item in todos) {
      if (item.relatedWikiSlug == page.slug) {
        todo = item;
        break;
      }
    }
    if (todo == null) return const SizedBox.shrink();
    final workItem = todo;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;

    Future<void> updateFields({String? priority, String? dueAt}) async {
      await repo.updateTodo(
        id: workItem.id,
        title: workItem.title,
        note: workItem.note,
        priority: priority ?? workItem.priority,
        dueAt: dueAt ?? workItem.dueAt,
      );
      ref.invalidate(todosProvider);
    }

    return Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        PopupMenuButton<String>(
          tooltip: '更新状态',
          onSelected: (status) async {
            await repo.updateTodoStatus(workItem.id, status);
            ref.invalidate(todosProvider);
          },
          itemBuilder: (_) => [
            for (final status in TodoStatus.values)
              PopupMenuItem(value: status.wire, child: Text(status.label)),
          ],
          child: _WorkItemChip(
            icon: workItem.isDone
                ? Icons.check_circle
                : Icons.radio_button_unchecked,
            label: workItem.status.label,
            color: workItem.isDone ? AppTheme.success : AppTheme.accentPrimary,
          ),
        ),
        PopupMenuButton<String>(
          tooltip: '更新优先级',
          onSelected: (priority) => updateFields(priority: priority),
          itemBuilder: (_) => const [
            PopupMenuItem(value: 'high', child: Text('高优先级')),
            PopupMenuItem(value: 'normal', child: Text('普通优先级')),
            PopupMenuItem(value: 'low', child: Text('低优先级')),
          ],
          child: _WorkItemChip(
            icon: Icons.flag_outlined,
            label: workItem.priority == 'high'
                ? '高优先级'
                : workItem.priority == 'low'
                    ? '低优先级'
                    : '普通优先级',
            color: AppTheme.textSecondary,
          ),
        ),
        InkWell(
          onTap: () async {
            final now = DateTime.now();
            final selected = await showDatePicker(
              context: context,
              initialDate: workItem.dueAt == null
                  ? now
                  : DateTime.tryParse(workItem.dueAt!) ?? now,
              firstDate: DateTime(now.year - 1),
              lastDate: DateTime(now.year + 5),
              helpText: '选择截止日期',
            );
            if (selected != null) {
              await updateFields(dueAt: selected.toIso8601String());
            }
          },
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: _WorkItemChip(
            icon: Icons.event_outlined,
            label: workItem.dueAt == null
                ? '设置截止日期'
                : '截止 ${_fmtWorkItemDate(workItem.dueAt!)}',
            color: AppTheme.textSecondary,
          ),
        ),
      ],
    );
  }
}

class _WorkItemChip extends StatelessWidget {
  final IconData icon;
  final String label;
  final Color color;

  const _WorkItemChip({
    required this.icon,
    required this.label,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 14, color: color),
          const SizedBox(width: 5),
          Text(label, style: TextStyle(fontSize: 11, color: color)),
        ],
      ),
    );
  }
}

String _fmtWorkItemDate(String value) {
  final date = DateTime.tryParse(value)?.toLocal();
  if (date == null) return value;
  return '${date.year}-${date.month.toString().padLeft(2, '0')}-${date.day.toString().padLeft(2, '0')}';
}

class _EntityAliases extends ConsumerStatefulWidget {
  final String slug;
  const _EntityAliases({required this.slug});
  @override
  ConsumerState<_EntityAliases> createState() => _EntityAliasesState();
}

class _EntityAliasesState extends ConsumerState<_EntityAliases> {
  Future<void> _addAlias() async {
    final controller = TextEditingController();
    final value = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('添加别名'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(hintText: '例如：项目简称、常用称呼'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () =>
                Navigator.pop(dialogContext, controller.text.trim()),
            child: const Text('添加'),
          ),
        ],
      ),
    );
    controller.dispose();
    if (value == null || value.trim().isEmpty || !mounted) return;
    final kind = widget.slug.split('/').first;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.addEntityAlias(kind, widget.slug, value.trim());
    ref.invalidate(entityAliasesProvider(widget.slug));
  }

  @override
  Widget build(BuildContext context) {
    final aliases =
        ref.watch(entityAliasesProvider(widget.slug)).valueOrNull ?? const [];
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Wrap(
        spacing: 6,
        children: [
          if (aliases.isNotEmpty) ...[
            const Icon(Icons.alt_route, size: 14),
            for (final alias in aliases)
              Chip(label: Text(alias, style: const TextStyle(fontSize: 11))),
          ],
          ActionChip(
            avatar: const Icon(Icons.add, size: 14),
            label: const Text('添加别名', style: TextStyle(fontSize: 11)),
            onPressed: _addAlias,
          ),
        ],
      ),
    );
  }
}

class _EntityMergeControls extends ConsumerWidget {
  final WikiPage page;
  const _EntityMergeControls({required this.page});

  bool get _isEntity =>
      const ['person', 'project', 'topic'].contains(page.kind);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!_isEntity) return const SizedBox.shrink();
    final merge = ref.watch(entityMergeStatusProvider(page.slug)).valueOrNull;
    if (merge != null || page.status == 'merged') {
      return Container(
        width: double.infinity,
        margin: const EdgeInsets.only(bottom: AppTheme.space4),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: AppTheme.warning.withValues(alpha: .08),
          border: Border.all(color: AppTheme.warning.withValues(alpha: .3)),
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        ),
        child: Row(
          children: [
            Icon(Icons.merge_type, size: 17, color: AppTheme.warning),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                merge == null ? '此实体已合并' : '此实体已合并到 ${merge.targetSlug}',
                style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
              ),
            ),
            if (merge != null)
              TextButton(
                onPressed: () => _openTarget(ref, merge.targetSlug),
                child: const Text('查看目标'),
              ),
            if (merge != null)
              TextButton(
                onPressed: () => _undo(context, ref, merge.targetSlug),
                child: const Text('撤销合并'),
              ),
          ],
        ),
      );
    }
    return Align(
      alignment: Alignment.centerRight,
      child: TextButton.icon(
        onPressed: () => _merge(context, ref),
        icon: const Icon(Icons.merge_type, size: 15),
        label: const Text('合并实体'),
      ),
    );
  }

  Future<void> _merge(BuildContext context, WidgetRef ref) async {
    final pages = (await ref.read(wikiPagesProvider.future))
        .where(
          (candidate) =>
              candidate.kind == page.kind &&
              candidate.slug != page.slug &&
              candidate.status != 'merged',
        )
        .toList();
    if (!context.mounted) return;
    if (pages.isEmpty) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('没有可合并的同类型实体')));
      return;
    }
    String target = pages.first.slug;
    final selected = await showDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setState) => AlertDialog(
          title: const Text('选择目标实体'),
          content: DropdownButtonFormField<String>(
            initialValue: target,
            isExpanded: true,
            decoration: const InputDecoration(labelText: '合并到'),
            items: [
              for (final candidate in pages)
                DropdownMenuItem(
                  value: candidate.slug,
                  child: Text(
                    '${candidate.title}  ·  ${candidate.slug}',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (value) {
              if (value != null) setState(() => target = value);
            },
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, target),
              child: const Text('继续'),
            ),
          ],
        ),
      ),
    );
    if (selected == null || !context.mounted) return;
    final targetPage = pages.firstWhere(
      (candidate) => candidate.slug == selected,
    );
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('合并到“${targetPage.title}”？'),
        content: const Text(
          '事实、别名和关系会迁移到目标实体；重复内容会合并。原始事件不会改变。只要迁移后的内容没有被修改，就可以从旧实体页撤销。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('确认合并'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.mergeEntity(page.kind, page.slug, selected);
      _refresh(ref, selected);
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('已合并到“${targetPage.title}”，原始事件仍保留')),
        );
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('合并失败：$error')));
      }
    }
  }

  Future<void> _openTarget(WidgetRef ref, String slug) async {
    final target = await ref.read(wikiPageProvider(slug).future);
    if (target != null) openWikiPageTab(ref, target);
  }

  Future<void> _undo(
    BuildContext context,
    WidgetRef ref,
    String targetSlug,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('撤销这次合并？'),
        content: const Text(
          '只恢复本次合并迁移的事实、别名和关系。若这些内容合并后已被修改，系统会拒绝操作以保护新数据。原始事件不会改变。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('撤销合并'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.undoEntityMerge(page.slug);
      _refresh(ref, targetSlug);
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text('合并已撤销，实体内容已恢复')));
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('无法安全撤销：$error')));
      }
    }
  }

  void _refresh(WidgetRef ref, String targetSlug) {
    for (final slug in [page.slug, targetSlug]) {
      ref.invalidate(wikiPageProvider(slug));
      ref.invalidate(entityFactsProvider(slug));
      ref.invalidate(entityAliasesProvider(slug));
      ref.invalidate(pageRelationsProvider(slug));
      ref.invalidate(entityMergeStatusProvider(slug));
    }
    ref.invalidate(wikiPagesProvider);
  }
}

class _EntityRelatedTodos extends ConsumerWidget {
  final WikiPage page;
  const _EntityRelatedTodos({required this.page});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!const ['person', 'project', 'topic'].contains(page.kind)) {
      return const SizedBox.shrink();
    }
    final todos = (ref.watch(todosProvider).valueOrNull ?? const <Todo>[])
        .where((todo) => todo.relatedWikiSlug == page.slug)
        .toList();
    if (todos.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '关联待办',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textTertiary,
            ),
          ),
          const SizedBox(height: 6),
          for (final todo in todos)
            CheckboxListTile(
              value: todo.status == TodoStatus.done,
              dense: true,
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              title: Text(
                todo.title,
                style: TextStyle(
                  fontSize: 12,
                  decoration: todo.status == TodoStatus.done
                      ? TextDecoration.lineThrough
                      : null,
                  color: todo.status == TodoStatus.done
                      ? AppTheme.textTertiary
                      : AppTheme.textSecondary,
                ),
              ),
              subtitle: todo.dueAt == null
                  ? null
                  : Text(
                      '截止 ${todo.dueAt}',
                      style: TextStyle(
                        fontSize: 10.5,
                        color: AppTheme.textTertiary,
                      ),
                      ),
              secondary: IconButton(
                tooltip: '打开工作项',
                icon: Icon(
                  Icons.open_in_new,
                  size: 16,
                  color: AppTheme.accentPrimary,
                ),
                onPressed: () async {
                  final repo = ref.read(storageRepositoryProvider)
                      as RustBridgeRepository;
                  final workItem = await repo.openTodoWorkItem(todo.id);
                  if (!context.mounted) return;
                  openWikiPageTab(ref, workItem);
                },
              ),
              onChanged: todo.status == TodoStatus.archived
                  ? null
                  : (_) => _toggle(ref, todo),
            ),
        ],
      ),
    );
  }

  Future<void> _toggle(WidgetRef ref, Todo todo) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.updateTodoStatus(
      todo.id,
      todo.status == TodoStatus.done ? 'open' : 'done',
    );
    ref.invalidate(todosProvider);
  }
}

class _EntityFacts extends ConsumerWidget {
  final String slug;
  const _EntityFacts({required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final facts = ref.watch(entityFactsProvider(slug)).valueOrNull ?? const [];
    if (facts.isEmpty) return const SizedBox.shrink();
    final conflicts = _conflictingFactIds(facts);
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '结构化事实',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textTertiary,
            ),
          ),
          if (conflicts.isNotEmpty) ...[
            const SizedBox(height: 8),
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: AppTheme.warning.withValues(alpha: .10),
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.warning.withValues(alpha: .35)),
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    Icons.warning_amber_rounded,
                    size: 16,
                    color: AppTheme.warning,
                  ),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      '发现可能冲突的事实。历史来源均已保留，请查看来源后移除错误的派生事实。',
                      style: TextStyle(
                        fontSize: 11.5,
                        height: 1.35,
                        color: AppTheme.textSecondary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
          const SizedBox(height: 8),
          for (final fact in facts.take(8))
            Container(
              margin: const EdgeInsets.only(bottom: 8),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                border: Border.all(color: AppTheme.surface3),
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    conflicts.contains(fact.id)
                        ? Icons.warning_amber_rounded
                        : Icons.fact_check_outlined,
                    size: 14,
                    color: conflicts.contains(fact.id)
                        ? AppTheme.warning
                        : AppTheme.accentPrimary,
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          fact.factText,
                          style: TextStyle(
                            fontSize: 12,
                            height: 1.4,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                        const SizedBox(height: 4),
                        InkWell(
                          onTap: () =>
                              _showSource(context, ref, fact.sourceEventId),
                          child: Text(
                            '${_factDate(fact.occurredAt)} · 置信度 ${fact.confidence}/5 · 查看来源 ${_shortId(fact.sourceEventId)}',
                            style: TextStyle(
                              fontSize: 10.5,
                              color: AppTheme.accentPrimary,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: '纠正：移除此事实',
                    icon: Icon(
                      Icons.close,
                      size: 15,
                      color: AppTheme.textTertiary,
                    ),
                    onPressed: () => _deleteFact(context, ref, fact.id),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }

  Set<String> _conflictingFactIds(List<EntityFactDto> facts) {
    final groups = <String, List<EntityFactDto>>{};
    for (final fact in facts) {
      final separator = fact.factText.indexOf('：');
      final key = separator > 0
          ? fact.factText.substring(0, separator).trim()
          : fact.factText.trim();
      groups.putIfAbsent(key, () => []).add(fact);
    }
    return {
      for (final group in groups.values)
        if (group.map((fact) => fact.factText).toSet().length > 1)
          ...group.map((fact) => fact.id),
    };
  }

  String _factDate(String value) {
    final parsed = DateTime.tryParse(value)?.toLocal();
    if (parsed == null) return value;
    return '${parsed.year}-${parsed.month.toString().padLeft(2, '0')}-${parsed.day.toString().padLeft(2, '0')}';
  }

  String _shortId(String value) =>
      value.length > 8 ? value.substring(0, 8) : value;

  Future<void> _deleteFact(
    BuildContext context,
    WidgetRef ref,
    String id,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('移除这条事实？'),
        content: const Text('只会移除知识页中的派生事实，不会删除原始事件。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('移除'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.deleteEntityFact(id);
    ref.invalidate(entityFactsProvider(slug));
    if (context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('事实已移除，原始事件仍保留')));
    }
  }

  Future<void> _showSource(
    BuildContext context,
    WidgetRef ref,
    String eventId,
  ) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final detail = await repo.getEventAnalysisDetail(eventId);
    if (!context.mounted) return;
    if (detail == null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('来源事件不存在')));
      return;
    }
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('来源事件'),
        content: SizedBox(
          width: 520,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SelectableText(detail.rawText),
                if ((detail.summary ?? '').isNotEmpty) ...[
                  const SizedBox(height: 16),
                  Text(
                    'AI 摘要',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  const SizedBox(height: 6),
                  SelectableText(detail.summary!),
                ],
                const SizedBox(height: 16),
                Text(
                  '记录于 ${_factDate(detail.recordedAt)} · 分析状态 ${detail.jobStatus}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('关闭'),
          ),
        ],
      ),
    );
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
                style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
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
      final page = await repo.saveImportedPage(
        title: widget.fetch.displayTitle,
        contentMd: widget.fetch.contentMd,
        sourceUrl: widget.fetch.sourceUrl,
        sourceKind: widget.fetch.sourceKind,
        tags: _parsedTags(),
      );
      final inputRecordId = widget.fetch.inputRecordId;
      if (inputRecordId != null) {
        await repo.finishUrlInput(inputRecordId, wikiPageSlug: page.slug);
      }
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
      final inputRecordId = widget.fetch.inputRecordId;
      if (inputRecordId != null) {
        await repo.finishUrlInput(inputRecordId, wikiPageSlug: page.slug);
      }
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
              for (final msg in _chat) WikiChatBubble(message: msg),
              if (_chatBusy) const WikiChatBubble.pending(),
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
    final author =
        fetch.authorName ??
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
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
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
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
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
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.all(
                    Radius.circular(AppTheme.radiusMedium),
                  ),
                  borderSide: BorderSide(
                    color: AppTheme.accentPrimary,
                    width: 1.5,
                  ),
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
