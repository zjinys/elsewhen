import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../wiki/wiki_markdown_codec.dart';

/// AppFlowy Editor 中文输入法（IME）测试页。
///
/// 应用内入口：左侧栏底部「编辑器测试」。
///
/// 功能：
///  - 编辑 / 只读切换（切换时强制重建编辑器实例，验证输入法 attach 路径）
///  - 中文样例 markdown 载入（wikiMarkdownToDocument）
///  - 程序化在光标处插入中文（transaction.insertText），验证 IME 合成后的插入
///  - 导出 markdown（wikiDocumentToMarkdown）并复制，验证往返
class AppFlowyImeTestScreen extends StatefulWidget {
  const AppFlowyImeTestScreen({super.key});

  @override
  State<AppFlowyImeTestScreen> createState() => _AppFlowyImeTestScreenState();
}

class _AppFlowyImeTestScreenState extends State<AppFlowyImeTestScreen> {
  late EditorState _editorState;
  bool _editable = true;
  bool _logging = true;
  String _exportedMd = '';
  String _lastError = '';

  @override
  void initState() {
    super.initState();
    _editorState = _fromMarkdown(_sampleMarkdown);
    _applyLogging(true);
  }

  @override
  void dispose() {
    _applyLogging(false);
    super.dispose();
  }

  /// 打开/关闭 AppFlowy 的 input 级日志（delta / attach / composing 全量输出），
  /// 用于排查中文输入法合成问题。
  void _applyLogging(bool on) {
    final config = AppFlowyLogConfiguration();
    if (on) {
      config.level = AppFlowyEditorLogLevel.debug;
      config.handler = (message) => debugPrint('[appflowy] $message');
    } else {
      config.level = AppFlowyEditorLogLevel.off;
      config.handler = null;
    }
  }

  void _toggleLogging(bool value) {
    setState(() {
      _logging = value;
      _applyLogging(value);
    });
  }

  EditorState _fromMarkdown(String md) {
    // wiki codec：注册 WikilinkInlineSyntax，`[[slug]]` 解析为行内属性
    return EditorState(document: wikiMarkdownToDocument(md));
  }

  void _reload() {
    setState(() {
      _editorState = _fromMarkdown(_sampleMarkdown);
      _exportedMd = '';
      _lastError = '';
    });
  }

  void _toggleEditable(bool value) {
    setState(() {
      _editable = value;
      // 强制重建编辑器实例，重新走一遍输入法 attach 路径
      _editorState = _fromMarkdown(
        _exportedMd.isEmpty
            ? wikiDocumentToMarkdown(_editorState.document)
            : _exportedMd,
      );
    });
  }

  Future<void> _insertTestText() async {
    final selection = _editorState.selection;
    if (selection == null || !selection.isSingle) {
      setState(() => _lastError = '无光标位置：先点击编辑器定位光标');
      return;
    }
    final node = _editorState.getNodeAtPath(selection.end.path);
    if (node == null) {
      setState(() => _lastError = '定位节点失败');
      return;
    }
    try {
      final transaction = _editorState.transaction;
      transaction.insertText(
        node,
        selection.end.offset,
        '【程序插入：中文输入法合成测试】',
      );
      await _editorState.apply(transaction);
      setState(() => _lastError = '');
    } catch (e) {
      setState(() => _lastError = '插入失败: $e');
    }
  }

  Future<void> _export() async {
    try {
      final md = wikiDocumentToMarkdown(_editorState.document);
      setState(() {
        _exportedMd = md;
        _lastError = '';
      });
    } catch (e) {
      setState(() => _lastError = '导出失败: $e');
    }
  }

