part of '../chat_proposal_review.dart';

//! 方案评审对话框（逐项批注/提交）

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
  // 主从布局：右栏当前展示的单条详情或批量集。
  api.ChatProposalDto? _viewing;
  bool _viewingBatch = false;

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
    setState(() {
      _viewing = proposal;
      _viewingBatch = false;
    });
  }

  Future<void> _reviewSelected() async {
    if (_selected.isEmpty) return;
    setState(() {
      _viewing = null;
      _viewingBatch = true;
    });
  }

  void _closeDetail() => setState(() {
    _viewing = null;
    _viewingBatch = false;
  });

  Future<void> _afterReview(bool? changed) async {
    if (!mounted) return;
    _changed = _changed || changed == true;
    _offset = 0;
    _viewing = null;
    _viewingBatch = false;
    ref.invalidate(pendingActionsProvider);
    ref.invalidate(todosProvider);
    ref.invalidate(wikiPagesProvider);
    ref.invalidate(conversationsProvider);
    ref.invalidate(_hasPendingProvider(widget.conversationId));
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final count = _history ? _decisions.length : _proposals.length;
    return AlertDialog(
      title: Text(widget.conversationId == null ? '历史全局规则审阅' : '对话提议审阅'),
      content: SizedBox(
        width: 1000,
        height: MediaQuery.sizeOf(context).height * .75,
        child: LayoutBuilder(
          builder: (context, constraints) {
            final wide = constraints.maxWidth >= 720;
            final listPane = _buildListPane(count);
            final detailPane = _buildDetailPane();
            if (wide) {
              return Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SizedBox(width: 400, child: listPane),
                  const VerticalDivider(width: 1),
                  Expanded(child: detailPane),
                ],
              );
            }
            // 窄屏：详情打开时页内切换，不叠对话框。
            final showDetail = !_history && (_viewing != null || _viewingBatch);
            return showDetail ? detailPane : listPane;
          },
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

  Widget _buildDetailPane() {
    if (_viewingBatch) {
      final selected = _proposals
          .where((p) => _selected.contains('${p.kind}:${p.id}'))
          .toList();
      return ChatProposalBatchPanel(
        proposals: selected,
        onClose: () => _afterReview(true),
      );
    }
    final proposal = _viewing;
    if (proposal == null) {
      return const Center(child: Text('选择左侧一条提议查看详情'));
    }
    return ChatProposalDetailPanel(
      key: ValueKey('${proposal.kind}:${proposal.id}:${proposal.version}'),
      proposal: proposal,
      onClose: () => _afterReview(_changed),
    );
  }

  Widget _buildListPane(int count) {
    return Column(
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
                                    _ReceiptText(receipt.result),
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
                    final key = '${proposal.kind}:${proposal.id}';
                    final isViewing =
                        _viewing != null &&
                        '${_viewing!.kind}:${_viewing!.id}' == key;
                    return ListTile(
                      key: ValueKey(key),
                      selected: isViewing,
                      leading: Checkbox(
                        key: ValueKey('select-${proposal.kind}-${proposal.id}'),
                        value: _selected.contains(key),
                        onChanged: (selected) => setState(() {
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
        // 正文按 markdown 渲染（知识页/规则/记录多为 md），不再用纯文本
        if (body != null) MarkdownView(markdown: '$body'),
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
