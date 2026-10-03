import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/conversation_provider.dart';
import '../providers/todo_provider.dart';
import '../providers/wiki_provider.dart';

String _actionLabel(String action) =>
    const {
      'activate_rule': '个人规则',
      'create_todo': '创建待办',
      'save_knowledge_draft': '保存知识页',
      'save_wiki_revision': '知识页修订或派生',
      'rename_wiki_page': '知识页改名',
      'record_event': '保存记录',
      'propose_people_relations': '联系人与关系',
      'archive_conversations_by_title': '归档对话',
      'import_url_to_wiki': '导入来源',
      'import_directory_files': '导入目录文件',
      'import_directory_as_project': '导入项目摘要',
    }[action] ??
    action;

Map<String, dynamic>? _args(String text) {
  try {
    final value = jsonDecode(text);
    return value is Map ? Map<String, dynamic>.from(value) : null;
  } catch (_) {
    return null;
  }
}

api.ChatReviewRequest _request(api.ChatProposalDto proposal) =>
    api.ChatReviewRequest(
      kind: proposal.kind,
      proposalId: proposal.id,
      conversationId: proposal.conversationId,
      expectedVersion: proposal.version,
      approvalToken: proposal.approvalToken,
    );

String _statusLabel(String status) =>
    const {
      'executing': '等待执行回执',
      'failed': '执行失败（已回滚）',
      'succeeded': '已完成',
      'declined': '已拒绝',
      'uncertain': '结果待核实',
    }[status] ??
    status;

class ChatProposalReviewButton extends StatelessWidget {
  const ChatProposalReviewButton({
    super.key,
    required this.conversationId,
    this.onChanged,
    this.label = '审阅提议',
    this.compact = false,
  });
  final String? conversationId;
  final VoidCallback? onChanged;
  final String label;
  final bool compact;

  Future<void> _open(BuildContext context) async {
    await showDialog<bool>(
      context: context,
      builder: (_) => ChatProposalReviewDialog(conversationId: conversationId),
    );
    if (context.mounted) onChanged?.call();
  }

  @override
  Widget build(BuildContext context) => compact
      ? IconButton(
          tooltip: label,
          icon: const Icon(Icons.fact_check_outlined, size: 18),
          onPressed: () => _open(context),
        )
      : TextButton.icon(
          icon: const Icon(Icons.fact_check_outlined, size: 18),
          label: Text(label),
          onPressed: () => _open(context),
        );
}

class ChatProposalReviewDialog extends ConsumerStatefulWidget {
  const ChatProposalReviewDialog({super.key, required this.conversationId});
  final String? conversationId;
  @override
  ConsumerState<ChatProposalReviewDialog> createState() =>
      _ChatProposalReviewDialogState();
}

