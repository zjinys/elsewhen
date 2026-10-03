import 'knowledge_workflows.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';

import '../bridge/rust_bridge_repository.dart';
import '../bridge/api.dart' show EntityFactDto;
import '../utils/error_report.dart';
import '../models/relation.dart';
import '../models/tweet_fetch.dart';
import '../models/import_fetch.dart';
import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';
import '../providers/settings_provider.dart';
import '../providers/state_holder.dart';
import '../providers/todo_provider.dart';
import '../models/todo.dart';
import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import '../wiki/wiki_content_editor.dart';
import 'wiki_reading_settings_dialog.dart';
import 'wiki_ai_chat_panel.dart';
import 'wiki_derivatives.dart';
import 'markdown_view.dart';
import 'knowledge_panel.dart';
import 'knowledge_import.dart';

part 'wiki_detail_import_section.dart';
part 'wiki_detail_entity_sections.dart';
part 'wiki_detail_page_chips.dart';
part 'wiki_detail_import_preview.dart';

/// 右侧知识库面板：多 tab。
/// - 固定 Tab 1：导入（粘贴链接抓取或直接文本保存）
/// - 抓取成功后新开「预览」tab：内容 + 保存按钮（点保存才入库）
/// - 从左侧点击知识库页时新开「页面详情」tab（含 AI 处理面板）
class WikiPageDetailView extends ConsumerStatefulWidget {
  const WikiPageDetailView({super.key});

  @override
  ConsumerState<WikiPageDetailView> createState() => _WikiPageDetailViewState();
}

/// 标签编辑对话框。
///
/// 控制器归本 State 所有，不在调用方 `await showDialog` 之后 dispose：
/// `showDialog` 在 pop 那一刻返回，退出动画还要再跑约 200ms，其间 widget 树仍挂载。
/// 本对话框带 `autofocus`，聚焦变化几乎必然在动画期间触发重建，打到已释放的
/// controller 上会抛 "A TextEditingController was used after being disposed"。
class _EditTagsDialog extends StatefulWidget {
  const _EditTagsDialog({required this.initial});

  final String initial;

  @override
  State<_EditTagsDialog> createState() => _EditTagsDialogState();
}

class _EditTagsDialogState extends State<_EditTagsDialog> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initial);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
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
            controller: _controller,
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
            onSubmitted: (value) => Navigator.of(context).pop(value),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
        ),
        FilledButton(
          style: FilledButton.styleFrom(backgroundColor: AppTheme.accentPrimary),
          onPressed: () => Navigator.of(context).pop(_controller.text),
          child: const Text('保存'),
        ),
      ],
    );
  }
}

class _WikiPageDetailViewState extends ConsumerState<WikiPageDetailView> {
  @override
  Widget build(BuildContext context) {
    final tabs = ref.watch(wikiOpenTabsProvider);
    final activeId = ref.watch(wikiActiveTabIdProvider);
    final active = tabs.firstWhere(
      (t) => t.id == activeId,
      orElse: () => tabs.first,
    );

    return Column(
      children: [
        _WikiTabBar(tabs: tabs, activeId: active.id),
        const Align(
          alignment: Alignment.centerRight,
          child: KnowledgeMaintenanceButton(),
        ),
        Expanded(
          // IndexedStack：所有 tab 的子树常驻，切 tab 不销毁详情页编辑器状态
          // （未保存的编辑切走再切回仍在；§6.3 未保存保护的前提）
          child: IndexedStack(
            index: tabs.indexWhere((t) => t.id == active.id),
            children: [
              for (final tab in tabs)
                switch (tab) {
                  ImportTabEntry() => const _HomeTab(),
                  PageTabEntry(:final slug) => _PageTabBody(slug: slug),
                  TweetTabEntry(:final fetch) => _TweetTabBody(fetch: fetch),
                  ImportFetchTabEntry(:final fetch) => _ImportFetchTabBody(
                    fetch: fetch,
                  ),
                },
            ],
          ),
        ),
      ],
    );
  }
}

/// 知识库首页：上方导入，下方浏览。二者属于同一工作区，不再拆成两个 tab。
class _HomeTab extends StatelessWidget {
  const _HomeTab();

