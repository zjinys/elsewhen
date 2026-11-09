part of '../chat_proposal_review.dart';

//! 方案评审对话框（逐项批注/提交）

class ChatProposalReviewDialog extends ConsumerStatefulWidget {
  const ChatProposalReviewDialog({
    super.key,
    required this.conversationId,
    this.embedded = false,
  });
  final String? conversationId;
  final bool embedded;

  /// The shell chooses a desktop companion panel or a modal sheet. Embedded
  /// content shares the chat route and must never pop that route.
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
  // 处理记录：右栏展示的回执。
  api.ChatDecisionDto? _viewingReceipt;

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

  void _reviewReceipt(api.ChatDecisionDto receipt) => setState(() {
    _viewingReceipt = receipt;
    _viewing = null;
    _viewingBatch = false;
  });

  Future<void> _afterReview(bool? changed) async {
    if (!mounted) return;
    _changed = _changed || changed == true;
    _offset = 0;
    _viewing = null;
    _viewingBatch = false;
    _viewingReceipt = null;
    ref.invalidate(pendingActionsProvider);
    ref.invalidate(todosProvider);
    ref.invalidate(wikiPagesProvider);
    ref.invalidate(conversationsProvider);
    ref.invalidate(activeGoalsProvider);
    ref.invalidate(archivedGoalsProvider);
    ref.invalidate(pausedGoalsProvider);
    ref.invalidate(completedGoalsProvider);
    ref.invalidate(_hasPendingProvider(widget.conversationId));
    await _load();
  }

