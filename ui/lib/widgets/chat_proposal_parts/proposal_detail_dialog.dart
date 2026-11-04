part of '../chat_proposal_review.dart';

//! 单条详情对话框+回执文本

/// 详情 dialog 与 panel 的共同基类，使两者能共用同一个 State。
abstract class ChatProposalDetailBase extends ConsumerStatefulWidget {
  const ChatProposalDetailBase({super.key, required this.proposal});
  final api.ChatProposalDto proposal;
}

class ChatProposalDetailDialog extends ChatProposalDetailBase {
  const ChatProposalDetailDialog({super.key, required super.proposal});
  @override
  ConsumerState<ChatProposalDetailBase> createState() =>
      _ChatProposalDetailDialogState();
}

/// 详情面板（不带 dialog 壳），供主从布局右栏内嵌。逻辑与 dialog 共用
/// 同一个 State，只是 build 不含 AlertDialog 壳。
class ChatProposalDetailPanel extends ChatProposalDetailBase {
  const ChatProposalDetailPanel({
    super.key,
    required super.proposal,
    this.onClose,
  });
  final VoidCallback? onClose;
  @override
  ConsumerState<ChatProposalDetailBase> createState() =>
      _ChatProposalDetailDialogState();
}

class _ChatProposalDetailDialogState
    extends ConsumerState<ChatProposalDetailBase> {
  api.ChatProposalDto get _widgetProposal => widget.proposal;
  VoidCallback? get _panelOnClose => widget is ChatProposalDetailPanel
      ? (widget as ChatProposalDetailPanel).onClose
      : null;
  late api.ChatProposalDto _proposal;
  final Map<String, TextEditingController> _controllers = {};
  bool _busy = false, _editing = false, _changed = false;
  String? _error;
  api.ChatDecisionDto? _receipt;

  @override
  void initState() {
    super.initState();
    _proposal = _widgetProposal;
  }

  @override
  void dispose() {
    for (final controller in _controllers.values) {
      controller.dispose();
    }
    super.dispose();
  }

  List<String> get _editableFields => switch (_proposal.action) {
    'activate_rule' => ['content'],
    'create_todo' => ['title', 'note', 'due_at', 'priority'],
    'save_knowledge_draft' => ['title', 'content_md'],
    'save_wiki_revision' => ['title', 'content_md', 'change_note'],
    'rename_wiki_page' => ['new_title'],
    'record_event' => ['raw_text'],
    'create_goal' => [
      'content',
      'phase',
      'why',
      'current_focus',
      'blocker',
      'done_criteria',
    ],
    'update_goal_facets' => [
      'why',
      'current_focus',
      'blocker',
      'done_criteria',
    ],
    _ => [],
  };

  void _beginEdit() {
    final args = _args(_proposal.argsJson);
    if (args == null) return;
    for (final controller in _controllers.values) {
      controller.dispose();
    }
    _controllers.clear();
    for (final field in _editableFields) {
      _controllers[field] = TextEditingController(
        text:
            args[field]?.toString() ??
            (field == 'priority' ? 'normal' : (field == 'phase' ? 'near' : '')),
      );
    }
    setState(() {
      _editing = true;
      _error = null;
    });
  }

  Future<void> _saveEdit() async {
    if (_busy || !_editing) return;
    final args = _args(_proposal.argsJson)!;
    for (final entry in _controllers.entries) {
      final text = entry.value.text;
      if ((entry.key == 'due_at' ||
              entry.key == 'note' ||
              entry.key == 'why' ||
              entry.key == 'current_focus' ||
              entry.key == 'blocker' ||
              entry.key == 'done_criteria') &&
          text.trim().isEmpty) {
        args.remove(entry.key);
      } else {
        args[entry.key] = text;
      }
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final updated = await ref
          .read(conversationRepositoryProvider)
          .editChatProposal(_request(_proposal), jsonEncode(args));
      if (!mounted) return;
      setState(() {
        _proposal = updated;
        _editing = false;
        _changed = true;
      });
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _decide(bool accept) async {
    if (_busy || _receipt != null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final receipt = await ref
          .read(conversationRepositoryProvider)
          .decideChatProposal(_request(_proposal), accept: accept);
      if (!mounted) return;
      setState(() {
        _receipt = receipt;
        _changed = true;
      });
      // 面板模式下决策完成即通知父级刷新列表（面板内已显示回执，
      // 父级刷新后会收起面板，回执记录可在「处理记录」页查看）。
    } catch (e) {
      if (mounted) setState(() => _error = '$e\n请关闭后重新打开审阅，不会自动接受新的版本。');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    // 面板形态：直接返回内容+底部操作条（供主从布局右栏）。顶栏带
    // 「返回列表」入口：移动端 sheet 无系统标题栏，只靠底部按钮
    // 返回不可发现（用户反馈 2026-11-04）。
    final isPanel = widget is ChatProposalDetailPanel;
    if (isPanel) {
      final panelClose = _panelOnClose;
      return PopScope(
        // 系统返回（Android 手势/返回键）＝「返回列表」，不冒泡关掉
        // 整个审阅面板；列表态才允许 pop 关 sheet。
        canPop: false,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop && !_busy && panelClose != null) panelClose();
        },
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 4, 16, 0),
              child: Row(
                children: [
                  if (panelClose != null)
                    TextButton.icon(
                      onPressed: _busy ? null : panelClose,
                      icon: const Icon(Icons.arrow_back_ios_new, size: 15),
                      label: const Text('返回列表'),
                    ),
                  const Spacer(),
                  Text(
                    '${_actionLabel(_proposal.action)} · 版本 ${_proposal.version}',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ],
              ),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [..._detailBlocks(context)],
                ),
              ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              child: _actionButtons(context),
            ),
          ],
        ),
      );
    }
    return PopScope(
      canPop: !_busy,
      child: AlertDialog(
        title: Text(
          '${_actionLabel(_proposal.action)} · 版本 ${_proposal.version}',
        ),
        content: SizedBox(
          width: 620,
          height: MediaQuery.sizeOf(context).height * .6,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: _detailBlocks(context),
            ),
          ),
        ),
        actions: [_actionButtons(context)],
      ),
    );
  }

  List<Widget> _detailBlocks(BuildContext context) => [
    if (_receipt != null) ...[
      Text(
        _statusLabel(_receipt!.status),
        style: Theme.of(context).textTheme.titleMedium,
      ),
      _ReceiptText(_receipt!.result),
      const Divider(),
    ],
    if (_error != null)
      Text(
        _error!,
        style: TextStyle(color: Theme.of(context).colorScheme.error),
      ),
    if (_editing)
      ..._controllers.entries.map(
        (entry) => Padding(
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: TextField(
            key: ValueKey(entry.key),
            controller: entry.value,
            enabled: !_busy,
            minLines:
                entry.key == 'content_md' ||
                    entry.key == 'content' ||
                    entry.key == 'why' ||
                    entry.key == 'current_focus' ||
                    entry.key == 'blocker' ||
                    entry.key == 'done_criteria'
                ? 3
                : 1,
            maxLines: null,
            decoration: InputDecoration(
              labelText:
                  {
                    'title': '标题',
                    'content_md': '正文',
                    'content': _proposal.action == 'create_goal'
                        ? '目标愿景'
                        : '规则内容',
                    'raw_text': '记录内容',
                    'note': '备注',
                    'due_at': '截止时间（ISO 8601，可留空）',
                    'priority': '优先级（low/normal/high）',
                    'change_note': '修改说明',
                    'new_title': '新标题',
                    'phase': '周期阶段（near/mid/long）',
                    'why': '为什么做 / 核心动力',
                    'current_focus': '当前 1~2 周启动抓手',
                    'blocker': '已知卡点 / 困难预判',
                    'done_criteria': '完成判据 / 验收标准',
                  }[entry.key] ??
                  entry.key,
            ),
          ),
        ),
      )
    else
      _ProposalContent(argsJson: _proposal.argsJson),
    if (_proposal.targetBasisJson != '[]')
      ExpansionTile(
        title: const Text('审阅所依据的目标版本'),
        children: [SelectableText(_proposal.targetBasisJson)],
      ),
    if (_editing) const Text('保存修改只更新草稿；重新查看后，仍需点击接受才执行业务操作。'),
  ];

  Widget _actionButtons(BuildContext context) {
    final panelClose = _panelOnClose;
    // 面板形态顶部已有「‹ 返回列表」（用户反馈 2026-11-04：底部重复），
    // 底部操作条不再放返回；对话框形态仍靠底部「关闭」收尾。
    final showClose = panelClose == null;
    return OverflowBar(
      alignment: MainAxisAlignment.end,
      spacing: 8,
      children: [
        if (showClose)
          TextButton(
            onPressed: _busy
                ? null
                : () {
                    Navigator.maybePop(context, _changed);
                  },
            child: const Text('关闭'),
          ),
        if (_receipt == null && !_editing) ...[
          if (_editableFields.isNotEmpty && _args(_proposal.argsJson) != null)
            TextButton(
              onPressed: _busy ? null : _beginEdit,
              child: const Text('编辑'),
            ),
          TextButton(
            onPressed: _busy ? null : () => _decide(false),
            child: const Text('拒绝这项'),
          ),
          FilledButton(
            onPressed: _busy || _args(_proposal.argsJson) == null
                ? null
                : () => _decide(true),
            child: Text(_busy ? '处理中…' : '接受这项'),
          ),
        ],
        if (_editing)
          FilledButton(
            onPressed: _busy ? null : _saveEdit,
            child: const Text('保存修改，不执行'),
          ),
      ],
    );
  }
}

/// 回执摘要化（事故 2026-10-19 根因 1.3）：弹窗/列表只做状态反馈，长回执
/// （MCP 工具 raw JSON 动辄几十行）折叠预览，不糊正文；完整结果仍可展开
/// 查看（SelectableText 保留可复制）。短回执原样直显。
class _ReceiptText extends StatefulWidget {
  final String result;
  const _ReceiptText(this.result);

  @override
  State<_ReceiptText> createState() => _ReceiptTextState();
}

class _ReceiptTextState extends State<_ReceiptText> {
  bool _expanded = false;

  static const _previewLength = 200;

  @override
  Widget build(BuildContext context) {
    final result = widget.result;
    if (result.length <= _previewLength) {
      return SelectableText(result);
    }
    final preview = _expanded
        ? result
        : '${result.substring(0, _previewLength)}…';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SelectableText(preview),
        if (!_expanded)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              onPressed: () => setState(() => _expanded = true),
              child: Text('展开完整回执（全文 ${result.length} 字）'),
            ),
          ),
      ],
    );
  }
}