class _ChatProposalReviewDialogState
    extends ConsumerState<ChatProposalReviewDialog> {
  static const _pageSize = 25;
  List<api.ChatProposalDto> _proposals = [];
  List<api.ChatDecisionDto> _decisions = [];
  final Set<String> _selected = {};
  bool _loading = true, _history = false, _changed = false;
  int _offset = 0;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _selected.clear();
      _error = null;
    });
    try {
      final repo = ref.read(conversationRepositoryProvider);
      if (_history) {
        final rows = await repo.listChatDecisions(
          widget.conversationId,
          offset: _offset,
          limit: _pageSize,
        );
        if (!mounted) return;
        setState(() => _decisions = rows);
      } else {
        final rows = await repo.listChatProposals(
          widget.conversationId,
          offset: _offset,
          limit: _pageSize,
        );
        if (!mounted) return;
        setState(() => _proposals = rows);
      }
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _review(api.ChatProposalDto proposal) async {
    final changed = await showDialog<bool>(
      context: context,
      builder: (_) => ChatProposalDetailDialog(proposal: proposal),
    );
    await _afterReview(changed);
  }

  Future<void> _reviewSelected() async {
    final selected = _proposals
        .where((p) => _selected.contains('${p.kind}:${p.id}'))
        .toList();
    if (selected.isEmpty) return;
    final changed = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (_) => ChatProposalBatchDialog(proposals: selected),
    );
    await _afterReview(changed);
  }

  Future<void> _afterReview(bool? changed) async {
    if (!mounted) return;
    _changed = _changed || changed == true;
    _offset = 0;
    ref.invalidate(pendingActionsProvider);
    ref.invalidate(todosProvider);
    ref.invalidate(wikiPagesProvider);
    ref.invalidate(conversationsProvider);
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final count = _history ? _decisions.length : _proposals.length;
    return AlertDialog(
      title: Text(widget.conversationId == null ? '历史全局规则审阅' : '对话提议审阅'),
      content: SizedBox(
        width: 640,
        height: MediaQuery.sizeOf(context).height * .6,
        child: Column(
          children: [
            Wrap(
              spacing: 8,
              children: [
                ChoiceChip(
                  label: const Text('待审'),
                  selected: !_history,
                  onSelected: _loading
                      ? null
                      : (_) {
                          setState(() {
                            _history = false;
                            _offset = 0;
                          });
                          _load();
                        },
                ),
                ChoiceChip(
                  label: const Text('处理记录'),
                  selected: _history,
                  onSelected: _loading
                      ? null
                      : (_) {
                          setState(() {
                            _history = true;
                            _offset = 0;
                          });
                          _load();
                        },
                ),
                IconButton(
                  tooltip: '刷新',
                  onPressed: _loading ? null : _load,
                  icon: const Icon(Icons.refresh),
                ),
              ],
            ),
            if (!_history && _selected.isNotEmpty)
              TextButton.icon(
                onPressed: _loading ? null : _reviewSelected,
                icon: const Icon(Icons.checklist),
                label: Text('审阅选中（${_selected.length}）'),
              ),
            const Text('接受或拒绝只作用于你打开的具体提议；普通聊天回复不会批准操作。'),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.all(12),
                child: Text(
                  _error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            Expanded(
              child: _loading
                  ? const Center(child: CircularProgressIndicator())
                  : count == 0
                  ? Center(child: Text(_history ? '暂无处理记录' : '暂无待审提议'))
                  : ListView.builder(
                      itemCount: count,
                      itemBuilder: (context, index) {
                        if (_history) {
                          final receipt = _decisions[index];
                          return ExpansionTile(
                            key: ValueKey('${receipt.kind}:${receipt.id}'),
                            title: Text(
                              '${_actionLabel(receipt.action)} · ${_statusLabel(receipt.status)}',
                            ),
                            subtitle: Text(
                              receipt.result,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            children: [
                              Padding(
                                padding: const EdgeInsets.all(12),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    SelectableText(receipt.result),
                                    const SizedBox(height: 12),
                                    _ProposalContent(
                                      argsJson: receipt.argsJson,
                                    ),
                                  ],
                                ),
                              ),
                            ],
                          );
                        }
                        final proposal = _proposals[index];
                        final args = _args(proposal.argsJson);
                        return ListTile(
                          key: ValueKey('${proposal.kind}:${proposal.id}'),
                          leading: Checkbox(
                            key: ValueKey(
                              'select-${proposal.kind}-${proposal.id}',
                            ),
                            value: _selected.contains(
                              '${proposal.kind}:${proposal.id}',
                            ),
                            onChanged: (selected) => setState(() {
                              final key = '${proposal.kind}:${proposal.id}';
                              if (selected == true) {
                                _selected.add(key);
                              } else {
                                _selected.remove(key);
                              }
                            }),
                          ),
                          title: Text(
                            '${args?['title'] ?? args?['content'] ?? _actionLabel(proposal.action)}',
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                          subtitle: Text(
                            '${_actionLabel(proposal.action)} · 版本 ${proposal.version}',
                          ),
                          trailing: const Icon(Icons.chevron_right),
                          onTap: () => _review(proposal),
                        );
                      },
                    ),
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                TextButton(
                  onPressed: _loading || _offset == 0
                      ? null
                      : () {
                          _offset = (_offset - _pageSize).clamp(0, _offset);
                          _load();
                        },
                  child: const Text('上一页'),
                ),
                Text('第 ${_offset ~/ _pageSize + 1} 页'),
                TextButton(
                  onPressed: _loading || count < _pageSize
                      ? null
                      : () {
                          _offset += _pageSize;
                          _load();
                        },
                  child: const Text('下一页'),
                ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, _changed),
          child: const Text('关闭'),
        ),
      ],
    );
  }
}

class _ProposalContent extends StatelessWidget {
  const _ProposalContent({required this.argsJson});
  final String argsJson;
  @override
  Widget build(BuildContext context) {
    final args = _args(argsJson);
    if (args == null) {
      return SelectableText('提议格式异常，不能接受；可以拒绝并重新草拟。\n$argsJson');
    }
    final body = args['content_md'] ?? args['content'] ?? args['raw_text'];
    final rest = Map<String, dynamic>.from(args)
      ..remove('title')
      ..remove('content_md')
      ..remove('content')
      ..remove('raw_text');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (args['title'] != null)
          SelectableText(
            '${args['title']}',
            style: Theme.of(context).textTheme.titleMedium,
          ),
        if (body != null) SelectableText('$body'),
        if (rest.isNotEmpty) ...[
          const SizedBox(height: 12),
          ExpansionTile(
            title: const Text('参数与来源'),
            initiallyExpanded: body == null,
            children: [
              SelectableText(const JsonEncoder.withIndent('  ').convert(rest)),
            ],
          ),
        ],
      ],
    );
  }
}

