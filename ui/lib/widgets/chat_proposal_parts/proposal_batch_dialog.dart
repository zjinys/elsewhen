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
        // 面板顶部已有「‹ 返回列表」，底部不再重复；对话框形态仍靠「关闭」。
        if (panelClose == null)
          TextButton(
            onPressed: _busy
                ? null
                : () {
                    Navigator.maybePop(context, _finished);
                  },
            child: const Text('关闭'),
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
                      label: const Text('返回列表'),
                    ),
                  const Spacer(),
                  Text(
                    '审阅选中的 ${_widgetProposals.length} 项',
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
