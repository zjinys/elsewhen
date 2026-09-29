import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import 'markdown_view.dart';
import '../utils/error_report.dart';

/// 异常出口：诊断进日志，界面只见通用文案。
///
/// [context] 是中文短动作标签，只进日志和通用文案，不放细节——
/// 异常原文（SQL 语句、数据库绝对路径、provider 返回体）一律不进界面。
///
/// [showDetail] 逐站点显式开：只给「Rust 侧这条路径只 bail 可读领域文案」
/// 的站点用（见 [reportUiError] 的说明）。目前全仓只有两处：保存 Provider
/// 的三条表单校验、处理提案的来源变更提示。
String _error(Object error, {String context = '操作', bool showDetail = false}) =>
    reportUiError(context, error, showDetail: showDetail);

Future<void> showKnowledgeText(
  BuildContext context,
  String title,
  String text,
) => showDialog<void>(
  context: context,
  builder: (context) => AlertDialog(
    title: Text(title),
    content: SizedBox(
      width: 760,
      child: SingleChildScrollView(
        child: SelectionArea(child: MarkdownView(markdown: text)),
      ),
    ),
  ),
);

Future<void> _openPage(BuildContext context, WidgetRef ref, String slug) async {
  try {
    final page = await ref.read(wikiPageProvider(slug).future);
    if (!context.mounted) return;
    if (page == null) throw StateError('页面不存在');
    openWikiPageTab(ref, page);
    ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
    Navigator.of(context).pop();
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(_error(error, context: '打开知识页'))));
    }
  }
}

Future<void> _showEvent(BuildContext context, WidgetRef ref, String id) async {
  try {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    final event = await repo.getEventAnalysisDetail(id);
    if (!context.mounted) return;
    await showKnowledgeText(context, '来源记录', event?.rawText ?? '来源记录不存在');
  } catch (error) {
    if (context.mounted) {
      await showKnowledgeText(
        context,
        '读取失败',
        _error(error, context: '读取来源记录'),
      );
    }
  }
}

void _refresh(WidgetRef ref, String? slug) {
  if (slug != null) {
    ref.invalidate(knowledgeDetailsProvider(slug));
    ref.invalidate(wikiPageProvider(slug));
    ref.invalidate(knowledgeRevisionsProvider(slug));
  }
  ref.invalidate(wikiPagesProvider);
  ref.invalidate(knowledgeProposalsProvider);
  ref.invalidate(knowledgeIssuesProvider);
}

class KnowledgePageActions extends ConsumerStatefulWidget {
  final WikiPage page;
  const KnowledgePageActions({super.key, required this.page});
  @override
  ConsumerState<KnowledgePageActions> createState() =>
      _KnowledgePageActionsState();
}

class _KnowledgePageActionsState extends ConsumerState<KnowledgePageActions> {
  bool _busy = false;
  Future<void> _propose(String kind) async {
    setState(() => _busy = true);
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .propose(widget.page.slug, kind);
      if (!mounted) return;
      _refresh(ref, widget.page.slug);
      await showDialog<void>(
        context: context,
        builder: (_) => KnowledgePageDialog(page: widget.page, initialTab: 1),
      );
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(_error(error, context: '打开知识页'))),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final material = ['source', 'note'].contains(widget.page.kind);
    return Wrap(
      spacing: 4,
      children: [
        TextButton.icon(
          onPressed: () => showDialog<void>(
            context: context,
            builder: (_) => KnowledgePageDialog(page: widget.page),
          ),
          icon: const Icon(Icons.source_outlined, size: 16),
          label: const Text('来源与修订'),
        ),
        if (_busy)
          const Padding(padding: EdgeInsets.all(10), child: Text('正在整理建议…'))
        else if (material)
          PopupMenuButton<String>(
            tooltip: '提炼知识',
            onSelected: _propose,
            itemBuilder: (_) => const [
              PopupMenuItem(value: 'method', child: Text('提炼方法')),
              PopupMenuItem(value: 'case', child: Text('整理案例')),
              PopupMenuItem(value: 'principle', child: Text('提炼规律')),
            ],
            child: const Padding(
              padding: EdgeInsets.all(10),
              child: Text('提炼知识 ▾'),
            ),
          )
        else
          TextButton.icon(
            onPressed: () => _propose('revision'),
            icon: const Icon(Icons.fact_check_outlined, size: 16),
            label: const Text('审阅本页'),
          ),
      ],
    );
  }
}