class ChatProposalBatchDialog extends ConsumerStatefulWidget {
  const ChatProposalBatchDialog({super.key, required this.proposals});
  final List<api.ChatProposalDto> proposals;
  @override
  ConsumerState<ChatProposalBatchDialog> createState() =>
      _ChatProposalBatchDialogState();
}

class _ChatProposalBatchDialogState
    extends ConsumerState<ChatProposalBatchDialog> {
  final Map<String, String> _results = {};
  bool _busy = false, _finished = false;

  Future<void> _apply(bool accept) async {
    if (_busy || _finished) return;
    setState(() => _busy = true);
    final repo = ref.read(conversationRepositoryProvider);
    for (final proposal in widget.proposals) {
      final key = '${proposal.kind}:${proposal.id}';
      String message;
      try {
        final receipt = await repo.decideChatProposal(
          _request(proposal),
          accept: accept,
        );
        message = '${_statusLabel(receipt.status)}：${receipt.result}';
      } catch (e) {
        message = '本项未确认完成，请核对处理记录；不会自动重试：$e';
      }
      if (!mounted) return;
      setState(() => _results[key] = message);
    }
    if (mounted) {
      setState(() {
        _busy = false;
        _finished = true;
      });
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !_busy,
    child: AlertDialog(
      title: Text('审阅选中的 ${widget.proposals.length} 项'),
      content: SizedBox(
        width: 660,
        height: MediaQuery.sizeOf(context).height * .6,
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('仅处理下面列出的具体版本。逐项执行，不是整批原子提交；可能部分成功。每项都会显示结果。'),
              for (final proposal in widget.proposals) ...[
                const Divider(),
                Text(
                  '${_actionLabel(proposal.action)} · 版本 ${proposal.version}',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                _ProposalContent(argsJson: proposal.argsJson),
                if (proposal.targetBasisJson != '[]')
                  ExpansionTile(
                    title: const Text('目标版本依据'),
                    children: [SelectableText(proposal.targetBasisJson)],
                  ),
                if (_results['${proposal.kind}:${proposal.id}']
                    case final result?)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: SelectableText(result),
                  ),
              ],
              if (_finished) const Text('本次逐项处理结束，请查看每项结果。'),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context, _finished),
          child: const Text('关闭'),
        ),
        if (!_finished) ...[
          TextButton(
            onPressed: _busy ? null : () => _apply(false),
            child: const Text('拒绝这些项'),
          ),
          FilledButton(
            onPressed:
                _busy || widget.proposals.any((p) => _args(p.argsJson) == null)
                ? null
                : () => _apply(true),
            child: Text(_busy ? '逐项处理中…' : '接受这些项'),
          ),
        ],
      ],
    ),
  );
}

class ChatProposalDetailDialog extends ConsumerStatefulWidget {
  const ChatProposalDetailDialog({super.key, required this.proposal});
  final api.ChatProposalDto proposal;
  @override
  ConsumerState<ChatProposalDetailDialog> createState() =>
      _ChatProposalDetailDialogState();
}

class _ChatProposalDetailDialogState
    extends ConsumerState<ChatProposalDetailDialog> {
  late api.ChatProposalDto _proposal;
  final Map<String, TextEditingController> _controllers = {};
  bool _busy = false, _editing = false, _changed = false;
  String? _error;
  api.ChatDecisionDto? _receipt;

  @override
  void initState() {
    super.initState();
    _proposal = widget.proposal;
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
        text: args[field]?.toString() ?? (field == 'priority' ? 'normal' : ''),
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
      if ((entry.key == 'due_at' || entry.key == 'note') &&
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
    } catch (e) {
      if (mounted) setState(() => _error = '$e\n请关闭后重新打开审阅，不会自动接受新的版本。');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
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
            children: [
              if (_receipt != null) ...[
                Text(
                  _statusLabel(_receipt!.status),
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                SelectableText(_receipt!.result),
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
                          entry.key == 'content_md' || entry.key == 'content'
                          ? 5
                          : 1,
                      maxLines: null,
                      decoration: InputDecoration(
                        labelText:
                            const {
                              'title': '标题',
                              'content_md': '正文',
                              'content': '规则内容',
                              'raw_text': '记录内容',
                              'note': '备注',
                              'due_at': '截止时间（ISO 8601，可留空）',
                              'priority': '优先级（low/normal/high）',
                              'change_note': '修改说明',
                              'new_title': '新标题',
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
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context, _changed),
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
    ),
  );
}