  @override
  Widget build(BuildContext context) {
    final narrow = MediaQuery.sizeOf(context).width < 560;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 14, 16, 12),
          child: narrow
              ? Column(
                  children: [
                    _HomeAction(
                      icon: Icons.link_outlined,
                      title: '网址导入',
                      subtitle: '抓取网页或推文，确认后保存',
                      onTap: () => _showImportDialog(context, urlMode: true),
                    ),
                    const SizedBox(height: 8),
                    _HomeAction(
                      icon: Icons.notes_outlined,
                      title: '直接文本',
                      subtitle: '粘贴内容并创建知识页',
                      onTap: () => _showImportDialog(context, urlMode: false),
                    ),
                  ],
                )
              : Row(
                  children: [
                    Expanded(
                      child: _HomeAction(
                        icon: Icons.link_outlined,
                        title: '网址导入',
                        subtitle: '抓取网页或推文，确认后保存',
                        onTap: () => _showImportDialog(context, urlMode: true),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: _HomeAction(
                        icon: Icons.notes_outlined,
                        title: '直接文本',
                        subtitle: '粘贴内容并创建知识页',
                        onTap: () => _showImportDialog(context, urlMode: false),
                      ),
                    ),
                  ],
                ),
        ),
        Divider(height: 1, color: AppTheme.surface3),
        const Expanded(child: _WikiBrowseTab()),
      ],
    );
  }

  Future<void> _showImportDialog(
    BuildContext context, {
    required bool urlMode,
  }) async {
    await showDialog<void>(
      context: context,
      builder: (_) => Dialog(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 720, maxHeight: 680),
          child: Padding(
            padding: const EdgeInsets.all(4),
            child: _ImportTab(
              initialUrlMode: urlMode,
              onCompleted: () => Navigator.of(context).pop(),
            ),
          ),
        ),
      ),
    );
  }
}

class _HomeAction extends StatelessWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;

  const _HomeAction({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) => Material(
    color: AppTheme.surface1,
    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
    child: InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Row(
          children: [
            Icon(icon, size: 19, color: AppTheme.accentPrimary),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    subtitle,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ),
            ),
            Icon(Icons.chevron_right, size: 18, color: AppTheme.textTertiary),
          ],
        ),
      ),
    ),
  );
}

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
              ref.read(wikiActiveTabIdProvider.notifier).set(tab.id);
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
    final dirty =
        slug != null && ref.read(wikiDirtyTabsProvider).contains(slug);
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
    ref
        .read(wikiDirtyTabsProvider.notifier)
        .update((set) => {...set}..remove(slug));
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
        return _WikiPageBody(key: ValueKey(page.slug), page: page);
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
const double _kReadingMaxWidth = 1160;

/// 编辑冲突对话框中用户选择「取消/重新加载」时中止保存的内部信号：
/// 让 [WikiContentEditor.save] 返回 false（留在/退出编辑态由调用处控制），
/// 但不触发错误条（不是失败）。
class _SaveCancelled implements Exception {
  const _SaveCancelled();
}

enum _WikiPageSection {
  content('内容'),
  outputs('产出'),
  relations('关联'),
  sources('来源与修订');

  const _WikiPageSection(this.label);
  final String label;
}

class _WikiPageBody extends ConsumerStatefulWidget {
  final WikiPage page;

  const _WikiPageBody({super.key, required this.page});

  @override
  ConsumerState<_WikiPageBody> createState() => _WikiPageBodyState();
}

