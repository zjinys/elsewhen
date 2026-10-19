import 'knowledge_workflows.dart';

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:file_picker/file_picker.dart';

import '../bridge/rust_bridge_repository.dart';
import '../bridge/api.dart' show EntityFactDto, KnowledgeRelationDto, PageGuidanceDto;
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
import '../providers/knowledge_provider.dart';

part 'wiki_detail_import_section.dart';
part 'wiki_page_parts/wiki_detail_entity_items.dart';
part 'wiki_page_parts/wiki_detail_entity_guidance.dart';
part 'wiki_page_parts/wiki_detail_entity_facts.dart';
part 'wiki_detail_page_chips.dart';
part 'wiki_detail_import_preview.dart';
// ── 详情视图主体拆分（行为零变化，见 wiki_page_parts/）──
part 'wiki_page_parts/wiki_detail_home_tab.dart';
part 'wiki_page_parts/wiki_detail_body_immersive.dart';
part 'wiki_page_parts/wiki_detail_body_edit.dart';
part 'wiki_page_parts/wiki_detail_browse.dart';

/// 右侧知识库面板：多 tab。
/// - 固定 Tab 1：导入（粘贴链接抓取或文本导入保存）
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
          style: FilledButton.styleFrom(
            backgroundColor: AppTheme.accentPrimary,
          ),
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

/// 打开知识库导入对话框（文本 / 网址模式）。
/// 供首页 _HomeAction 与移动端标题栏 _WikiTitleBarActions 共用。
Future<void> openWikiImportDialog(
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

/// 知识库首页：上方导入，下方浏览。二者属于同一工作区，不再拆成两个 tab。
class _PageTabBody extends ConsumerWidget {
  final String slug;

  const _PageTabBody({required this.slug});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pageAsync = ref.watch(wikiPageProvider(slug));
    // 知识号 bump（分析/消化/对端同步产出）会让本 provider reload；reload 必须
    // 沿用旧页面数据原地重建，否则整页闪 loading、页面内挂载的 AI 对话框等
    // 有状态浮层会被连根卸载（表现为"AI 回复一到，聊天窗自动关闭"）。
    return pageAsync.when(
      skipLoadingOnReload: true,
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

/// 沉浸阅读专用宽度：比正常阅读栏更窄，接近纸质书行宽，减少眼球横向移动。
const double _kImmersiveMaxWidth = 780;

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

  /// 沉浸阅读：插到 app Overlay 的全屏覆盖层，盖住一切 chrome（含左侧导航 / 顶栏）。
  /// 非空 = 沉浸中。用 OverlayEntry 而非内部 Stack，才能盖住本 widget 之外的导航栏。
  OverlayEntry? _immersiveEntry;

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
    // 若沉浸覆盖层还挂着（如沉浸中切走了页面），先移除防泄漏。
    _immersiveEntry?.remove();
    _immersiveEntry = null;
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
    // 沉浸阅读不再是本 widget 内部的叠层，而是 app Overlay 里的全屏 entry（见
    // _showImmersive）。这里只画正常 body；覆盖层由 Overlay 管理，故 build 不感知。
    return _buildNormalBody(context);
  }
}
