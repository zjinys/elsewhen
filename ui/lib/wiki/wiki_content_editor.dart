import 'dart:async';

import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/app_theme.dart';
import 'wiki_chat_block.dart';
import 'wiki_code_block.dart';
import 'wiki_markdown_codec.dart';
import 'wiki_text_span_decorator.dart';

/// 正文编辑器组件（M3 §6 / §7 集成）。
///
/// - 浏览 / 编辑双模式：外层切 [editable] 即可（同一 [EditorState] 实例）；
/// - 文档为「正文 markdown ⇄ AppFlowy 文档」生产管线解码结果，尾部追加
///   [wikiChatNode]（§7 Form A 页尾对话块，聊天内容不进 markdown）；
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
  });

  /// 页面 slug：决定尾部对话块挂哪个会话
  final String slug;

  /// 加载时的正文 markdown（脏检查快照基准；变更即视为重新加载）
  final String contentMd;

  /// 是否可编辑（false = 只读浏览；聊天块仍可交互）
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

  @override
  State<WikiContentEditor> createState() => WikiContentEditorState();
}

class WikiContentEditorState extends State<WikiContentEditor> {
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

  @override
  void initState() {
    super.initState();
    _initEditor(widget.contentMd, widget.slug);
    HardwareKeyboard.instance.addHandler(_onKeyEvent);
  }

  /// 文档生命周期：解码正文 + 尾部对话块 → 建 EditorState → 定格 markdown 快照。
  ///
  /// 编辑器首次挂载时可能对文档做规范化，首帧渲染完成后再定格一次快照，
  /// 避免「无改动却判脏」。之后的事务才代表真实编辑。
  void _initEditor(String contentMd, String slug) {
    _txSub?.cancel();
    _editorState = _buildEditorState(contentMd, slug);
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
    HardwareKeyboard.instance.removeHandler(_onKeyEvent);
    _txSub?.cancel();
    super.dispose();
  }

  /// 文档构造：正文解码 + 尾部对话块（§7 Form A）
  static EditorState _buildEditorState(String contentMd, String slug) {
    final doc = wikiMarkdownToDocument(contentMd);
    doc.root.insert(wikiChatNode(slug: slug));
    return EditorState(document: doc);
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

  @override
  Widget build(BuildContext context) {
    return AppFlowyEditor(
      editorState: _editorState,
      editable: widget.editable,
      autoFocus: false,
      shrinkWrap: widget.shrinkWrap,
      editorStyle: EditorStyle.desktop(
        // vendor 默认 desktop padding 左右各 100：宽屏阅读栏内仍够，但窄视口
        // （移动端）会把内容压垮（390 - 48 页边 - 200 = 142px 可用，移动走查实证）。
        // 显式收窄到 24，宽屏仍有阅读栏兜底、窄屏不溢出。
        padding: EdgeInsets.symmetric(horizontal: AppTheme.space6),
        textSpanDecorator: wikiTextSpanDecorator(
          onTapWikiLink: widget.onWikiLinkTap,
        ),
      ),
      blockComponentBuilders: {
        ...standardBlockComponentBuilderMap,
        // §7 Form A：页尾对话块（聊天内容不进 markdown，按 slug 独立持久化）
        WikiChatBlockKeys.type: WikiChatBlockComponentBuilder(),
        // §11 Q4：code 块降级（vendor 无组件，占位 30px；只读展示 + 复制）
        WikiCodeBlockKeys.type: WikiCodeBlockComponentBuilder(),
      },
    );
  }
}