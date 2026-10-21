part of '../chat_proposal_review.dart';

//! 批量处置对话框

/// 批量 dialog 与 panel 的共同基类，使两者能共用同一个 State。
abstract class ChatProposalBatchBase extends ConsumerStatefulWidget {
  const ChatProposalBatchBase({super.key, required this.proposals});
  final List<api.ChatProposalDto> proposals;
}

class ChatProposalBatchDialog extends ChatProposalBatchBase {
  const ChatProposalBatchDialog({super.key, required super.proposals});
  @override
  ConsumerState<ChatProposalBatchBase> createState() =>
      _ChatProposalBatchDialogState();
}

/// 批量面板（不带 dialog 壳），供主从布局右栏内嵌。
class ChatProposalBatchPanel extends ChatProposalBatchBase {
  const ChatProposalBatchPanel({
    super.key,
    required super.proposals,
    this.onClose,
  });
  final VoidCallback? onClose;
  @override
  ConsumerState<ChatProposalBatchBase> createState() =>
      _ChatProposalBatchDialogState();
}

class _ChatProposalBatchDialogState
    extends ConsumerState<ChatProposalBatchBase> {
  List<api.ChatProposalDto> get _widgetProposals => widget.proposals;
  VoidCallback? get _panelOnClose => widget is ChatProposalBatchPanel
      ? (widget as ChatProposalBatchPanel).onClose
      : null;
  final Map<String, String> _results = {};
  bool _busy = false, _finished = false;

  Future<void> _apply(bool accept) async {
    if (_busy || _finished) return;
    setState(() => _busy = true);
    final repo = ref.read(conversationRepositoryProvider);
    for (final proposal in _widgetProposals) {
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
  Widget build(BuildContext context) {
    final isPanel = widget is ChatProposalBatchPanel;
    final panelClose = _panelOnClose;
    final body = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('仅处理下面列出的具体版本。逐项执行，不是整批原子提交；可能部分成功。每项都会显示结果。'),
        for (final proposal in _widgetProposals) ...[
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
          if (_results['${proposal.kind}:${proposal.id}'] case final result?)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: _ReceiptText(result),
            ),
        ],
        if (_finished) const Text('本次逐项处理结束，请查看每项结果。'),
      ],
    );
    final actions = OverflowBar(
      alignment: MainAxisAlignment.end,
      spacing: 8,
      children: [
        TextButton(
          onPressed: _busy
              ? null
              : () {
                  if (panelClose != null) {
                    panelClose();
                  } else {
                    Navigator.maybePop(context, _finished);
                  }
                },
          child: Text(panelClose != null ? '返回列表' : '关闭'),
        ),
        if (!_finished) ...[
          TextButton(
            onPressed: _busy ? null : () => _apply(false),
            child: const Text('拒绝这些项'),
          ),
          FilledButton(
            onPressed:
                _busy || _widgetProposals.any((p) => _args(p.argsJson) == null)
                ? null
                : () => _apply(true),
            child: Text(_busy ? '逐项处理中…' : '接受这些项'),
          ),
        ],
      ],
    );
    if (isPanel) {
      return PopScope(
        canPop: !_busy,
        child: Column(
          children: [
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '审阅选中的 ${_widgetProposals.length} 项',
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                    const SizedBox(height: 8),
                    body,
                  ],
                ),
              ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              child: actions,
            ),
          ],
        ),
      );
    }
    return PopScope(
      canPop: !_busy,
      child: AlertDialog(
        title: Text('审阅选中的 ${_widgetProposals.length} 项'),
        content: SizedBox(
          width: 660,
          height: MediaQuery.sizeOf(context).height * .6,
          child: SingleChildScrollView(child: body),
        ),
        actions: [actions],
      ),
    );
  }
}