  /// Shared list/detail content for companion panels and mobile sheets.
  Widget _buildPanes(int count) {
    final detailOpen = _history
        ? _viewingReceipt != null
        : _viewing != null || _viewingBatch;
    final panes = LayoutBuilder(
      builder: (context, constraints) {
        final listPane = _buildListPane(count);
        final detailPane = _buildDetailPane();
        if (constraints.maxWidth >= 720) {
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(width: 400, child: listPane),
              const VerticalDivider(width: 1),
              Expanded(child: detailPane),
            ],
          );
        }
        return detailOpen ? detailPane : listPane;
      },
    );
    // A companion panel shares the main route. It must never pop that route.
    if (widget.embedded) return panes;
    return PopScope(
      // 详情/回执打开时，系统返回（Android 手势/返回键）＝回列表，
      // 不冒泡关掉整个审阅面板；列表态才允许 pop 关 sheet。
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        if (detailOpen) {
          setState(() {
            _viewing = null;
            _viewingBatch = false;
            _viewingReceipt = null;
          });
        } else {
          Navigator.of(context).pop();
        }
      },
      child: panes,
    );
  }

  @override
  Widget build(BuildContext context) {
    final count = _history ? _decisions.length : _proposals.length;
    // 呈现骨架（面板壳）由外部的 showPanelSheet 提供，这里只出内容。
    return _buildPanes(count);
  }

  Widget _buildDetailPane() {
    if (_history) {
      final receipt = _viewingReceipt;
      if (receipt == null) {
        return Center(child: Text(context.l10n.selectReceipt));
      }
      // 处理记录详情：顶部带「返回列表」（详情/批量面板同款）；
      // 此前这条内联路径没有任何返回入口，手机上无法回到列表。
      return Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(4, 4, 16, 0),
            child: Row(
              children: [
                TextButton.icon(
                  onPressed: () => setState(() {
                    _viewingReceipt = null;
                  }),
                  icon: const Icon(Icons.arrow_back_ios_new, size: 15),
                  label: Text(context.l10n.backToList),
                ),
                const Spacer(),
                Text(
                  '${_actionLabel(context, receipt.action)} · ${_statusLabel(context, receipt.status)}',
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
                children: [
                  _ReceiptText(receipt.result),
                  const SizedBox(height: 12),
                  _ProposalContent(argsJson: receipt.argsJson),
                ],
              ),
            ),
          ),
        ],
      );
    }
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
      return Center(child: Text(context.l10n.selectProposal));
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
              label: Text(context.l10n.pendingReview),
              selected: !_history,
              onSelected: _loading
                  ? null
                  : (_) {
                      setState(() {
                        _history = false;
                        _offset = 0;
                        _viewing = null;
                        _viewingBatch = false;
                        _viewingReceipt = null;
                      });
                      _load();
                    },
            ),
            ChoiceChip(
              label: Text(context.l10n.decisionHistory),
              selected: _history,
              onSelected: _loading
                  ? null
                  : (_) {
                      setState(() {
                        _history = true;
                        _offset = 0;
                        _viewing = null;
                        _viewingBatch = false;
                        _viewingReceipt = null;
                      });
                      _load();
                    },
            ),
            IconButton(
              tooltip: context.l10n.refresh,
              onPressed: _loading ? null : _load,
              icon: const Icon(Icons.refresh),
            ),
          ],
        ),
        if (!_history && _selected.isNotEmpty)
          TextButton.icon(
            onPressed: _loading ? null : _reviewSelected,
            icon: const Icon(Icons.checklist),
            label: Text(context.l10n.reviewSelectedCount(_selected.length)),
          ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.all(12),
            child: AppErrorMessage(
              action: context.l10n.load,
              error: _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        Expanded(
          child: _loading
              ? const Center(child: CircularProgressIndicator())
              : count == 0
              ? Center(
                  child: Text(
                    _history
                        ? context.l10n.noDecisions
                        : context.l10n.noProposals,
                  ),
                )
              : ListView.builder(
                  itemCount: count,
                  itemBuilder: (context, index) {
                    if (_history) {
                      return _receiptTile(_decisions[index]);
                    }
                    return _proposalTile(_proposals[index]);
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
              child: Text(context.l10n.previousPage),
            ),
            Text(context.l10n.pageNumber(_offset ~/ _pageSize + 1)),
            TextButton(
              onPressed: _loading || count < _pageSize
                  ? null
                  : () {
                      _offset += _pageSize;
                      _load();
                    },
              child: Text(context.l10n.nextPage),
            ),
          ],
        ),
      ],
    );
  }

  Widget _proposalTile(api.ChatProposalDto proposal) {
    final args = _args(proposal.argsJson);
    final key = '${proposal.kind}:${proposal.id}';
    final isViewing =
        _viewing != null && '${_viewing!.kind}:${_viewing!.id}' == key;
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      elevation: 0,
      color: isViewing
          ? theme.colorScheme.primaryContainer
          : theme.colorScheme.surfaceContainerHighest,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => _review(proposal),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Row(
            children: [
              Checkbox(
                key: ValueKey('select-${proposal.kind}-${proposal.id}'),
                value: _selected.contains(key),
                visualDensity: VisualDensity.compact,
                onChanged: (selected) => setState(() {
                  if (selected == true) {
                    _selected.add(key);
                  } else {
                    _selected.remove(key);
                  }
                }),
              ),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '${args?['title'] ?? args?['content'] ?? args?['intent_text'] ?? (proposal.action == 'update_goal_facets' ? context.l10n.updateGoalById('${args?['id']}') : _actionLabel(context, proposal.action))}',
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      context.l10n.proposalVersion(
                        _actionLabel(context, proposal.action),
                        proposal.version.toString(),
                      ),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    // 创建时间（相对时间）：不然列表里分不清哪个新哪个旧。
                    if (prettyTimestamp(
                          proposal.createdAt,
                          strings: context.l10n,
                        )
                        case final String timeLabel)
                      Text(
                        timeLabel,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                  ],
                ),
              ),
              Icon(
                Icons.chevron_right,
                size: 18,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _receiptTile(api.ChatDecisionDto receipt) {
    final theme = Theme.of(context);
    final key = '${receipt.kind}:${receipt.id}';
    final isViewing =
        _viewingReceipt != null &&
        '${_viewingReceipt!.kind}:${_viewingReceipt!.id}' == key;
    final (icon, color) = _statusIcon(receipt.status, theme);
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      elevation: 0,
      color: isViewing
          ? theme.colorScheme.primaryContainer
          : theme.colorScheme.surfaceContainerHighest,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => _reviewReceipt(receipt),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Row(
            children: [
              Icon(icon, size: 18, color: color),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '${_actionLabel(context, receipt.action)} · ${_statusLabel(context, receipt.status)}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      receipt.result,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    // 处理时间（相对时间）：与待审列表同样的新旧判断依据。
                    if (prettyTimestamp(
                          receipt.startedAt,
                          strings: context.l10n,
                        )
                        case final String timeLabel)
                      Text(
                        timeLabel,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                  ],
                ),
              ),
              Icon(
                Icons.chevron_right,
                size: 18,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }

  (IconData, Color) _statusIcon(String status, ThemeData theme) =>
      switch (status) {
        'succeeded' => (Icons.check_circle_outline, theme.colorScheme.primary),
        'failed' => (Icons.error_outline, theme.colorScheme.error),
        'declined' => (
          Icons.cancel_outlined,
          theme.colorScheme.onSurfaceVariant,
        ),
        'uncertain' => (Icons.help_outline, theme.colorScheme.tertiary),
        _ => (Icons.hourglass_empty, theme.colorScheme.onSurfaceVariant),
      };
}

class _ProposalContent extends StatelessWidget {
  const _ProposalContent({required this.argsJson});
  final String argsJson;
  @override
  Widget build(BuildContext context) {
    final args = _args(argsJson);
    if (args == null) {
      return SelectableText(context.l10n.invalidProposal(argsJson));
    }

    // P2 计划拆解草案：专属渲染（意图原文 / 评估 / 假设清单 / 里程碑步骤），
    // 不再走通用正文 + 参数 JSON 的路径。
    if (args.containsKey('intent_text') && args.containsKey('steps')) {
      return _PlanDraftCard(args: args);
    }

    final hasGoalFacets =
        args.containsKey('why') ||
        args.containsKey('current_focus') ||
        args.containsKey('blocker') ||
        args.containsKey('done_criteria') ||
        args.containsKey('phase');

    final body = args['content_md'] ?? args['content'] ?? args['raw_text'];
    final rest = Map<String, dynamic>.from(args)
      ..remove('title')
      ..remove('content_md')
      ..remove('content')
      ..remove('raw_text');

    if (hasGoalFacets) {
      rest.remove('phase');
      rest.remove('why');
      rest.remove('current_focus');
      rest.remove('blocker');
      rest.remove('done_criteria');
      rest.remove('id');
    }

    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (args['title'] != null)
          SelectableText(
            '${args['title']}',
            style: theme.textTheme.titleMedium,
          ),
        if (args['content'] != null && hasGoalFacets)
          SelectableText(
            '${args['content']}',
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.bold,
            ),
          ),
        if (body != null && !hasGoalFacets) MarkdownView(markdown: '$body'),
        if (hasGoalFacets) ...[
          const SizedBox(height: 8),
          _GoalFacetsCard(args: args),
        ],
        if (rest.isNotEmpty) ...[
          const SizedBox(height: 12),
          ExpansionTile(
            title: Text(context.l10n.parametersSources),
            initiallyExpanded: body == null && !hasGoalFacets,
            children: [
              SelectableText(const JsonEncoder.withIndent('  ').convert(rest)),
            ],
          ),
        ],
      ],
    );
  }
}

