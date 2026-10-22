import 'dart:async';

import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../models/settings.dart';
import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import '../utils/system_fonts.dart';
import 'wiki_code_block.dart';
import 'wiki_markdown_codec.dart';
import 'wiki_table_block.dart';
import 'wiki_text_span_decorator.dart';

/// 正文编辑器组件（M3 §6 / §7 集成）。
///
/// - 浏览 / 编辑双模式：外层切 [editable] 即可（同一 [EditorState] 实例）；
/// - 文档为「正文 markdown ⇄ AppFlowy 文档」生产管线解码结果；
/// - [save]：dirty（当前文档 vs 加载快照）才有真实写库，经 [onSave] 交给外层；
/// - `Ctrl/Cmd+S` 保存（HardwareKeyboard 全局监听，编辑态内任意焦点可用）；
/// - wikilink 点击经 [onWikiLinkTap] 交给外层跳转。
class WikiContentEditor extends StatefulWidget {
  const WikiContentEditor({
    super.key,
    required this.slug,
    required this.contentMd,
    required this.editable,
    required this.onWikiLinkTap,
    required this.onSave,
    this.onSaveError,
    this.onDirtyChanged,
    this.shrinkWrap = false,
    this.fontSize = AppFonts.defaultFontSize,
    this.textColor,
    this.fontFamily,
    this.lineHeight,
    this.enableSaveShortcut = true,
  });

  /// 页面 slug：用于在页面切换时重建对应正文。
  final String slug;

  /// 加载时的正文 markdown（脏检查快照基准；变更即视为重新加载）
  final String contentMd;

  /// 是否可编辑（false = 只读浏览）
  final bool editable;

  /// wikilink 点击回调（参数为链接目标 slug）
  final void Function(String slug) onWikiLinkTap;

  /// 保存回调：由外层完成 `saveWikiPageContent` + 刷新 + 退出编辑态。
  /// 抛异常视为保存失败，调用方呈现错误并留在编辑态。
  final Future<void> Function(String markdown) onSave;

  /// 保存失败兜底（快捷键路径等无返回值场景）
  final void Function(Object error)? onSaveError;

  /// 脏标记变化（true=有未保存改动；用于 tab 关闭前确认）
  final void Function(bool dirty)? onDirtyChanged;

  /// 是否由外层滚动容器承载（独立滚动场景传 false，即编辑器自带滚动）
  final bool shrinkWrap;

  /// 正文字号（px）：跟随「外观 → 正文字号」，仅作用于正文
  final double fontSize;

  /// 正文颜色覆盖（null = 跟随主题正文色）。沉浸阅读传 softened 色降低对比度疲劳。
  final Color? textColor;

  /// 正文字体存值（知识页详情页传 [AppSettings.contentFontName]，即
  /// 编辑器覆盖 ?? 全局内容字体）：null = 回退 [ContentFont] 作用域；
  /// [AppFonts.system] = 跟随系统字体（不注入家族）。
  final String? fontFamily;

  /// 编辑器（内容区）行距覆盖（倍数）：null = 跟随全局（vendor 默认 1.5）。
  /// 由知识页详情页从「AA」浮层写入。
  final double? lineHeight;

  /// 是否监听 `Ctrl/Cmd+S` 全局快捷键（dialog 场景传 false，避免
  /// HardwareKeyboard 全局监听干扰输入焦点）。
  final bool enableSaveShortcut;

  @override
  State<WikiContentEditor> createState() => WikiContentEditorState();
}

