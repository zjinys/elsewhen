part of '../knowledge_panel.dart';

//! 知识提案卡片与问题条目。

class KnowledgeProposalCard extends ConsumerStatefulWidget {
  final api.KnowledgeProposal proposal;
  final String? sourceSlug;
  const KnowledgeProposalCard({
    super.key,
    required this.proposal,
    this.sourceSlug,
  });
  @override
  ConsumerState<KnowledgeProposalCard> createState() =>
      _KnowledgeProposalCardState();
}

class _KnowledgeProposalCardState extends ConsumerState<KnowledgeProposalCard> {
  bool _busy = false;
  String? _failure;
  String? _comparison;
  String? _comparisonTitle;
  bool _resolved = false;
  List<api.KnowledgeDiffPart>? _parts;
  final Set<int> _selectedParts = {};
  final Set<String> _resolvedIssues = {};
  List<api.KnowledgeIssue> _reviewIssues = [];
  bool _acceptApplicability = true;
  Future<void> _loadDiff() async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      final repository = ref.read(knowledgeRepositoryProvider);
      final parts = await repository.diff(widget.proposal.id);
      final issues = await repository.issues();
      if (!mounted) return;
      setState(() {
        _parts = parts;
        _selectedParts.addAll([
          for (var i = 0; i < parts.length; i++)
            if (parts[i].changed) i,
        ]);
        _reviewIssues = issues
            .where((i) => i.pageSlug == widget.proposal.targetSlug)
            .toList();
      });
    } catch (error) {
      if (mounted) {
        setState(
          () => _failure = _error(error, context: '读取差异', showDetail: true),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _resolve(bool accept) async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      final repository = ref.read(knowledgeRepositoryProvider);
      final saved = accept && _parts != null
          ? await repository.acceptParts(
              widget.proposal.id,
              _selectedParts.toList()..sort(),
              _acceptApplicability,
              _resolvedIssues.toList(),
            )
          : await repository.resolve(widget.proposal.id, accept);
      if (!mounted) return;
      _refresh(ref, widget.sourceSlug);
      _refresh(ref, saved?.slug ?? widget.proposal.targetSlug);
      setState(() => _resolved = true);
      // Stay with the source. Its output section exposes the saved page explicitly.
    } catch (error) {
      // resolve() 的失败是领域结论（来源已更新/被拒绝 → 请重新生成建议），
      // 用户必须看到具体是哪一条，否则只能反复点确认。
      if (mounted) {
        setState(
          () => _failure = _error(error, context: '处理提案', showDetail: true),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.proposal;
    final isPending = p.status == 'pending' && !_resolved;
    return Container(
      margin: const EdgeInsets.only(bottom: 12),
      child: Material(
        color: AppTheme.surface1,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(
            color: isPending
                ? AppTheme.accentPrimary.withValues(alpha: 0.35)
                : AppTheme.surface3,
          ),
        ),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Text(
                      p.title,
                      style: const TextStyle(
                        fontSize: 14.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: _resolved
                          ? AppTheme.surface3
                          : (p.status == 'pending'
                                ? AppTheme.accentPrimary.withValues(alpha: 0.12)
                                : AppTheme.surface3),
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Text(
                      _resolved
                          ? '已处理'
                          : (p.status == 'pending' ? '待审阅' : p.status),
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: _resolved
                            ? AppTheme.textTertiary
                            : (p.status == 'pending'
                                  ? AppTheme.accentPrimary
                                  : AppTheme.textSecondary),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                p.reason,
                style: TextStyle(
                  fontSize: 12.5,
                  color: AppTheme.textSecondary,
                  height: 1.45,
                ),
              ),
              if (p.applicableWhen.isNotEmpty) ...[
                const SizedBox(height: 4),
                Text(
                  '适用条件：${p.applicableWhen}',
                  style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
                ),
              ],
              if (p.pageId != null)
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: TextButton.icon(
                    icon: const Icon(Icons.compare_outlined, size: 15),
                    label: const Text('对照当前正文', style: TextStyle(fontSize: 12)),
                    onPressed: () async {
                      try {
                        final page = await ref.read(
                          wikiPageProvider(p.targetSlug).future,
                        );
                        if (context.mounted) {
                          setState(() {
                            _comparisonTitle = '当前正文';
                            _comparison = page?.contentMd ?? '页面不存在';
                          });
                        }
                      } catch (error) {
                        if (context.mounted) {
                          await showKnowledgeText(
                            context,
                            '读取失败',
                            _error(error, context: '读取页面正文'),
                          );
                        }
                      }
                    },
                  ),
                ),
              if (p.status == 'pending')
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: TextButton.icon(
                    onPressed: _busy ? null : _loadDiff,
                    icon: const Icon(Icons.difference_outlined, size: 15),
                    label: const Text('逐段审阅', style: TextStyle(fontSize: 12)),
                  ),
                ),
              if (_parts == null)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: SelectionArea(
                    child: MarkdownView(markdown: p.contentMd),
                  ),
                )
              else ...[
                const SizedBox(height: 6),
                for (var i = 0; i < _parts!.length; i++)
                  if (_parts![i].changed)
                    Container(
                      margin: const EdgeInsets.only(bottom: 8),
                      child: Material(
                        color: AppTheme.surface2,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(8),
                          side: BorderSide(
                            color: _selectedParts.contains(i)
                                ? AppTheme.accentPrimary.withValues(alpha: 0.4)
                                : AppTheme.surface3,
                          ),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.all(12),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              CheckboxListTile(
                                contentPadding: EdgeInsets.zero,
                                title: Text(
                                  '采纳修改 ${i + 1}',
                                  style: const TextStyle(
                                    fontSize: 13,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                value: _selectedParts.contains(i),
                                onChanged: _busy
                                    ? null
                                    : (checked) => setState(() {
                                        if (checked == true) {
                                          _selectedParts.add(i);
                                        } else {
                                          _selectedParts.remove(i);
                                        }
                                      }),
                              ),
                              if (_parts![i].before.isNotEmpty) ...[
                                Text(
                                  '原段落',
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    fontWeight: FontWeight.w600,
                                    color: AppTheme.textTertiary,
                                  ),
                                ),
                                Container(
                                  width: double.infinity,
                                  margin: const EdgeInsets.symmetric(
                                    vertical: 4,
                                  ),
                                  padding: const EdgeInsets.all(8),
                                  decoration: BoxDecoration(
                                    color: AppTheme.surface1,
                                    borderRadius: BorderRadius.circular(6),
                                  ),
                                  child: SelectionArea(
                                    child: MarkdownView(
                                      markdown: _parts![i].before,
                                    ),
                                  ),
                                ),
                              ],
                              if (_parts![i].after.isNotEmpty) ...[
                                Text(
                                  '建议段落',
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    fontWeight: FontWeight.w600,
                                    color: AppTheme.textTertiary,
                                  ),
                                ),
                                Container(
                                  width: double.infinity,
                                  margin: const EdgeInsets.symmetric(
                                    vertical: 4,
                                  ),
                                  padding: const EdgeInsets.all(8),
                                  decoration: BoxDecoration(
                                    color: AppTheme.surface1,
                                    borderRadius: BorderRadius.circular(6),
                                  ),
                                  child: SelectionArea(
                                    child: MarkdownView(
                                      markdown: _parts![i].after,
                                    ),
                                  ),
                                ),
                              ] else
                                Padding(
                                  padding: const EdgeInsets.symmetric(
                                    vertical: 4,
                                  ),
                                  child: Text(
                                    '删除这一段',
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: AppTheme.textTertiary,
                                    ),
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ),
                    )
                  else
                    Padding(
                      padding: const EdgeInsets.all(8),
                      child: SelectionArea(
                        child: MarkdownView(markdown: _parts![i].after),
                      ),
                    ),
                CheckboxListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text(
                    '同时采纳建议的适用条件',
                    style: TextStyle(fontSize: 13),
                  ),
                  subtitle: Text(
                    p.applicableWhen,
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  value: _acceptApplicability,
                  onChanged: _busy
                      ? null
                      : (v) =>
                            setState(() => _acceptApplicability = v ?? false),
                ),
                if (_reviewIssues.isNotEmpty)
                  const Padding(
                    padding: EdgeInsets.only(top: 4, bottom: 2),
                    child: Text(
                      '本次修订解决了哪些提示（由你确认）：',
                      style: TextStyle(fontSize: 12.5),
                    ),
                  ),
                for (final issue in _reviewIssues)
                  CheckboxListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(
                      issue.description,
                      style: const TextStyle(fontSize: 13),
                    ),
                    value: _resolvedIssues.contains(issue.fingerprint),
                    onChanged: _busy
                        ? null
                        : (v) => setState(() {
                            if (v == true) {
                              _resolvedIssues.add(issue.fingerprint);
                            } else {
                              _resolvedIssues.remove(issue.fingerprint);
                            }
                          }),
                  ),
                Padding(
                  padding: const EdgeInsets.only(top: 4, bottom: 8),
                  child: Text(
                    '如果来源依据同时变化，需要完整确认；局部采纳只用于来源不变的修订。',
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
              ],
              Wrap(
                spacing: 4,
                children: [
                  for (final id in p.snapshotIds)
                    TextButton.icon(
                      icon: const Icon(Icons.visibility_outlined, size: 14),
                      label: const Text('查看原料', style: TextStyle(fontSize: 12)),
                      onPressed: () async {
                        try {
                          final snapshot = await ref
                              .read(knowledgeRepositoryProvider)
                              .snapshot(id);
                          if (context.mounted && snapshot != null) {
                            setState(() {
                              _comparisonTitle =
                                  '${snapshot.title} · v${snapshot.version}';
                              _comparison = snapshot.contentMd;
                            });
                          }
                        } catch (error) {
                          if (context.mounted) {
                            await showKnowledgeText(
                              context,
                              '读取失败',
                              _error(error, context: '读取素材快照'),
                            );
                          }
                        }
                      },
                    ),
                  for (final id in p.eventIds)
                    TextButton.icon(
                      icon: const Icon(Icons.event_note_outlined, size: 14),
                      label: const Text('查看事件', style: TextStyle(fontSize: 12)),
                      onPressed: () => _showEvent(context, ref, id),
                    ),
                ],
              ),
              if (_comparison != null)
                Container(
                  margin: const EdgeInsets.symmetric(vertical: 8),
                  decoration: BoxDecoration(
                    color: AppTheme.surface2,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: AppTheme.surface3),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                _comparisonTitle!,
                                style: const TextStyle(
                                  fontSize: 13,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                            IconButton(
                              tooltip: '收起对照',
                              icon: const Icon(Icons.close, size: 16),
                              onPressed: () =>
                                  setState(() => _comparison = null),
                            ),
                          ],
                        ),
                        const Divider(height: 12),
                        SelectionArea(
                          child: MarkdownView(markdown: _comparison!),
                        ),
                      ],
                    ),
                  ),
                ),
              if (_failure != null)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    _failure!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                      fontSize: 12.5,
                    ),
                  ),
                ),
              if (p.status == 'pending' && !_resolved)
                if (p.pageId != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Text(
                      '请对照当前正文。确认会应用以上修改；若本页是个人规则，也表示你确认修订后的规则及适用条件。',
                      style: TextStyle(
                        fontSize: 12,
                        color: AppTheme.textTertiary,
                        height: 1.45,
                      ),
                    ),
                  ),
              if (p.status == 'pending' && !_resolved)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Wrap(
                    spacing: 8,
                    runSpacing: 6,
                    children: [
                      FilledButton(
                        onPressed: _busy ? null : () => _resolve(true),
                        child: Text(
                          _busy
                              ? '处理中…'
                              : p.pageId == null
                              ? '确认保存'
                              : '确认修订',
                        ),
                      ),
                      TextButton(
                        onPressed: _busy ? null : () => _resolve(false),
                        child: const Text('拒绝建议'),
                      ),
                    ],
                  ),
                )
              else
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    _resolved
                        ? '已处理，原文保持不变'
                        : p.status == 'accepted'
                        ? '已保存参考知识'
                        : '已拒绝；保留决定记录',
                    style: TextStyle(
                      fontSize: 12.5,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _IssueTile extends ConsumerWidget {
  final api.KnowledgeIssue issue;
  const _IssueTile({required this.issue});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Column(
    children: [
      ListTile(
        contentPadding: EdgeInsets.zero,
        title: Text(issue.description),
        subtitle: Text(issue.pageSlug),
        trailing: TextButton(
          child: const Text('忽略提示'),
          onPressed: () async {
            try {
              await ref
                  .read(knowledgeRepositoryProvider)
                  .dismiss(issue.fingerprint);
              if (context.mounted) _refresh(ref, issue.pageSlug);
            } catch (error) {
              if (context.mounted) {
                await showKnowledgeText(
                  context,
                  '操作失败',
                  _error(error, context: '忽略检查项'),
                );
              }
            }
          },
        ),
        onTap: () => _openPage(context, ref, issue.pageSlug),
      ),
      if (['conflict', 'outdated', 'duplicate'].contains(issue.kind))
        KnowledgeIssueResolutionSection(
          issue: issue,
          onChanged: () => _refresh(ref, issue.pageSlug),
        ),
    ],
  );
}
