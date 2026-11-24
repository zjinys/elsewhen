part of '../personal_memory_screen.dart';

// ignore_for_file: invalid_use_of_protected_member

extension _MemoryReview on _PersonalMemoryScreenState {
  Future<void> _change(Map<String, dynamic> args) async {
    final detail = _detail;
    if (_busy || detail == null) return;
    setState(() {
      _busy = true;
      _working = true;
    });
    try {
      final claim = _memoryMap(detail['claim']);
      final proposal = await _repo.preparePersonalMemoryAction(
        claimId: claim['id'] as String,
        expectedVersion: claim['version'] as int,
        detailToken: detail['detail_token'] as String,
        argsJson: jsonEncode(args),
      );
      if (!mounted) return;
      setState(() => _working = false);
      await showDialog<bool>(
        context: context,
        barrierDismissible: false,
        builder: (_) => _MemoryApprovalDialog(repo: _repo, proposal: proposal),
      );
    } catch (error) {
      if (mounted) showAppError(context, context.l10n.memoryManager, error);
    } finally {
      if (mounted) {
        setState(() {
          _busy = false;
          _working = false;
        });
        await _load();
      }
    }
  }

  Future<void> _confirmMemory(Map<String, dynamic> claim) async {
    var scope = claim['scope'] as String;
    final selected = await showDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: Text(context.l10n.memoryConfirm),
          content: SizedBox(
            width: 440,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(context.l10n.memoryReviewHelp),
                const SizedBox(height: 16),
                DropdownButtonFormField<String>(
                  initialValue: scope,
                  decoration: InputDecoration(
                    labelText: context.l10n.memoryScope,
                  ),
                  items: [
                    for (final value in [
                      'reference',
                      'conversation',
                      'personal',
                    ])
                      DropdownMenuItem(
                        value: value,
                        child: Text(_memoryLabel(context, value)),
                      ),
                  ],
                  onChanged: (value) {
                    if (value != null) setDialogState(() => scope = value);
                  },
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, scope),
              child: Text(context.l10n.confirm),
            ),
          ],
        ),
      ),
    );
    if (selected != null && mounted) {
      await _change({
        'operation': 'confirm',
        'scope': selected,
        if (claim['valid_from'] != null) 'valid_from': claim['valid_from'],
        if (claim['valid_until'] != null) 'valid_until': claim['valid_until'],
      });
    }
  }

  Future<void> _replaceMemory(Map<String, dynamic> claim) async {
    final replacement = await showDialog<String>(
      context: context,
      builder: (_) => _MemoryReplacementDialog(repo: _repo, claim: claim),
    );
    if (replacement != null && mounted) {
      await _change({
        'operation': 'supersede',
        'replacement_id': replacement,
        'scope': claim['scope'],
        if (claim['valid_from'] != null) 'valid_from': claim['valid_from'],
        if (claim['valid_until'] != null) 'valid_until': claim['valid_until'],
      });
    }
  }
}

class _MemoryApprovalDialog extends StatefulWidget {
  const _MemoryApprovalDialog({required this.repo, required this.proposal});
  final RustBridgeRepository repo;
  final api.ChatProposalDto proposal;

  @override
  State<_MemoryApprovalDialog> createState() => _MemoryApprovalDialogState();
}

class _MemoryApprovalDialogState extends State<_MemoryApprovalDialog> {
  bool _busy = false, _failed = false;
  Object? _error;

  Future<void> _decide(bool accept) async {
    if (_busy) return;
    if (_failed) {
      Navigator.pop(context, false);
      return;
    }
    setState(() => _busy = true);
    try {
      final proposal = widget.proposal;
      final receipt = await widget.repo.decidePersonalMemoryAction(
        api.ChatReviewRequest(
          kind: proposal.kind,
          proposalId: proposal.id,
          conversationId: proposal.conversationId,
          expectedVersion: proposal.version,
          approvalToken: proposal.approvalToken,
        ),
        accept: accept,
      );
      if (accept && receipt.status != 'succeeded') {
        throw StateError(receipt.result);
      }
      if (mounted) Navigator.pop(context, accept);
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error;
          _failed = true;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final args = _memoryMap(jsonDecode(widget.proposal.argsJson));
    final basis = _memoryMap(args['reviewed_basis']);
    final claim = _memoryMap(basis['claim']);
    final replacement = basis['replacement'] as Map?;
    final goal = basis['goal'] as Map?;
    return PopScope(
      canPop: false,
      child: AlertDialog(
        title: Text(context.l10n.memoryReviewTitle),
        content: SizedBox(
          width: 640,
          child: SingleChildScrollView(
            child: SelectionArea(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    _memoryLabel(context, args['operation'] as String),
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  const SizedBox(height: 8),
                  Text(
                    '${basis['title'] ?? claim['page_slug']} · ${context.l10n.memoryVersion} ${claim['version']}',
                  ),
                  Text('${claim['subject']} · ${claim['predicate']}'),
                  Text(_memoryLabel(context, basis['validity'] as String)),
                  Text(
                    '${context.l10n.memoryScope}: ${_memoryLabel(context, (args['scope'] ?? claim['scope']) as String)}',
                  ),
                  if (args['salience'] != null)
                    Text(
                      '${context.l10n.memorySalience}: ${_memoryLabel(context, args['salience'] as String)}',
                    ),
                  if (args['goal_id'] != null)
                    Text(
                      '${context.l10n.memoryLinkGoal}: ${goal?['goal']?['title'] ?? args['goal_id']}',
                    ),
                  if (args['reason'] != null) Text(args['reason'] as String),
                  if (args['valid_from'] != null)
                    Text('≥ ${args['valid_from']}'),
                  if (args['valid_until'] != null)
                    Text('< ${args['valid_until']}'),
                  const SizedBox(height: 16),
                  Text(
                    basis['content_excerpt'] as String? ??
                        context.l10n.memoryNoContent,
                  ),
                  if (basis['content_excerpt'] != null &&
                      basis['complete'] != true)
                    Text(context.l10n.memoryTruncated),
                  if (replacement != null) ...[
                    const Divider(),
                    Text(
                      context.l10n.memorySupersede,
                      style: Theme.of(context).textTheme.titleSmall,
                    ),
                    Text(
                      '${replacement['claim']['page_slug']} · ${context.l10n.memoryVersion} ${replacement['claim']['version']}',
                    ),
                    Text(
                      replacement['content_excerpt'] as String? ??
                          context.l10n.memoryNoContent,
                    ),
                    if (replacement['complete'] != true)
                      Text(context.l10n.memoryTruncated),
                  ],
                  const Divider(),
                  Text(
                    context.l10n.memorySources,
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  for (final source in _memoryRows(claim['basis']))
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: Text(
                        '${source['kind']} · ${source['id']}\n${source['quote'] ?? context.l10n.memorySourceHidden}',
                      ),
                    ),
                  const SizedBox(height: 16),
                  if (args['operation'] == 'delete')
                    Text(context.l10n.memoryDeleteHelp),
                  Text(context.l10n.memoryReviewHelp),
                  if (_error != null) ...[
                    const SizedBox(height: 12),
                    Text(context.l10n.memoryReviewFailed),
                    AppErrorMessage(
                      action: context.l10n.memoryManager,
                      error: _error!,
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: _busy ? null : () => _decide(false),
            child: Text(_failed ? context.l10n.close : context.l10n.cancel),
          ),
          FilledButton(
            key: const ValueKey('memory-approve'),
            onPressed: _busy || _failed ? null : () => _decide(true),
            child: _busy
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : Text(context.l10n.memoryApprove),
          ),
        ],
      ),
    );
  }
}