class _GoalFacetsCard extends StatelessWidget {
  const _GoalFacetsCard({required this.args});
  final Map<String, dynamic> args;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final phase = args['phase']?.toString();
    final why = args['why']?.toString().trim();
    final focus = args['current_focus']?.toString().trim();
    final blocker = args['blocker']?.toString().trim();
    final done = args['done_criteria']?.toString().trim();
    final id = args['id']?.toString();

    String phaseLabel(String? p) => switch (p) {
      'mid' => context.l10n.midTermGoal,
      'long' => context.l10n.longTermGoal,
      _ => context.l10n.nearTermGoal,
    };

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          color: theme.colorScheme.outlineVariant.withValues(alpha: 0.5),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (phase != null || id != null)
            Row(
              children: [
                if (phase != null)
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.primaryContainer,
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      phaseLabel(phase),
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.onPrimaryContainer,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                if (id != null) ...[
                  const SizedBox(width: 8),
                  Text(
                    context.l10n.goalId(id),
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ],
            ),
          if (focus != null && focus.isNotEmpty) ...[
            const SizedBox(height: 8),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('📍 ', style: TextStyle(fontSize: 13)),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      style: theme.textTheme.bodyMedium,
                      children: [
                        TextSpan(
                          text: context.l10n.currentActionLabel,
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(text: focus),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ],
          if (blocker != null && blocker.isNotEmpty) ...[
            const SizedBox(height: 6),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('🚧 ', style: TextStyle(fontSize: 13)),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      style: theme.textTheme.bodyMedium,
                      children: [
                        TextSpan(
                          text: context.l10n.currentObstacleLabel,
                          style: TextStyle(
                            fontWeight: FontWeight.bold,
                            color: theme.colorScheme.error,
                          ),
                        ),
                        TextSpan(text: blocker),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ],
          if (why != null && why.isNotEmpty) ...[
            const SizedBox(height: 6),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('💡 ', style: TextStyle(fontSize: 13)),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      style: theme.textTheme.bodyMedium,
                      children: [
                        TextSpan(
                          text: context.l10n.motivationLabel,
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(text: why),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ],
          if (done != null && done.isNotEmpty) ...[
            const SizedBox(height: 6),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('🎯 ', style: TextStyle(fontSize: 13)),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      style: theme.textTheme.bodyMedium,
                      children: [
                        TextSpan(
                          text: context.l10n.completionCriteriaLabel,
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(text: done),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

/// P2 计划拆解草案的专属渲染：意图原文 / 现实性评估 / 假设清单 / 里程碑步骤。
/// `create_plan` 与 `add_plan_steps` 的 args 同形（intent_text + steps）。
class _PlanDraftCard extends StatelessWidget {
  const _PlanDraftCard({required this.args});
  final Map<String, dynamic> args;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final intent = args['intent_text']?.toString().trim() ?? '';
    final vision = args['vision']?.toString().trim();
    final assessment = args['assessment']?.toString().trim();
    final planIntent = args['plan_intent']?.toString();
    final assumptions = (args['assumptions'] as List?) ?? const [];
    final steps = (args['steps'] as List?) ?? const [];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (planIntent != null)
          SelectableText(
            context.l10n.planLabel(planIntent),
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        if (intent.isNotEmpty)
          SelectableText(
            context.l10n.intentLabel(intent),
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.bold,
            ),
          ),
        if (vision != null && vision.isNotEmpty) ...[
          const SizedBox(height: 6),
          SelectableText(context.l10n.visionLabel(vision)),
        ],
        if (assessment != null && assessment.isNotEmpty) ...[
          const SizedBox(height: 12),
          Text(
            context.l10n.feasibilityAssessment,
            style: theme.textTheme.labelLarge,
          ),
          const SizedBox(height: 4),
          SelectableText(assessment),
        ],
        if (assumptions.isNotEmpty) ...[
          const SizedBox(height: 12),
          Text(context.l10n.assumptions, style: theme.textTheme.labelLarge),
          const SizedBox(height: 4),
          for (final assumption in assumptions.whereType<Map>())
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Text.rich(
                TextSpan(
                  style: theme.textTheme.bodyMedium,
                  children: [
                    TextSpan(
                      text:
                          '[${assumption['kind'] ?? context.l10n.assumption}] ',
                      style: const TextStyle(fontWeight: FontWeight.bold),
                    ),
                    TextSpan(text: '${assumption['text'] ?? ''}'),
                  ],
                ),
              ),
            ),
        ],
        if (steps.isNotEmpty) ...[
          const SizedBox(height: 12),
          Text(context.l10n.milestoneSteps, style: theme.textTheme.labelLarge),
          const SizedBox(height: 4),
          for (var i = 0; i < steps.length; i++)
            if (steps[i] is Map) _stepTile(context, i, steps[i] as Map),
        ],
      ],
    );
  }

  Widget _stepTile(BuildContext context, int index, Map step) {
    final theme = Theme.of(context);
    final done = step['done_criteria']?.toString().trim();
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Text.rich(
        TextSpan(
          style: theme.textTheme.bodyMedium,
          children: [
            TextSpan(
              text: '${index + 1}. ',
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
            TextSpan(text: '${step['content'] ?? ''}'),
            if (done != null && done.isNotEmpty)
              TextSpan(
                text: context.l10n.criterionLabel(done),
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
          ],
        ),
      ),
    );
  }
}
