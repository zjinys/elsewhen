part of '../message_area.dart';

//! 知识草稿对话框、底部面板外壳与草稿编辑弹窗。

class _KnowledgeDraftsDialog extends ConsumerWidget {
  const _KnowledgeDraftsDialog();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final drafts = actions.where(
      (action) => action.action == 'save_knowledge_draft',
    );
    final conversationId = ref.watch(selectedConversationIdProvider);
    return AlertDialog(
      title: Text('待入库草稿（${drafts.length}）'),
      content: SizedBox(
        width: 540,
        height: min(440, MediaQuery.sizeOf(context).height * .55),
        child: drafts.isEmpty
            ? const Center(child: Text('当前对话没有待入库草稿'))
            : ListView(
                children: [
                  for (final action in drafts)
                    Builder(
                      builder: (context) {
                        Map<String, dynamic> payload;
                        try {
                          payload = Map<String, dynamic>.from(
                            jsonDecode(action.argsJson) as Map,
                          );
                        } catch (_) {
                          return const SizedBox.shrink();
                        }
                        final title = payload['title']?.toString() ?? '';
                        return ListTile(
                          leading: const Icon(Icons.article_outlined),
                          title: Text(
                            title,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                          subtitle: const Text('待确认，未入库'),
                          trailing: const Icon(Icons.chevron_right),
                          onTap: conversationId == null
                              ? null
                              : () => showDialog<void>(
                                  context: context,
                                  builder: (_) => _KnowledgeDraftDialog(
                                    conversationId: conversationId,
                                    proposal: action,
                                    title: title,
                                    kind:
                                        payload['kind']?.toString() ?? 'topic',
                                    content:
                                        payload['content_md']?.toString() ?? '',
                                    applicableWhen:
                                        payload['applicable_when']
                                            ?.toString() ??
                                        '',
                                    evidence:
                                        (payload['verified_evidence']
                                                    as List? ??
                                                [])
                                            .map(
                                              (e) =>
                                                  (e as Map)['title']
                                                      ?.toString() ??
                                                  (e)['page_slug']
                                                      ?.toString() ??
                                                  '',
                                            )
                                            .where((s) => s.isNotEmpty)
                                            .toList(),
                                  ),
                                ),
                        );
                      },
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('关闭'),
        ),
      ],
    );
  }
}

/// bottom sheet 通用外壳：拖拽把手 + 标题行 + 内容区。
class _PanelSheet extends StatelessWidget {
  final String title;
  final Widget Function(BuildContext) builder;

  const _PanelSheet({required this.title, required this.builder});

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      top: false,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const _SheetHandle(),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    title,
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
                IconButton(
                  tooltip: '关闭',
                  icon: const Icon(Icons.close, size: 20),
                  color: AppTheme.textSecondary,
                  onPressed: () => Navigator.of(context).pop(),
                ),
              ],
            ),
          ),
          Flexible(
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: MediaQuery.sizeOf(context).height * 0.7,
              ),
              child: builder(context),
            ),
          ),
        ],
      ),
    );
  }
}

class _SheetHandle extends StatelessWidget {
  const _SheetHandle();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 10, bottom: 10),
      child: Container(
        width: 36,
        height: 4,
        decoration: BoxDecoration(
          color: AppTheme.surface3,
          borderRadius: BorderRadius.circular(2),
        ),
      ),
    );
  }
}

/// 待入库草稿的 bottom sheet 版（列表随 DraggableScrollableSheet 伸缩）。
class _KnowledgeDraftsSheet extends ConsumerWidget {
  final ScrollController scrollController;