class KnowledgePageDialog extends ConsumerWidget {
  final WikiPage page;
  final int initialTab;
  const KnowledgePageDialog({
    super.key,
    required this.page,
    this.initialTab = 0,
  });
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final details = ref.watch(knowledgeDetailsProvider(page.slug));
    return Dialog(
      child: SizedBox(
        width: 860,
        height: MediaQuery.sizeOf(context).height * .85,
        child: Column(
          children: [
            ListTile(
              title: Text(page.title),
              subtitle: const Text('原始来源、待审建议与修改历史'),
              trailing: IconButton(
                tooltip: '关闭',
                onPressed: () => Navigator.pop(context),
                icon: const Icon(Icons.close),
              ),
            ),
            Expanded(
              child: details.when(
                loading: () => const Center(child: CircularProgressIndicator()),
                error: (error, _) =>
                    Center(child: Text(_error(error, context: '读取来源记录'))),
                data: (data) => DefaultTabController(
                  length: 3,
                  initialIndex: initialTab,
                  child: Column(
                    children: [
                      TabBar(
                        tabs: [
                          const Tab(text: '来源'),
                          Tab(
                            text:
                                '待审建议 ${data.proposals.where((p) => p.status == 'pending').length}',
                          ),
                          const Tab(text: '修订历史'),
                        ],
                      ),
                      Expanded(
                        child: TabBarView(
                          children: [
                            ListView(
                              padding: const EdgeInsets.all(16),
                              children: [
                                if (data.sources.isEmpty &&
                                    page.sourceEventIds.isEmpty)
                                  const Text('尚无可核验来源。'),
                                for (final source in data.sources)
                                  _SourceTile(source: source),
                                for (final id in page.sourceEventIds)
                                  ListTile(
                                    title: const Text('来源记录'),
                                    subtitle: Text(id),
                                    onTap: () => _showEvent(context, ref, id),
                                  ),
                                if (['source', 'note'].contains(page.kind))
                                  _OpinionControl(
                                    page: page,
                                    opinion: data.sources.firstOrNull?.opinion,
                                  ),
                                if ([
                                  'method',
                                  'case',
                                  'principle',
                                ].contains(page.kind))
                                  _MetadataEditor(
                                    slug: page.slug,
                                    metadata: data.metadata,
                                  ),
                                if (data.history.length > 1) ...[
                                  const Divider(),
                                  const Text('原文版本'),
                                  for (final source in data.history)
                                    _SourceTile(source: source),
                                ],
                                for (final issue in data.issues)
                                  _IssueTile(issue: issue),
                              ],
                            ),
                            ListView(
                              padding: const EdgeInsets.all(16),
                              children: [
                                if (data.proposals.isEmpty)
                                  const Text('暂无建议。提炼或审阅结果会先放在这里，确认后才写入知识页。'),
                                for (final proposal in data.proposals)
                                  KnowledgeProposalCard(
                                    proposal: proposal,
                                    sourceSlug: page.slug,
                                  ),
                              ],
                            ),
                            _RevisionList(slug: page.slug),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SourceTile extends StatelessWidget {
  final api.SourceSnapshot source;
  const _SourceTile({required this.source});
  @override
  Widget build(BuildContext context) => ListTile(
    contentPadding: EdgeInsets.zero,
    leading: const Icon(Icons.description_outlined),
    title: Text('${source.title} · v${source.version}'),
    subtitle: Text('${source.locator ?? '粘贴文本'}\n${source.capturedAt}'),
    isThreeLine: true,
    onTap: () => showKnowledgeText(
      context,
      '${source.title} · v${source.version}',
      source.contentMd,
    ),
  );
}

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
  Future<void> _resolve(bool accept) async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      final saved = await ref
          .read(knowledgeRepositoryProvider)
          .resolve(widget.proposal.id, accept);
      if (!mounted) return;
      _refresh(ref, widget.sourceSlug);
      _refresh(ref, saved?.slug ?? widget.proposal.targetSlug);
      if (saved != null) openWikiPageTab(ref, WikiPage.fromDto(saved));
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
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(p.title, style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            Text(p.reason),
            if (p.applicableWhen.isNotEmpty) Text('适用条件：${p.applicableWhen}'),
            if (p.pageId != null)
              TextButton(
                onPressed: () async {
                  try {
                    final page = await ref.read(
                      wikiPageProvider(p.targetSlug).future,
                    );
                    if (context.mounted) {
                      await showKnowledgeText(
                        context,
                        '当前正文',
                        page?.contentMd ?? '页面不存在',
                      );
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
                child: const Text('对照当前正文'),
              ),
            const SizedBox(height: 8),
            SelectionArea(child: MarkdownView(markdown: p.contentMd)),
            Wrap(
              spacing: 4,
              children: [
                for (final id in p.snapshotIds)
                  TextButton(
                    onPressed: () async {
                      try {
                        final snapshot = await ref
                            .read(knowledgeRepositoryProvider)
                            .snapshot(id);
                        if (context.mounted && snapshot != null) {
                          await showKnowledgeText(
                            context,
                            '${snapshot.title} · v${snapshot.version}',
                            snapshot.contentMd,
                          );
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
                    child: const Text('查看原料'),
                  ),
                for (final id in p.eventIds)
                  TextButton(
                    onPressed: () => _showEvent(context, ref, id),
                    child: const Text('查看事件'),
                  ),
              ],
            ),
            if (_failure != null)
              Text(
                _failure!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            if (p.status == 'pending')
              Wrap(
                spacing: 8,
                children: [
                  FilledButton(
                    onPressed: _busy ? null : () => _resolve(true),
                    child: Text(_busy ? '处理中…' : '确认保存'),
                  ),
                  TextButton(
                    onPressed: _busy ? null : () => _resolve(false),
                    child: const Text('拒绝建议'),
                  ),
                ],
              )
            else
              Text(p.status == 'accepted' ? '已确认保存' : '已拒绝；保留决定记录'),
          ],
        ),
      ),
    );
  }
}

class _IssueTile extends ConsumerWidget {
  final api.KnowledgeIssue issue;
  const _IssueTile({required this.issue});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ListTile(
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
  );
}

class _RevisionList extends ConsumerWidget {
  final String slug;
  const _RevisionList({required this.slug});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(knowledgeRevisionsProvider(slug))
      .when(
        error: (error, _) =>
            Center(child: Text(_error(error, context: '读取修订记录'))),
        loading: () => const Center(child: CircularProgressIndicator()),
        data: (items) => ListView(
          children: [
            for (final revision in items)
              ListTile(
                title: Text(revision.reason),
                subtitle: Text(revision.createdAt),
                onTap: () => showKnowledgeText(
                  context,
                  revision.reason,
                  revision.contentMd,
                ),
              ),
          ],
        ),
      );
}

class _OpinionControl extends ConsumerWidget {
  final WikiPage page;
  final String? opinion;
  const _OpinionControl({required this.page, this.opinion});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Wrap(
    spacing: 8,
    children: [
      const Padding(padding: EdgeInsets.all(8), child: Text('材料评价')),
      for (final value in ['endorse', 'reject'])
        ChoiceChip(
          label: Text(value == 'endorse' ? '认可' : '不认可'),
          selected: opinion == value,
          onSelected: (_) async {
            try {
              await api.setWikiOpinion(
                slug: page.slug,
                opinion: opinion == value ? null : value,
              );
              if (context.mounted) _refresh(ref, page.slug);
            } catch (error) {
              if (context.mounted) {
                await showKnowledgeText(
                  context,
                  '操作失败',
                  _error(error, context: '记录观点'),
                );
              }
            }
          },
        ),
    ],
  );
}

class _MetadataEditor extends ConsumerStatefulWidget {
  final String slug;
  final api.KnowledgeMetadata metadata;
  const _MetadataEditor({required this.slug, required this.metadata});
  @override
  ConsumerState<_MetadataEditor> createState() => _MetadataEditorState();
}

class _MetadataEditorState extends ConsumerState<_MetadataEditor> {
  late final _conditions = TextEditingController(
    text: widget.metadata.applicableWhen,
  );
  late String _strength = widget.metadata.strength;
  bool _busy = false;
  String? _failure;
  @override
  void dispose() {
    _conditions.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_strength == 'rule' && widget.metadata.strength != 'rule') {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('确认作为规则使用？'),
          content: const Text('AI 会在符合适用条件时对照这条规则。请确认内容可靠且适用条件准确。'),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('确认升级'),
            ),
          ],
        ),
      );
      if (confirmed != true || !mounted) return;
    }
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .metadata(widget.slug, _conditions.text, _strength);
      if (mounted) _refresh(ref, widget.slug);
    } catch (error) {
      if (mounted) setState(() => _failure = _error(error, context: '保存元数据'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        TextField(
          controller: _conditions,
          maxLines: 3,
          maxLength: 600,
          decoration: const InputDecoration(labelText: '适用条件'),
        ),
        DropdownButton<String>(
          value: _strength,
          onChanged: _busy
              ? null
              : (value) => setState(() => _strength = value!),
          items: const [
            DropdownMenuItem(value: 'reference', child: Text('参考材料')),
            DropdownMenuItem(value: 'method', child: Text('可复用方法')),
            DropdownMenuItem(value: 'rule', child: Text('已确认规则')),
          ],
        ),
        if (_failure != null) Text(_failure!),
        FilledButton(
          onPressed: _busy ? null : _save,
          child: const Text('保存适用条件与强度'),
        ),
      ],
    ),
  );
}

class KnowledgeMaintenanceButton extends StatelessWidget {
  const KnowledgeMaintenanceButton({super.key});
  @override
  Widget build(BuildContext context) => TextButton.icon(
    icon: const Icon(Icons.fact_check_outlined, size: 18),
    label: const Text('知识审阅'),
    onPressed: () => showDialog<void>(
      context: context,
      builder: (_) => const _MaintenanceDialog(),
    ),
  );
}

class _MaintenanceDialog extends ConsumerWidget {
  const _MaintenanceDialog();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final proposals = ref.watch(knowledgeProposalsProvider);
    final issues = ref.watch(knowledgeIssuesProvider);
    final runs = ref.watch(knowledgeRunsProvider);
    return Dialog(
      child: SizedBox(
        width: 860,
        height: MediaQuery.sizeOf(context).height * .85,
        child: Column(
          children: [
            ListTile(
              title: const Text('知识审阅'),
              subtitle: const Text('建议确认后生效；来源变化保留已有正文'),
              trailing: IconButton(
                onPressed: () => Navigator.pop(context),
                icon: const Icon(Icons.close),
              ),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.all(16),
                children: [
                  proposals.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取提案')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('暂无待审建议。'),
                        for (final p in items)
                          KnowledgeProposalCard(proposal: p),
                      ],
                    ),
                  ),
                  const Divider(),
                  const Text('来源检查'),
                  issues.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取检查项')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('未发现来源问题。'),
                        for (final issue in items) _IssueTile(issue: issue),
                      ],
                    ),
                  ),
                  const Divider(),
                  const Text('自动洞察运行记录'),
                  runs.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取洞察记录')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('积累事件后由后台自动检查，无需手动触发。'),
                        for (final run in items)
                          ListTile(
                            title: Text(
                              run.status == 'succeeded'
                                  ? '完成 · ${run.resultCount} 条洞察'
                                  : run.status == 'running'
                                  ? '处理中'
                                  : '失败，稍后自动重试',
                            ),
                            subtitle: Text(
                              '${run.startedAt}${run.error == null ? '' : '\n${run.error}'}',
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class KnowledgeCitationsButton extends ConsumerWidget {
  final String messageId;
  const KnowledgeCitationsButton({super.key, required this.messageId});
  @override
  Widget build(BuildContext context, WidgetRef ref) => TextButton.icon(
    icon: const Icon(Icons.source_outlined, size: 14),
    label: const Text('引用依据'),
    onPressed: () async {
      try {
        final citations = await ref
            .read(knowledgeRepositoryProvider)
            .citations(messageId);
        if (!context.mounted) return;
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('本条回答实际引用的知识'),
            content: SizedBox(
              width: 680,
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (citations.isEmpty) const Text('这条回答没有经过系统核验的知识引用。'),
                    for (final citation in citations) ...[
                      TextButton(
                        onPressed: () =>
                            _openPage(context, ref, citation.pageSlug),
                        child: Text(citation.title),
                      ),
                      Text('${citation.category} · ${citation.reason}'),
                      if (citation.applicableWhen.isNotEmpty)
                        Text('适用条件：${citation.applicableWhen}'),
                      SelectionArea(child: Text(citation.excerpt)),
                      for (final source in citation.sources)
                        TextButton(
                          child: Text('${source.title} · v${source.version}'),
                          onPressed: () async {
                            try {
                              final snapshot = await ref
                                  .read(knowledgeRepositoryProvider)
                                  .snapshot(source.snapshotId);
                              if (context.mounted) {
                                await showKnowledgeText(
                                  context,
                                  source.title,
                                  snapshot?.contentMd ?? '来源版本不存在',
                                );
                              }
                            } catch (error) {
                              if (context.mounted) {
                                await showKnowledgeText(
                                  context,
                                  '读取失败',
                                  _error(error),
                                );
                              }
                            }
                          },
                        ),
                      for (final id in citation.eventIds)
                        TextButton(
                          onPressed: () => _showEvent(context, ref, id),
                          child: const Text('查看来源事件'),
                        ),
                      const Divider(),
                    ],
                  ],
                ),
              ),
            ),
          ),
        );
      } catch (error) {
        if (context.mounted) {
          await showKnowledgeText(
            context,
            '读取失败',
            _error(error, context: '读取来源事件'),
          );
        }
      }
    },
  );
}