  Future<void> _copyExported() async {
    if (_exportedMd.isEmpty) {
      await _export();
    }
    await Clipboard.setData(ClipboardData(text: _exportedMd));
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('markdown 已复制'),
          duration: Duration(seconds: 1),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    // 测试页强制浅色 + 白底：EditorStyle 默认文本是黑色，
    // 若沿用主应用深色主题会黑字压深底看不清。
    return Theme(
      data: ThemeData.light(useMaterial3: true),
      child: Scaffold(
        backgroundColor: Colors.white,
        appBar: AppBar(
        title: const Text('AppFlowy Editor · 中文输入法测试'),
        actions: [
          const Text('编辑'),
          Switch(value: _editable, onChanged: _toggleEditable),
          const SizedBox(width: 8),
          IconButton(
            tooltip: '重新加载样例',
            icon: const Icon(Icons.refresh),
            onPressed: _reload,
          ),
          IconButton(
            tooltip: '光标处插入中文',
            icon: const Icon(Icons.text_fields),
            onPressed: _insertTestText,
          ),
          IconButton(
            tooltip: '输入日志（IME delta 追踪）',
            icon: Icon(
              _logging
                  ? Icons.bug_report
                  : Icons.bug_report_outlined,
              color: _logging ? Colors.red : null,
            ),
            onPressed: () => _toggleLogging(!_logging),
          ),
          IconButton(
            tooltip: '撤销',
            icon: const Icon(Icons.undo),
            onPressed: () => _editorState.undoManager.undo(),
          ),
          IconButton(
            tooltip: '重做',
            icon: const Icon(Icons.redo),
            onPressed: () => _editorState.undoManager.redo(),
          ),
          IconButton(
            tooltip: '导出 markdown',
            icon: const Icon(Icons.ios_share),
            onPressed: _export,
          ),
          IconButton(
            tooltip: '复制导出的 markdown',
            icon: const Icon(Icons.copy),
            onPressed: _copyExported,
          ),
          const SizedBox(width: 12),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Container(
                width: double.infinity,
                decoration: BoxDecoration(
                  color: Colors.white,
                  border: Border.all(color: Colors.black12),
                  borderRadius: BorderRadius.circular(8),
                ),
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 4,
                ),
                child: AppFlowyEditor(
                  key: ValueKey(_editable),
                  editorState: _editorState,
                  editable: _editable,
                  autoFocus: _editable,
                ),
              ),
            ),
          ),
          _StatusBar(
            status: 'editable=${_editable ? '是' : '否'} '
                '· 节点数=${_editorState.document.root.children.length} '
                '· ${_editorState.selection == null ? '无选区' : '有选区'}',
            error: _lastError,
          ),
          if (_exportedMd.isNotEmpty)
            Container(
              height: 160,
              margin: const EdgeInsets.symmetric(horizontal: 16),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.grey.shade100,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '导出的 markdown（wikiDocumentToMarkdown）',
                    style: Theme.of(context).textTheme.labelMedium,
                  ),
                  const SizedBox(height: 6),
                  Expanded(
                    child: SingleChildScrollView(
                      child: SelectableText(
                        _exportedMd,
                        style: const TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          const SizedBox(height: 12),
        ],
      ),
      ),
    );
  }
}

class _StatusBar extends StatelessWidget {
  const _StatusBar({required this.status, required this.error});

  final String status;
  final String error;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
      child: Row(
        children: [
          Icon(
            error.isEmpty ? Icons.check_circle_outline : Icons.error_outline,
            size: 14,
            color: error.isEmpty ? Colors.green : Colors.red,
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              error.isEmpty ? status : error,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }
}

/// 中文密集样例：覆盖标题 / 粗体斜体 / 无序有序列表 / 复选 / 引用 / 代码块 / wikilink。
const _sampleMarkdown = '''
# AppFlowy 中文输入法测试

这是一段**中文正文**，用来测试 *输入法*（拼音 / 五笔 / 手写）合成输入是否正常。复制粘贴也请试试：你好，世界。

- 列表项一：你好世界
- 列表项二：`行内代码 println("你好")`
- [x] 已完成任务
- [ ] 待办任务

> 引用块「中文引号」测试

```dart
void main() => print('你好，AppFlowy');
```

1. 有序列表第一项
2. 有序列表第二项

[[person/张三]] 我的 wikilink 占位行（wiki codec 已接入，`[[slug]]` 会解析为 wikilink）
''';