class WikiContentEditorState extends State<WikiContentEditor>
    with WidgetsBindingObserver {
  late EditorState _editorState;
  late String _loadedMd;
  StreamSubscription<EditorTransactionValue>? _txSub;
  bool _saving = false;

  /// 当前文档与加载快照是否不同（有未保存改动）。
  /// 以 markdown 编码串为比较基准：`Document.toJson()` 两次调用无法稳定
  /// 相等（内部 HashMap 迭代序不确定），而编码器输出是确定性的。
  bool get isDirty =>
      wikiDocumentToMarkdown(_editorState.document) != _loadedMd;

  /// 测试/外部访问：当前编辑器状态
  EditorState get editorState => _editorState;

  /// 最后一次选区：窗口失活时 primary focus 可能已被 GTK 提前丢弃（vendor 在
  /// 键盘服务失焦时清空选区），这里持续跟踪，供焦点恢复取用。
  Selection? _lastSelection;

  /// selectionNotifier 当前绑定的 EditorState（_initEditor 重建时换绑用）。
  EditorState? _boundSelectionState;

  /// 跟踪 vendor 内部节点的 selectionNotifier 绑定（_initEditor 会重建
  /// [_editorState]，监听须随之换绑）。
  void _bindSelectionNotifier() {
    _boundSelectionState?.selectionNotifier.removeListener(_onSelectionChanged);
    _boundSelectionState = _editorState;
    _editorState.selectionNotifier.addListener(_onSelectionChanged);
  }

  void _onSelectionChanged() {
    final selection = _editorState.selection;
    if (selection != null) _lastSelection = selection;
  }

  /// 测试观测：最后一次选区是否被跟踪。
  @visibleForTesting
  bool get debugHasTrackedSelection => _lastSelection != null;

  /// 测试观测：当前焦点恢复认领者是否为本编辑器。
  @visibleForTesting
  bool get debugIsFocusRecoveryOwner => _focusRecoveryOwner == this;

  // ---- 窗口焦点恢复（Linux 桌面：窗口失活会丢光标）----
  // 失活时 GTK 把 Flutter primary focus 抹掉，vendor 键盘服务随即清空选区、
  // 断开 IME；切回前台没有任何恢复逻辑，只能重新点击。
  // 修复：失活(inactive/hidden)时记下选区（认领恢复权）；回到前台(resumed)
  // 用 uiEvent 重放选区 —— vendor 按该原因 requestFocus + 重挂 IME，一步恢复输入态。
  // 多实例（编辑态知识页 + 顶层「创建主题」对话框）守卫：有 primary focus 才认领；
  // 无 focus 时仅当全局唯一候选；认领者为准，避免多个编辑器互相抢焦点。
  static WikiContentEditorState? _focusRecoveryOwner;
  static Selection? _focusRecoverySelection;
  static final Set<WikiContentEditorState> _mountedEditable = {};

  void _claimFocusRecovery() {
    final selection = _editorState.selection ?? _lastSelection;
    if (!widget.editable || !mounted || selection == null) return;
    final primary = FocusManager.instance.primaryFocus;
    if (primary != null) {
      if (!_isFocusInside(primary)) return;
    } else {
      // primary focus 已被窗口失活提前清掉：只有全局唯一候选才敢认领，
      // 多个编辑器有残存选区分不清谁在前台，宁可维持原状（点一下恢复）。
      final candidates = _mountedEditable.where((state) {
        return state.mounted &&
            state.widget.editable &&
            state._lastSelection != null;
      }).toList();
      if (candidates.length != 1) return;
    }
    _focusRecoveryOwner = this;
    _focusRecoverySelection = selection;
  }

  /// 判定一个 FocusNode 是否属于本编辑器子树（含内层对话框/浮层）。
  bool _isFocusInside(FocusNode node) {
    final nodeContext = node.context;
    if (nodeContext == null || !mounted) return false;
    var inside = false;
    nodeContext.visitAncestorElements((ancestor) {
      if (identical(ancestor, context)) {
        inside = true;
        return false;
      }
      return true;
    });
    return inside;
  }

  void _restoreFocusRecovery() {
    if (_focusRecoveryOwner != this) return;
    final selection = _focusRecoverySelection;
    _focusRecoveryOwner = null;
    _focusRecoverySelection = null;
    if (selection == null || !widget.editable || !mounted) return;
    unawaited(
      _editorState.updateSelectionWithReason(
        selection,
        reason: SelectionUpdateReason.uiEvent,
      ),
    );
  }

  @override
  void initState() {
    super.initState();
    _initEditor(widget.contentMd, widget.slug);
    if (widget.enableSaveShortcut) {
      HardwareKeyboard.instance.addHandler(_onKeyEvent);
    }
    WidgetsBinding.instance.addObserver(this);
    if (widget.editable) _mountedEditable.add(this);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted) return;
    switch (state) {
      case AppLifecycleState.inactive:
      case AppLifecycleState.hidden:
        _claimFocusRecovery();
      case AppLifecycleState.resumed:
        _restoreFocusRecovery();
      case AppLifecycleState.detached:
      case AppLifecycleState.paused:
        break;
    }
  }

  /// 文档生命周期：解码正文 + 尾部对话块 → 建 EditorState → 定格 markdown 快照。
  ///
  /// 编辑器首次挂载时可能对文档做规范化，首帧渲染完成后再定格一次快照，
  /// 避免「无改动却判脏」。之后的事务才代表真实编辑。
  void _initEditor(String contentMd, String slug) {
    _txSub?.cancel();
    _editorState = _buildEditorState(contentMd, slug);
    _lastSelection = null;
    _bindSelectionNotifier();
    _loadedMd = wikiDocumentToMarkdown(_editorState.document);
    _txSub = _editorState.transactionStream.listen((_) {
      widget.onDirtyChanged?.call(isDirty);
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _loadedMd = wikiDocumentToMarkdown(_editorState.document);
      widget.onDirtyChanged?.call(false);
    });
  }

  @override
  void didUpdateWidget(covariant WikiContentEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.contentMd != oldWidget.contentMd) {
      // 保存后页面重载：从最新正文重建文档（丢弃编辑态残留）
      _initEditor(widget.contentMd, widget.slug);
    }
  }

  @override
  void dispose() {
    if (widget.enableSaveShortcut) {
      HardwareKeyboard.instance.removeHandler(_onKeyEvent);
    }
    WidgetsBinding.instance.removeObserver(this);
    _mountedEditable.remove(this);
    _boundSelectionState?.selectionNotifier.removeListener(_onSelectionChanged);
    if (_focusRecoveryOwner == this) {
      _focusRecoveryOwner = null;
      _focusRecoverySelection = null;
    }
    _txSub?.cancel();
    super.dispose();
  }

  /// 文档构造：仅包含正文。AI 对话由页面右侧的独立面板承载。
  static EditorState _buildEditorState(String contentMd, String slug) {
    final document = wikiMarkdownToDocument(contentMd);
    // 空 markdown 解码后是零子节点的 Document.blank()，编辑器没有可聚焦的
    // 段落，点击/键盘都无法进入输入态（「创建主题」空正文 tab 的实测问题）。
    // 这里补一个空段落，让新建页从一开始就有落点。
    if (document.root.children.isEmpty) {
      document.insert([0], [paragraphNode()]);
    }
    return EditorState(document: document);
  }

  /// `Ctrl/Cmd+S`：编辑态内全局保存
  bool _onKeyEvent(KeyEvent event) {
    if (event is! KeyDownEvent || event is KeyRepeatEvent) return false;
    if (event.logicalKey != LogicalKeyboardKey.keyS) return false;
    final hw = HardwareKeyboard.instance;
    if (!(hw.isControlPressed || hw.isMetaPressed)) return false;
    unawaited(save());
    return true;
  }

  /// 保存正文。
  ///
  /// - 非编辑态 / 已在保存中 / 无改动：直接返回 true（无副作用）；
  /// - 有改动：`wikiDocumentToMarkdown` → [widget.onSave]；
  ///   成功后快照更新为当前文档；失败调用 [widget.onSaveError] 并返回 false。
  Future<bool> save() async {
    if (_saving || !widget.editable) return true;
    if (!isDirty) return true;

    final markdown = wikiDocumentToMarkdown(_editorState.document);
    setState(() => _saving = true);
    try {
      await widget.onSave(markdown);
      _loadedMd = wikiDocumentToMarkdown(_editorState.document);
      return true;
    } on Exception catch (e) {
      widget.onSaveError?.call(e);
      return false;
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  /// 放弃修改：从加载快照重建文档（「取消」入口）。
  void discard() {
    _initEditor(widget.contentMd, widget.slug);
  }

  /// 两层覆盖求值（调用方传 `编辑器覆盖 ?? 全局内容字体` 的存值）：
  /// system → null（不注入家族，跟随系统字体）；未传时回退 [ContentFont]
  /// 作用域的内容字体（主题 textTheme 是固定界面字体，不能作回退）。
  /// 覆盖值经 [resolveFontFamily] 解析；本地字体在启动/切换时预热。
  String? _effectiveFontFamily(BuildContext context) {
    final override = widget.fontFamily;
    if (override != null && override.isNotEmpty) {
      final family = resolveFontFamily(
        override,
        fallback: AppFonts.defaultFont,
      );
      if (family == null) return null;
      return family;
    }
    final fallback = ContentFont.styleOf(context)?.fontFamily;
    return (fallback == null || fallback.isEmpty) ? null : fallback;
  }

  @override
  Widget build(BuildContext context) {
    final editor = AppFlowyEditor(
      editorState: _editorState,
      editable: widget.editable,
      autoFocus: false,
      shrinkWrap: widget.shrinkWrap,
      editorStyle: EditorStyle.desktop(
        // vendor 默认 desktop padding 左右各 100：宽屏阅读栏内仍够，但窄视口
        // （移动端）会把内容压垮（390 - 48 页边 - 200 = 142px 可用，移动走查实证）。
        // 显式收窄到 24，宽屏仍有阅读栏兜底、窄屏不溢出。
        padding: EdgeInsets.symmetric(horizontal: AppTheme.space6),
        // 编辑器正文不继承 Theme textTheme（vendor 默认样式无 fontFamily），
        // 需按两层覆盖模型求值字体后注入：编辑器覆盖(system→不注入) ?? 全局。
        textStyleConfiguration: fontAwareTextStyleConfiguration(
          _effectiveFontFamily(context),
          // 正文颜色显式取主题正文色：编辑器文本不再依赖 Document rooting 之外
          // 的 DefaultTextStyle 继承链，深浅色切换下内容文字必定跟随主题。
          color:
              widget.textColor ??
              Theme.of(context).textTheme.bodyLarge?.color ??
              Theme.of(context).textTheme.bodyMedium?.color,
          // 正文字号：跟随「外观 → 正文字号」（默认 16 即当前观感）
          fontSize: widget.fontSize,
          // 行距：编辑器覆盖 ?? 全局（null 时 fontAware 回落 vendor 1.5）
          lineHeight: widget.lineHeight,
        ),
        textSpanDecorator: wikiTextSpanDecorator(
          onTapWikiLink: widget.onWikiLinkTap,
        ),
      ),
      blockComponentBuilders: {
        ...standardBlockComponentBuilderMap,
        // §11 Q4：code 块降级（vendor 无组件，占位 30px；只读展示 + 复制）
        WikiCodeBlockKeys.type: WikiCodeBlockComponentBuilder(),
        // §表格浏览态错位修复：vendor 行高同步依赖 apply（只读被 `!editable`
        // 拦截），列按各自内容高堆叠后外层 Row 垂直居中 → 列错位。
        // 只读态改 Flutter Table 布局（TableRow 行内天然等高），编辑态委托 vendor。
        TableBlockKeys.type: WikiReadonlyTableBlockComponentBuilder(),
        // 标题层级：vendor 标题块沿用全局 4px 垂直 padding，与正文区隔弱。
        // 加「上留白大、下留白小」的标题间距，观感对齐 AppFlowy 桌面端。
        HeadingBlockKeys.type: HeadingBlockComponentBuilder(
          configuration: standardBlockComponentConfiguration.copyWith(
            placeholderText: (node) {
              final level = node.attributes[HeadingBlockKeys.level];
              return switch (level) {
                1 => AppFlowyEditorL10n.current.heading1,
                2 => AppFlowyEditorL10n.current.heading2,
                3 => AppFlowyEditorL10n.current.heading3,
                _ => '标题 $level',
              };
            },
            padding: (node) {
              final level = node.attributes[HeadingBlockKeys.level] as int?;
              final top = switch (level) {
                1 => 24.0,
                2 => 18.0,
                3 => 12.0,
                _ => 8.0,
              };
              return EdgeInsets.only(top: top, bottom: AppTheme.space2);
            },
          ),
        ),
        // 引用块：vendor 图标是硬编码 AppFlowy 品牌蓝 #00BCF0 竖条，
        // 与暖橙主题冲突；换成主题 accentPrimary 竖条。
        QuoteBlockKeys.type: QuoteBlockComponentBuilder(
          configuration: standardBlockComponentConfiguration.copyWith(
            placeholderText: (_) => AppFlowyEditorL10n.current.quote,
          ),
          iconBuilder: (context, node) => Container(
            alignment: Alignment.center,
            constraints: const BoxConstraints(minWidth: 26, minHeight: 22),
            padding: const EdgeInsets.only(right: 4.0),
            child: Container(width: 4, color: AppTheme.accentPrimary),
          ),
        ),
      },
    );

    if (!widget.editable) return editor;

    return Column(
      children: [
        _WikiEditorToolbar(editorState: _editorState),
        Expanded(child: editor),
      ],
    );
  }
}

/// 编辑态格式工具栏：固定显示在正文上方，避免选区工具栏在对话框/窄屏里
/// 被遮住；按钮直接操作同一个 [EditorState]，与快捷键行为一致。
class _WikiEditorToolbar extends StatefulWidget {
  const _WikiEditorToolbar({required this.editorState});

  final EditorState editorState;

  @override
  State<_WikiEditorToolbar> createState() => _WikiEditorToolbarState();
}

class _WikiEditorToolbarState extends State<_WikiEditorToolbar> {
  StreamSubscription<EditorTransactionValue>? _txSub;

  @override
  void initState() {
    super.initState();
    _bind();
  }

  @override
  void didUpdateWidget(covariant _WikiEditorToolbar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.editorState != oldWidget.editorState) {
      _unbind();
      _bind();
    }
  }

  @override
  void dispose() {
    _unbind();
    super.dispose();
  }

  void _bind() {
    widget.editorState.selectionNotifier.addListener(_refresh);
    _txSub = widget.editorState.transactionStream.listen((_) => _refresh());
  }

  void _unbind() {
    widget.editorState.selectionNotifier.removeListener(_refresh);
    _txSub?.cancel();
    _txSub = null;
  }

  void _refresh() {
    if (mounted) setState(() {});
  }

  Selection? get _selection => widget.editorState.selection;

  Node? get _currentNode {
    final selection = _selection;
    if (selection == null) return null;
    return widget.editorState.getNodeAtPath(selection.start.path);
  }

  bool _isFormatActive(String key) {
    final selection = _selection;
    if (selection == null) return false;
    if (selection.isCollapsed) {
      return widget.editorState.toggledStyle[key] == true;
    }
    final nodes = widget.editorState.getNodesInSelection(selection);
    if (nodes.isEmpty) return false;
    return nodes.allSatisfyInSelection(
      selection,
      (delta) =>
          delta.isNotEmpty &&
          delta.everyAttributes((attributes) => attributes[key] == true),
    );
  }

  bool _isBlock(String type) => _currentNode?.type == type;

  bool _isHeading(int level) =>
      _isBlock(HeadingBlockKeys.type) &&
      _currentNode?.attributes[HeadingBlockKeys.level] == level;

  Future<void> _toggleAttribute(String key) =>
      widget.editorState.toggleAttribute(key);

  Future<void> _formatBlock(String type, {int? level}) async {
    final selection = _selection;
    if (selection == null) return;
    await widget.editorState.formatNode(selection, (node) {
      // 列表 / 引用沿用 vendor toolbar 的做法：只换 type，保留节点已有属性。
      if (type != HeadingBlockKeys.type && type != ParagraphBlockKeys.type) {
        return node.copyWith(type: type);
      }
      final delta = (node.delta ?? Delta()).toJson();
      final attributes = <String, Object?>{
        blockComponentDelta: delta,
        blockComponentBackgroundColor:
            node.attributes[blockComponentBackgroundColor],
        blockComponentTextDirection:
            node.attributes[blockComponentTextDirection],
      };
      if (type == HeadingBlockKeys.type) {
        attributes[HeadingBlockKeys.level] = level;
      }
      return node.copyWith(type: type, attributes: attributes);
    });
  }

  Future<void> _toggleHeading(int level) => _formatBlock(
    _isHeading(level) ? ParagraphBlockKeys.type : HeadingBlockKeys.type,
    level: level,
  );

  Future<void> _toggleBlock(String type) =>
      _formatBlock(_isBlock(type) ? ParagraphBlockKeys.type : type);

  Widget _iconButton({
    required IconData icon,
    required String tooltip,
    required bool active,
    required VoidCallback onPressed,
  }) {
    return IconButton(
      tooltip: tooltip,
      onPressed: onPressed,
      icon: Icon(icon),
      iconSize: 18,
      visualDensity: VisualDensity.compact,
      padding: const EdgeInsets.all(6),
      constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
      style: IconButton.styleFrom(
        foregroundColor: active
            ? AppTheme.accentPrimary
            : AppTheme.textSecondary,
        backgroundColor: active
            ? AppTheme.accentPrimary.withValues(alpha: 0.12)
            : null,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
      ),
    );
  }

  Widget _textButton({
    required String label,
    required String tooltip,
    required bool active,
    required VoidCallback onPressed,
  }) {
    return Tooltip(
      message: tooltip,
      child: TextButton(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          minimumSize: const Size(32, 32),
          padding: const EdgeInsets.symmetric(horizontal: 6),
          foregroundColor: active
              ? AppTheme.accentPrimary
              : AppTheme.textSecondary,
          backgroundColor: active
              ? AppTheme.accentPrimary.withValues(alpha: 0.12)
              : null,
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        ),
        child: Text(
          label,
          style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
        ),
      ),
    );
  }

  Widget _divider() => Container(
    width: 1,
    height: 20,
    margin: const EdgeInsets.symmetric(horizontal: 4),
    color: AppTheme.surface3,
  );

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(bottom: BorderSide(color: AppTheme.surface3)),
      ),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        // 工具栏按钮不抢编辑器焦点：点完格式后光标仍在正文，可以继续输入。
        child: ExcludeFocus(
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              _textButton(
                label: '正文',
                tooltip: '正文',
                active: _isBlock(ParagraphBlockKeys.type),
                onPressed: () => _formatBlock(ParagraphBlockKeys.type),
              ),
              _textButton(
                label: 'H1',
                tooltip: '一级标题',
                active: _isHeading(1),
                onPressed: () => _toggleHeading(1),
              ),
              _textButton(
                label: 'H2',
                tooltip: '二级标题',
                active: _isHeading(2),
                onPressed: () => _toggleHeading(2),
              ),
              _textButton(
                label: 'H3',
                tooltip: '三级标题',
                active: _isHeading(3),
                onPressed: () => _toggleHeading(3),
              ),
              _divider(),
              _iconButton(
                icon: Icons.format_bold,
                tooltip: '加粗',
                active: _isFormatActive('bold'),
                onPressed: () => _toggleAttribute('bold'),
              ),
              _iconButton(
                icon: Icons.format_italic,
                tooltip: '斜体',
                active: _isFormatActive('italic'),
                onPressed: () => _toggleAttribute('italic'),
              ),
              _iconButton(
                icon: Icons.format_underlined,
                tooltip: '下划线',
                active: _isFormatActive('underline'),
                onPressed: () => _toggleAttribute('underline'),
              ),
              _iconButton(
                icon: Icons.format_strikethrough,
                tooltip: '删除线',
                active: _isFormatActive('strikethrough'),
                onPressed: () => _toggleAttribute('strikethrough'),
              ),
              _iconButton(
                icon: Icons.code,
                tooltip: '行内代码',
                active: _isFormatActive('code'),
                onPressed: () => _toggleAttribute('code'),
              ),
              _divider(),
              _iconButton(
                icon: Icons.format_list_bulleted,
                tooltip: '无序列表',
                active: _isBlock(BulletedListBlockKeys.type),
                onPressed: () => _toggleBlock(BulletedListBlockKeys.type),
              ),
              _iconButton(
                icon: Icons.format_list_numbered,
                tooltip: '有序列表',
                active: _isBlock(NumberedListBlockKeys.type),
                onPressed: () => _toggleBlock(NumberedListBlockKeys.type),
              ),
              _iconButton(
                icon: Icons.format_quote,
                tooltip: '引用',
                active: _isBlock(QuoteBlockKeys.type),
                onPressed: () => _toggleBlock(QuoteBlockKeys.type),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// 编辑器正文样式：vendor 默认 `TextStyleConfiguration` 的基础样式
/// （text/bold/italic/underline/strikethrough）均无 fontFamily 与 color，
/// 这里把主题解析后的 [family] 与 [color] 平铺上去，让正文跟随
/// 「外观 → 字体」，并保证内容颜色显式挂钩主题（防继承链断链）。
///
/// - [family]：正文使用的字体族（如 'Inter' / 'Noto Sans SC'；system/空时为 null）；
/// - [color]：主题正文色。仅当目标样式自身未带颜色时才注入；
///   span 级显式颜色（attributes.color）在 combine 时仍覆盖基础色；
/// - 语义色对照 AppFlowy 显示效果调优：
///   - [TextStyleConfiguration.href]（vendor 亮蓝细下划线）→ 主题强调色 accent；
///   - [TextStyleConfiguration.code]（vendor 红字+青色透明底，深色主题下刺眼）
///     → 「等宽 + 中性底 surface2 + 主题正文色」；
///   - autoComplete 保留 vendor 灰；
/// - [fontSize]：正文字号。仅写入基础 `text` 样式——vendor 的
///   text/bold/italic/... 按 combine 合并，基础字号会自动传递到加粗/斜体等
///   组合；heading/code 等带显式 delta 字号的场景仍以显式值为准。为 null 时不注入；
/// - [lineHeight]：正文行距（倍数，编辑器「AA」覆盖层求值后的值）。为 null 时
///   回落 vendor 默认 1.5；
/// - 其余为空时原样返回 vendor 默认（零影响）。
TextStyleConfiguration fontAwareTextStyleConfiguration(
  String? family, {
  Color? color,
  double? fontSize,
  double? lineHeight,
}) {
  final base = const TextStyleConfiguration();
  final injectSize =
      (fontSize != null && fontSize > 0 && base.text.fontSize != fontSize);
  if ((family == null || family.isEmpty) &&
      color == null &&
      !injectSize &&
      lineHeight == null) {
    return base;
  }
  TextStyle themed(TextStyle style) {
    var next = style;
    if (family != null && family.isNotEmpty && next.fontFamily != family) {
      next = next.copyWith(fontFamily: family);
    }
    if (color != null && next.color == null) {
      next = next.copyWith(color: color);
    }
    return next;
  }

  // 字号只落基础 text：bold/italic/underline/strikethrough 经 combine
  // 继承基础字号（它们自身不带 fontSize，合并时保留父级值）。
  // 行距独立于基础样式，落在配置顶层（vendow 行盒读取 lineHeight）。
  return TextStyleConfiguration(
    text: themed(base.text).copyWith(fontSize: injectSize ? fontSize : null),
    bold: themed(base.bold),
    italic: themed(base.italic),
    underline: themed(base.underline),
    strikethrough: themed(base.strikethrough),
    // href（vendor 亮蓝细下划线）：改主题强调色 + 下划线，与 wikilink 视觉一致
    href: themed(base.href).copyWith(
      color: AppTheme.accentPrimary,
      decoration: TextDecoration.underline,
      decorationColor: AppTheme.accentMuted,
    ),
    // code（vendor 红字+青色透明底，深色主题下刺眼）：改等宽 + 中性底 +
    // 主题正文色——与自研 wiki_code_block 的观感对齐（§11 Q4）
    code: base.code.copyWith(
      fontFamily: 'monospace',
      color: AppTheme.textPrimary,
      backgroundColor: AppTheme.surface2,
    ),
    autoComplete: themed(base.autoComplete),
    lineHeight: lineHeight ?? base.lineHeight,
  );
}
