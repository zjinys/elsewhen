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
        if (!mounted) return;
        message = '${_statusLabel(context, receipt.status)}：${receipt.result}';
      } catch (e) {
        if (!mounted) return;
        message = context.l10n.batchUnconfirmed(e.toString());
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
        Text(context.l10n.batchReviewDescription),
        for (final proposal in _widgetProposals) ...[
          const Divider(),
          Text(
            context.l10n.proposalVersion(
              _actionLabel(context, proposal.action),
              proposal.version.toString(),
            ),
            style: Theme.of(context).textTheme.titleMedium,
          ),
          _ProposalContent(argsJson: proposal.argsJson),
          if (proposal.targetBasisJson != '[]')
            ExpansionTile(
              title: Text(context.l10n.targetVersionBasis),
              children: [SelectableText(proposal.targetBasisJson)],
            ),
          if (_results['${proposal.kind}:${proposal.id}'] case final result?)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: _ReceiptText(result),
            ),
        ],
        if (_finished) Text(context.l10n.batchFinished),
      ],
    );
    final actions = OverflowBar(
      alignment: MainAxisAlignment.end,
      spacing: 8,
      children: [
        // 面板顶部已有「‹ 返回列表」，底部不再重复；对话框形态仍靠「关闭」。
        if (panelClose == null)
          TextButton(
            onPressed: _busy
                ? null
                : () {
                    Navigator.maybePop(context, _finished);
                  },
            child: Text(context.l10n.close),
          ),
        if (!_finished) ...[
          TextButton(
            onPressed: _busy ? null : () => _apply(false),
            child: Text(context.l10n.rejectThese),
          ),
          FilledButton(
            onPressed:
                _busy || _widgetProposals.any((p) => _args(p.argsJson) == null)
                ? null
                : () => _apply(true),
            child: Text(
              _busy ? context.l10n.processingItems : context.l10n.acceptThese,
            ),
          ),
        ],
      ],
    );
    if (isPanel) {
      return PopScope(
        // 系统返回＝「返回列表」，同详情面板；列表态才关 sheet。
        canPop: false,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop && !_busy && panelClose != null) panelClose();
        },
        child: Column(
          children: [
            // 顶部「返回列表」：与详情面板一致，移动端 sheet 需要
            // 明显的返回入口（只靠底部按钮不可发现）。
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 4, 16, 0),
              child: Row(
                children: [
                  if (panelClose != null)
                    TextButton.icon(
                      onPressed: _busy ? null : panelClose,
                      icon: const Icon(Icons.arrow_back_ios_new, size: 15),
                      label: Text(context.l10n.backToList),
                    ),
                  const Spacer(),
                  Text(
                    context.l10n.reviewSelectedTitle(_widgetProposals.length),
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ],
              ),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(16),
                child: body,
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
        title: Text(context.l10n.reviewSelectedTitle(_widgetProposals.length)),
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