  const _KnowledgeDraftsSheet({required this.scrollController});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final drafts = actions
        .where((action) => action.action == 'save_knowledge_draft')
        .toList();
    final conversationId = ref.watch(selectedConversationIdProvider);
    return SafeArea(
      top: false,
      child: Column(
        children: [
          const _SheetHandle(),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    '待入库草稿（${drafts.length}）',
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
                IconButton(
                  tooltip: '关闭',
                  icon: const Icon(Icons.close, size: 20),
                  color: AppTheme.textSecondary,
                  onPressed: () => Navigator.of(context).pop(),
                ),
              ],
            ),
          ),
          Expanded(
            child: drafts.isEmpty
                ? const Center(child: Text('当前对话没有待入库草稿'))
                : ListView(
                    controller: scrollController,
                    children: [
                      for (final action in drafts)
                        Builder(
                          builder: (itemContext) {
                            Map<String, dynamic> payload;
                            try {
                              payload = Map<String, dynamic>.from(
                                jsonDecode(action.argsJson) as Map,
                              );
                            } catch (_) {
                              return const SizedBox.shrink();
                            }
                            final title = payload['title']?.toString() ?? '';
                            return ListTile(
                              leading: const Icon(Icons.article_outlined),
                              title: Text(
                                title,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                              ),
                              subtitle: const Text('待确认，未入库'),
                              trailing: const Icon(Icons.chevron_right),
                              onTap: conversationId == null
                                  ? null
                                  : () => showDialog<void>(
                                      context: itemContext,
                                      builder: (_) => _KnowledgeDraftDialog(
                                        conversationId: conversationId,
                                        proposal: action,
                                        title: title,
                                        kind:
                                            payload['kind']?.toString() ??
                                            'topic',
                                        content:
                                            payload['content_md']?.toString() ??
                                            '',
                                        applicableWhen:
                                            payload['applicable_when']
                                                ?.toString() ??
                                            '',
                                        evidence:
                                            (payload['verified_evidence']
                                                        as List? ??
                                                    [])
                                                .map(
                                                  (e) =>
                                                      (e as Map)['title']
                                                          ?.toString() ??
                                                      (e)['page_slug']
                                                          ?.toString() ??
                                                      '',
                                                )
                                                .where((s) => s.isNotEmpty)
                                                .toList(),
                                      ),
                                    ),
                            );
                          },
                        ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

class _KnowledgeDraftDialog extends ConsumerStatefulWidget {
  const _KnowledgeDraftDialog({
    required this.conversationId,
    required this.proposal,
    required this.title,
    required this.kind,
    required this.content,
    required this.applicableWhen,
    required this.evidence,
  });

  final String conversationId;
  final api.PendingActionDto proposal;
  final String title;
  final String kind;
  final String content;
  final String applicableWhen;
  final List<String> evidence;

  @override
  ConsumerState<_KnowledgeDraftDialog> createState() =>
      _KnowledgeDraftDialogState();
}

class _KnowledgeDraftDialogState extends ConsumerState<_KnowledgeDraftDialog> {
  bool _saving = false;
  String? _error;

  Future<void> _delete() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('删除草稿？'),
        content: Text('《${widget.title}》将不再等待入库。已保存的知识页不会受到影响。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('删除草稿'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(conversationRepositoryProvider)
          .declineKnowledgeDraft(widget.conversationId, widget.proposal);
      if (!mounted) return;
      ref.invalidate(pendingActionsProvider);
      Navigator.of(context).pop();
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '删除失败：$e';
          _saving = false;
        });
      }
    }
  }

  Future<void> _save() async {
    if (_saving) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final result = await ref
          .read(conversationRepositoryProvider)
          .confirmKnowledgeDraft(widget.conversationId, widget.proposal);
      if (!mounted) return;
      ref.invalidate(pendingActionsProvider);
      ref.invalidate(wikiPagesProvider);
      final messenger = ScaffoldMessenger.of(context);
      Navigator.of(context).pop();
      messenger.showSnackBar(SnackBar(content: Text(result)));
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '保存失败：$e';
          _saving = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.title),
    content: SizedBox(
      width: 620,
      height: min(480, MediaQuery.sizeOf(context).height * .65),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '类型：${widget.kind} · 待确认，未入库',
            style: TextStyle(color: AppTheme.textSecondary),
          ),
          Text(
            widget.evidence.isEmpty
                ? '自由笔记：未绑定可核验来源'
                : '引用依据：${widget.evidence.join('、')}',
          ),
          if (widget.applicableWhen.isNotEmpty)
            Text('适用条件：${widget.applicableWhen}'),
          const SizedBox(height: 12),
          Expanded(
            child: SingleChildScrollView(
              child: MarkdownView(markdown: widget.content),
            ),
          ),
          if (_error != null)
            Text(
              _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
        ],
      ),
    ),
    actions: [
      TextButton.icon(
        onPressed: _saving ? null : _delete,
        icon: const Icon(Icons.delete_outline),
        label: const Text('删除草稿'),
      ),
      TextButton(
        onPressed: _saving ? null : () => Navigator.of(context).pop(),
        child: const Text('返回'),
      ),
      FilledButton.icon(
        onPressed: _saving ? null : _save,
        icon: _saving
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.save_outlined),
        label: const Text('保存到知识库'),
      ),
    ],
  );
}
