part of '../chat_proposal_review.dart';

//! 批量处置对话框

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
                    child: _ReceiptText(result),
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