class _WikiPageBodyState extends ConsumerState<_WikiPageBody>
    with SingleTickerProviderStateMixin {
  late final TabController _sectionController;
  void _showSection(_WikiPageSection section) {
    _sectionController.index = section.index;
    setState(() => _section = section);
  }

  final _editorKey = GlobalKey<WikiContentEditorState>();

  _WikiPageSection _section = _WikiPageSection.content;

  /// 可编辑判定：素材页（source/note）只读，与 Rust 侧 `save_wiki_page_content`
  /// 保护一致（M1 素材保护）；人员/项目等 AI 档案页可人工修改。
  bool get _canEdit {
    final kind = widget.page.kind;
    return kind != 'source' && kind != 'note';
  }

  bool _editing = false;
  bool _saving = false;
  String? _editError;
  bool _chatOpen = false;
  bool _chatInitialized = false;
  Offset? _chatPosition;

  /// 保存回调注册表的 notifier 引用：dispose 后 `ref` 不可用，须提前缓存。
  late final StateHolder<Map<String, Future<bool> Function()>> _saveCallbacks;

  @override
  void initState() {
    super.initState();
    _sectionController = TabController(
      length: _WikiPageSection.values.length,
      vsync: this,
    );
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
    _sectionController.dispose();
    final notifier = _saveCallbacks;
    final slug = widget.page.slug;
    scheduleMicrotask(() {
      // provider 已随容器销毁（如测试 teardown）则无需注销；
      // Riverpod 3 下销毁后写 state 抛 UnmountedRefException（内部类型），
      // 用 mounted 守卫而不是捕获。
      if (!notifier.isMounted) return;
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
    return DefaultTabController(
      length: _WikiPageSection.values.length,
      initialIndex: _section.index,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildHeader(context),
          if (_editError != null) _buildEditErrorBar(),
          _buildSectionBar(),

          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final width = math.min(
                  720.0,
                  math.max(0.0, constraints.maxWidth - 24),
                );
                final height = math.min(
                  760.0,
                  math.max(0.0, constraints.maxHeight - 24),
                );
                final maxX = math.max(0.0, constraints.maxWidth - width);
                final maxY = math.max(0.0, constraints.maxHeight - height);
                final x = _chatPosition == null
                    ? maxX - 12
                    : _chatPosition!.dx.clamp(0.0, maxX);
                final y = _chatPosition == null
                    ? maxY - 12
                    : _chatPosition!.dy.clamp(0.0, maxY);
                return Stack(
                  children: [
                    Positioned.fill(child: _buildSectionContent()),
                    if (!_chatOpen)
                      Positioned(
                        right: 16,
                        bottom: 16,
                        child: FloatingActionButton.small(
                          tooltip: '和 AI 讨论此页',
                          onPressed: () => setState(() {
                            _chatOpen = true;
                            _chatInitialized = true;
                          }),
                          child: const Icon(Icons.auto_awesome_outlined),
                        ),
                      ),
                    if (_chatInitialized)
                      Positioned(
                        left: x,
                        top: y,
                        width: width,
                        height: height,
                        child: Offstage(
                          offstage: !_chatOpen,
                          child: Material(
                            elevation: 12,
                            borderRadius: BorderRadius.circular(
                              AppTheme.radiusMedium,
                            ),
                            clipBehavior: Clip.antiAlias,
                            child: WikiAiChatPanel(
                              slug: widget.page.slug,
                              onMinimize: () =>
                                  setState(() => _chatOpen = false),
                              onDragUpdate: (delta) => setState(() {
                                _chatPosition = Offset(
                                  (x + delta.dx).clamp(0.0, maxX),
                                  (y + delta.dy).clamp(0.0, maxY),
                                );
                              }),
                            ),
                          ),
                        ),
                      ),
                  ],
                );
              },
            ),
          ),

          _buildFooter(),
        ],
      ),
    );
  }

  Widget _buildSectionBar() {
    return Container(
      height: 48,
      padding: const EdgeInsets.symmetric(horizontal: AppTheme.space6),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: AppTheme.surface3)),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // 窄视口（<480 内宽，对应 ~528 以下窗口）下编辑操作收成紧凑态，
          // 避免「取消/完成」按钮把 Tab 栏挤到溢出。
          final compact = constraints.maxWidth < 480;
          return Row(
            // 不自适应时 TabBar 用自然宽度（isScrollable），宽度随内容；
            // spaceBetween 让 Tab 组贴左、编辑操作居中、聊天贴右。
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                child: TabBar(
                  controller: _sectionController,
                  isScrollable: true,
                  tabAlignment: TabAlignment.start,
                  labelColor: AppTheme.textPrimary,
                  unselectedLabelColor: AppTheme.textTertiary,
                  indicatorColor: AppTheme.accentPrimary,
                  indicatorWeight: 2,
                  labelStyle: const TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                  ),
                  unselectedLabelStyle: const TextStyle(fontSize: 13),
                  dividerColor: Colors.transparent,
                  onTap: (index) => setState(() {
                    _section = _WikiPageSection.values[index];
                  }),
                  tabs: [
                    for (final section in _WikiPageSection.values)
                      Tab(
                        text: section == _WikiPageSection.content && !_canEdit
                            ? '原文'
                            : section.label,
                      ),
                  ],
                ),
              ),
              if (_section == _WikiPageSection.content) ...[
                // 右侧操作组：「正文阅读设置」永远贴最右，编辑操作排在它左
                // 侧——编辑按钮（编辑正文/取消/完成）显隐不影响阅读按钮位置。
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (_canEdit) _buildInlineEditActions(compact: compact),
                    // 正文阅读参数（AA 浮层：字体/字号/行距，两层覆盖模型）。
                    // 宽屏与「编辑正文」统一为 tonal 标签按钮（同 32 高、同
                    // padding、同图标尺寸）；窄视口（compact）与编辑操作一致
                    // 收成图标按钮，避免手机宽度下 section bar 溢出。
                    if (compact)
                      IconButton(
                        tooltip: '正文阅读设置（字体 / 字号 / 行距）',
                        visualDensity: VisualDensity.compact,
                        onPressed: () => showWikiReadingSettings(context),
                        icon: const Icon(Icons.format_size, size: 19),
                        color: AppTheme.textSecondary,
                      )
                    else
                      Tooltip(
                        message: '正文阅读设置（字体 / 字号 / 行距）',
                        child: FilledButton.tonalIcon(
                          onPressed: () => showWikiReadingSettings(context),
                          icon: const Icon(Icons.format_size, size: 15),
                          label: const Text('阅读设置'),
                          style: FilledButton.styleFrom(
                            minimumSize: const Size(0, 32),
                            padding: const EdgeInsets.symmetric(horizontal: 10),
                          ),
                        ),
                      ),
                  ],
                ),
              ],
            ],
          );
        },
      ),
    );
  }

  Widget _buildSectionContent() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space3,
        AppTheme.space6,
        0,
      ),
      child: Align(
        alignment: Alignment.topLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: switch (_section) {
            _WikiPageSection.content => WikiContentEditor(
              key: _editorKey,
              slug: widget.page.slug,
              contentMd: widget.page.contentMd,
              editable: _editing,
              onWikiLinkTap: _openWikiPageFromSlug,
              onSave: _persistEdit,
              onSaveError: (e) {
                if (!mounted || e is _SaveCancelled) return;
                setState(() => _editError = _errText(e));
              },
              onDirtyChanged: _setDirty,
              // 两层覆盖模型：编辑器覆盖 ?? 全局内容字体（system = 跟随系统字体）
              fontFamily: ref.watch(
                settingsProvider.select((s) => s.contentFontName),
              ),
              fontSize: ref.watch(
                settingsProvider.select((s) => s.contentFontSize),
              ),
              lineHeight: ref.watch(
                settingsProvider.select((s) => s.contentLineHeight),
              ),
            ),
            _WikiPageSection.relations => SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (widget.page.basedOn != null)
                    WikiSourceLink(slug: widget.page.basedOn!),
                  KnowledgeOriginLinks(slug: widget.page.slug),
                  _EntityMergeControls(page: widget.page),
                  _EntityFacts(slug: widget.page.slug),
                  _EntityRelatedTodos(page: widget.page),
                ],
              ),
            ),
            _WikiPageSection.outputs => KnowledgeOutputsSection(
              page: widget.page,
            ),
            _WikiPageSection.sources => KnowledgeSourcesSection(
              page: widget.page,
            ),
          },
        ),
      ),
    );
  }

  Widget _buildInlineEditActions({bool compact = false}) {
    if (_editing) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (compact)
            IconButton(
              tooltip: '取消',
              visualDensity: VisualDensity.compact,
              onPressed: _saving ? null : _cancelEditing,
              icon: const Icon(Icons.close, size: 18),
            )
          else ...[
            TextButton(
              onPressed: _saving ? null : _cancelEditing,
              child: const Text('取消'),
            ),
            const SizedBox(width: 2),
          ],
          FilledButton(
            onPressed: _saving ? null : _finishEditing,
            style: FilledButton.styleFrom(
              minimumSize: const Size(58, 32),
              padding: EdgeInsets.symmetric(horizontal: compact ? 8 : 12),
            ),
            child: _saving
                ? const SizedBox(
                    width: 13,
                    height: 13,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('完成'),
          ),
        ],
      );
    }
    final editButton = FilledButton.tonalIcon(
      onPressed: _enterEditing,
      icon: const Icon(Icons.edit_outlined, size: 15),
      label: const Text('编辑正文'),
      style: FilledButton.styleFrom(
        minimumSize: const Size(0, 32),
        padding: const EdgeInsets.symmetric(horizontal: 10),
      ),
    );
    return editButton;
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
  static bool _isConflictError(Object e) => e.toString().contains('编辑冲突');

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
        AppTheme.space3,
        AppTheme.space6,
        AppTheme.space2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.72),
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Align(
        alignment: Alignment.centerLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // 紧凑主行：kind 徽章 + 标题 + 元数据同一流动行（窄屏自然换行）。
              // 布局原则：头部只保留「这页是什么」，把纵向空间还给正文。
              Wrap(
                spacing: AppTheme.space2,
                runSpacing: 4,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 2,
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
                    constraints: const BoxConstraints(maxWidth: 980),
                    child: _buildHeaderTitle(context),
                  ),
                  Text(
                    '${widget.page.slug} · 采集 ${_fmtDate(widget.page.lastSeenAt)}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  if (widget.page.sourceUrl != null)
                    if (widget.page.isLocalPath)
                      _LocalPathChip(page: widget.page)
                    else
                      _SourceChip(url: widget.page.sourceUrl!),
                ],
              ),
              if (widget.page.tags.contains('work-item')) ...[
                const SizedBox(height: AppTheme.space2),
                _WorkItemPanel(page: widget.page),
              ],
              // 联系人关系是内容级信息，保持直接可见（空时自隐藏）
              _buildRelationsRow(context),
              // 标签直接展开，避免用户还要打开一个元数据折叠行。
              _buildTagRow(context),
              KnowledgePageActions(
                page: widget.page,
                onShowSources: () => _showSection(_WikiPageSection.sources),
                onShowOutputs: () => _showSection(_WikiPageSection.outputs),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// 知识页标题：单行保留，过长省略并附 tooltip 显示完整标题；末尾挂一个复制按钮。
  ///
  /// 复制按钮放进标题自己的 Row 里（而不是当作头部 Wrap 的另一个子节点），是为了
  /// 让它始终紧贴标题末尾：短标题时贴在文字后面，长标题省略号也贴在省略号后面。
  /// 若作为 Wrap 的独立子节点，窄屏换行后它会变成一个孤零零的图标，混进下面的
  /// slug 元数据里分不清是干什么的。标题用 Flexible，复制按钮定宽，因此按钮永远
  /// 不被挤掉，也不会改变头部 Wrap 的换行行为。
  Widget _buildHeaderTitle(BuildContext context) {
    final title = widget.page.title;
    final style = TextStyle(
      fontSize: 23,
      fontWeight: FontWeight.w600,
      color: AppTheme.textPrimary,
      height: 1.25,
    );
    final text = Text(
      title,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: style,
    );

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final painter = TextPainter(
                text: TextSpan(text: title, style: style),
                maxLines: 1,
                textDirection: TextDirection.ltr,
              )..layout(maxWidth: constraints.maxWidth);
              // 标题在约束内能完整排入一行就不包 tooltip；溢出（省略号出现）才 hover 展示全文。
              if (!painter.didExceedMaxLines) return text;
              return Tooltip(message: title, child: text);
            },
          ),
        ),
        _copyTitleButton(context),
      ],
    );
  }

  /// 复制标题：与来源链接 chip 同一套反馈（floating SnackBar），文案带「标题」
  /// 以区分页面上其他可复制的东西（路径、链接、正文、代码块）。
  Widget _copyTitleButton(BuildContext context) {
    return Tooltip(
      message: '复制标题',
      child: InkWell(
        onTap: () {
          Clipboard.setData(ClipboardData(text: widget.page.title));
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('标题已复制'),
              behavior: SnackBarBehavior.floating,
            ),
          );
        },
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        // 标题是 23px/1.25 的行，图标需要额外留出可点区域才不会显得难点
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          child: Icon(Icons.copy, size: 13, color: AppTheme.textTertiary),
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
        color: AppTheme.surface1.withValues(alpha: 0.55),
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Row(
            children: [
              Icon(Icons.track_changes, size: 12, color: AppTheme.textTertiary),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  '源于 ${widget.page.sourceEventIds.isEmpty ? "尚无事件溯源" : "${widget.page.sourceEventIds.length} 条事件"}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
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

  /// 联系人关系区块：AI 从对话识别、用户确认后保存的「联系人 ↔ 事情/项目」。
  /// 双侧方向都以当前页为中心展示（出→ 人·事；入← 人·事）。
  Widget _buildRelationsRow(BuildContext context) {
    final relationsAsync = ref.watch(pageRelationsProvider(widget.page.slug));
    final relations = relationsAsync.value ?? const [];
    if (relations.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '联系人关系',
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
    final submitted = await showDialog<String>(
      context: context,
      builder: (_) => _EditTagsDialog(initial: widget.page.tags.join(' ')),
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

class _WikiBrowseTab extends StatelessWidget {
  const _WikiBrowseTab();
  @override
  Widget build(BuildContext context) => const KnowledgeLibrary();
}

/// 内容对话的气泡（用户右对齐高亮，AI 左侧带头像）
