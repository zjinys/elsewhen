import 'knowledge_workflows.dart';
import 'topic_organization_section.dart';
import 'knowledge_maintenance_sections.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:url_launcher/url_launcher.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../providers/knowledge_provider.dart';
import '../providers/wiki_provider.dart';
import 'markdown_view.dart';
import 'wiki_derivatives.dart';
import '../utils/error_report.dart';
import '../theme/app_theme.dart';

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

/// 产出 tab 内各小节的统一节标题样式（待审建议 / 已整理的参考知识 / 产物版本）。
Widget _outputsSectionTitle(BuildContext context, String text, {int? count}) =>
    Padding(
      padding: const EdgeInsets.only(top: 10, bottom: 6),
      child: Row(
        children: [
          Text(
            text,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: AppTheme.textSecondary,
              letterSpacing: 0.3,
            ),
          ),
          if (count != null) ...[
            const SizedBox(width: 6),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: AppTheme.surface3,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Text(
                '$count',
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
            ),
          ],
        ],
      ),
    );

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
    if (ModalRoute.of(context) is PopupRoute) Navigator.of(context).pop();
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(_error(error, context: '打开知识页'))));
    }
  }
}

/// D2 跳转已随事件域入口一并隐藏（需求复盘后入口撤下，跳转到隐藏域
/// 会成黑洞）；回退为只读 dialog 展示事件原文，待事件域重新定义后再接。
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

/// RFC3339 时间戳 → 简短可读格式（`MM-dd HH:mm`，当年省年份）。
/// 解析失败（非标准格式）时原样返回，不丟信息。
String _shortTime(String rfc3339) {
  final dt = DateTime.tryParse(rfc3339)?.toLocal();
  if (dt == null) return rfc3339;
  String two(int n) => n.toString().padLeft(2, '0');
  final now = DateTime.now();
  final date = dt.year == now.year
      ? '${two(dt.month)}-${two(dt.day)}'
      : '${dt.year}-${two(dt.month)}-${two(dt.day)}';
  return '$date ${two(dt.hour)}:${two(dt.minute)}';
}

void _refresh(WidgetRef ref, String? slug) {
  if (slug != null) {
    ref.invalidate(knowledgeDetailsProvider(slug));
    ref.invalidate(wikiPageProvider(slug));
    ref.invalidate(knowledgeRevisionsProvider(slug));
  }
  ref.invalidate(knowledgeRevisionProvider);
  ref.invalidate(wikiPagesProvider);
  ref.invalidate(knowledgeProposalsProvider);
  ref.invalidate(knowledgeIssuesProvider);
}

class KnowledgePageActions extends ConsumerStatefulWidget {
  final WikiPage page;
  final VoidCallback onShowSources;
  final VoidCallback onShowOutputs;
  final bool showSourcesButton;
  const KnowledgePageActions({
    super.key,
    required this.page,
    required this.onShowSources,
    required this.onShowOutputs,
    this.showSourcesButton = true,
  });
  @override
  ConsumerState<KnowledgePageActions> createState() =>
      _KnowledgePageActionsState();
}

class _KnowledgePageActionsState extends ConsumerState<KnowledgePageActions> {
  bool _busy = false;
  String? _failure;
  String? _lastKind;

  /// 摘录定论（孵化闭环，提案 2026-10-19）：仅 material 且未定论时显示。
  /// endorse 要求已采用派生稿（未采用服务端拒绝并给原因，直接展示）；
  /// reject 弹原因必填对话框。
  Future<void> _decide(String decision, String reason) async {
    setState(() {
      _busy = true;
      _failure = null;
      _lastKind = 'decide:$decision';
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .decideSource(widget.page.slug, decision, reason);
      if (mounted) _refresh(ref, widget.page.slug);
    } catch (error) {
      if (mounted) {
        setState(() => _failure = _error(error, context: '摘录定论', showDetail: true));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _confirmReject() async {
    final controller = TextEditingController();
    final reason = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.surface1,
        title: const Text('丢弃为负知识', style: TextStyle(fontSize: 16)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              '这条摘录为什么不可信/不可用？原因会成为负知识一部分，'
              '下次同源导入会提前提示。',
              style: TextStyle(fontSize: 12),
            ),
            const SizedBox(height: AppTheme.space2),
            TextField(
              controller: controller,
              autofocus: true,
              decoration: const InputDecoration(hintText: '必填：丢弃原因'),
              onSubmitted: (v) => Navigator.of(context).pop(v),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text.trim()),
            child: const Text('丢弃'),
          ),
        ],
      ),
    );
    if (reason == null || reason.isEmpty) return; // 必填：空原因即放弃
    await _decide('reject', reason);
  }

  Future<void> _propose(String kind) async {
    setState(() {
      _busy = true;
      _failure = null;
      _lastKind = kind;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .propose(widget.page.slug, kind);
      if (!mounted) return;
      _refresh(ref, widget.page.slug);
      widget.onShowOutputs();
    } catch (error) {
      if (mounted) {
        setState(() => _failure = _error(error, context: '整理知识'));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final material = ['source', 'note'].contains(widget.page.kind);
    final incubating = isIncubatingPage(widget.page.kind, widget.page.opinion);
    final style = TextButton.styleFrom(
      minimumSize: const Size(0, 36),
      padding: const EdgeInsets.symmetric(horizontal: 10),
    );
    return Wrap(
      spacing: 4,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        if (widget.showSourcesButton)
          TextButton.icon(
            style: style,
            onPressed: widget.onShowSources,
            icon: const Icon(Icons.source_outlined, size: 16),
            label: const Text('来源与修订'),
          ),
        if (_busy)
          const SizedBox(
            height: 36,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
                SizedBox(width: 8),
                Text('正在整理…'),
              ],
            ),
          )
        else if (material)
          PopupMenuButton<String>(
            tooltip: '按需提炼，新增参考知识直接保存',
            onSelected: _propose,
            itemBuilder: (_) => const [
              PopupMenuItem(
                value: 'method',
                child: ListTile(
                  dense: true,
                  title: Text('提炼方法'),
                  subtitle: Text('可复用的步骤与适用条件'),
                ),
              ),
              PopupMenuItem(
                value: 'case',
                child: ListTile(
                  dense: true,
                  title: Text('整理案例'),
                  subtitle: Text('具体情境、做法与结果'),
                ),
              ),
              PopupMenuItem(
                value: 'principle',
                child: ListTile(
                  dense: true,
                  title: Text('提炼规律'),
                  subtitle: Text('规律、依据与适用边界'),
                ),
              ),
            ],
            child: SizedBox(
              height: 36,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      Icons.auto_awesome_outlined,
                      size: 16,
                      color: Theme.of(context).colorScheme.primary,
                    ),
                    const SizedBox(width: 8),
                    Text(
                      '提炼知识',
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                    const SizedBox(width: 4),
                    const Icon(Icons.expand_more, size: 16),
                  ],
                ),
              ),
            ),
          )
        else
          TextButton.icon(
            style: style,
            onPressed: () => _propose('revision'),
            icon: const Icon(Icons.fact_check_outlined, size: 16),
            label: const Text('检查更新'),
          ),
        if (_failure != null) Text(_failure!),
        if (_failure != null && !_busy)
          TextButton.icon(
            onPressed: () => _propose(_lastKind!),
            icon: const Icon(Icons.refresh, size: 16),
            label: const Text('重试整理'),
          ),
        // 孵化定论动作（与「提炼」同区，互不替代：先提炼派生稿，纳
        // 入时服务端校验已采用）。
        if (incubating && !_busy) ...[
          TextButton.icon(
            style: TextButton.styleFrom(
              minimumSize: const Size(0, 36),
              padding: const EdgeInsets.symmetric(horizontal: 10),
            ),
            onPressed: () => _decide('endorse', ''),
            // 服务端校验：未采用派生稿的纳入会被拒并给原因，直接展示。
            icon: const Icon(Icons.verified_outlined, size: 16),
            label: const Text('纳入知识'),
          ),
          TextButton.icon(
            style: TextButton.styleFrom(
              minimumSize: const Size(0, 36),
              padding: const EdgeInsets.symmetric(horizontal: 10),
            ),
            onPressed: _confirmReject,
            icon: const Icon(Icons.block_outlined, size: 16),
            label: const Text('丢弃'),
          ),
        ] else if (widget.page.opinion == 'endorse')
          const _OpinionBadge(label: '已纳入知识', color: Color(0xFF9ECE6A))
        else if (widget.page.opinion == 'reject')
          const _OpinionBadge(label: '已丢弃（负知识）', color: Color(0xFFF7768E)),
      ],
    );
  }
}

/// 定论状态徽标（endorse/reject 已定论的只读展示）。
class _OpinionBadge extends StatelessWidget {
  final String label;
  final Color color;
  const _OpinionBadge({required this.label, required this.color});
  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Icon(Icons.check_circle_outline, size: 14, color: color),
      const SizedBox(width: 4),
      Text(
        label,
        style: TextStyle(fontSize: 11, color: color),
      ),
    ],
  );
}

class _CountBadge extends StatelessWidget {
  final int count;
  const _CountBadge({required this.count});
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
    decoration: BoxDecoration(
      color: AppTheme.surface3,
      borderRadius: BorderRadius.circular(10),
    ),
    child: Text(
      '$count',
      style: TextStyle(
        fontSize: 11,
        fontWeight: FontWeight.w600,
        color: AppTheme.textSecondary,
      ),
    ),
  );
}

/// Provenance is secondary information in the page, not another dialog stack.
class KnowledgeSourcesSection extends ConsumerWidget {
  final WikiPage page;
  const KnowledgeSourcesSection({super.key, required this.page});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return ref
        .watch(knowledgeDetailsProvider(page.slug))
        .when(
          // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view，
          // 防止分析/消化/同步产出的例行重查把面板内容闪成进度条）。
          skipLoadingOnReload: true,
          loading: () => const LinearProgressIndicator(),
          error: (error, _) => Text(_error(error, context: '读取来源与修订')),
          data: (data) => ListView(
            padding: const EdgeInsets.only(bottom: 24),
            children: [
              KnowledgeReadingControl(slug: page.slug),
              Padding(
                padding: const EdgeInsets.only(top: 8, bottom: 14),
                child: Row(
                  children: [
                    Icon(
                      Icons.info_outline,
                      size: 14,
                      color: AppTheme.textTertiary,
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        '来源用于核对依据，修订记录用于查看知识演进。日常阅读和 AI 检索不需要逐项操作。',
                        style: TextStyle(
                          fontSize: 12,
                          color: AppTheme.textTertiary,
                          height: 1.4,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              if (data.sourcePages.isNotEmpty) ...[
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Text(
                    '上游依据',
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
                for (final origin in data.sourcePages)
                  _KnowledgePageLink(
                    page: WikiPage.fromDto(origin),
                    prefix: ['source', 'note'].contains(origin.kind)
                        ? '原料'
                        : '上游知识',
                  ),
                const SizedBox(height: 6),
              ],
              for (var i = 0; i < data.sources.length; i++)
                _SourceTile(
                  source: data.sources[i],
                  page: (i == 0 && ['source', 'note'].contains(page.kind))
                      ? page
                      : null,
                ),
              if (data.sources.isEmpty && page.sourceEventIds.isEmpty)
                Container(
                  margin: const EdgeInsets.only(bottom: 12),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 12,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface1,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: AppTheme.surface3),
                  ),
                  child: Row(
                    children: [
                      Icon(
                        Icons.inbox_outlined,
                        size: 16,
                        color: AppTheme.textTertiary,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        '尚无可核验来源。',
                        style: TextStyle(
                          fontSize: 12.5,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                    ],
                  ),
                ),
              for (final id in page.sourceEventIds)
                Material(
                  color: AppTheme.surface1,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: ListTile(
                    dense: true,
                    leading: Container(
                      padding: const EdgeInsets.all(6),
                      decoration: BoxDecoration(
                        color: AppTheme.surface2,
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Icon(
                        Icons.history_edu_outlined,
                        size: 16,
                        color: AppTheme.accentPrimary,
                      ),
                    ),
                    title: const Text(
                      '来源事件记录',
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    subtitle: Text(
                      id,
                      style: TextStyle(
                        fontSize: 11.5,
                        fontFamily: 'monospace',
                        color: AppTheme.textTertiary,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    trailing: const Icon(Icons.chevron_right, size: 18),
                    onTap: () => _showEvent(context, ref, id),
                  ),
                ),
              if (['source', 'note'].contains(page.kind) &&
                  data.sources.isEmpty)
                Container(
                  margin: const EdgeInsets.only(bottom: 12),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 10,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface1,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: AppTheme.surface3),
                  ),
                  child: _OpinionControl(page: page, opinion: null),
                ),
              if (['method', 'case', 'principle'].contains(page.kind))
                _MetadataEditor(slug: page.slug, metadata: data.metadata),
              for (final issue in data.issues) _IssueTile(issue: issue),
              if (!['source', 'note'].contains(page.kind))
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Material(
                    color: AppTheme.surface1,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(color: AppTheme.surface3),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Theme(
                      data: Theme.of(context)
                          .copyWith(dividerColor: Colors.transparent),
                      child: KnowledgeSourceRepairSection(
                        slug: page.slug,
                        hasEvents: page.sourceEventIds.isNotEmpty,
                        onChanged: () => _refresh(ref, page.slug),
                        previewBuilder: (proposal) => KnowledgeProposalCard(
                          key: ValueKey('repair-${proposal.id}'),
                          proposal: proposal,
                        ),
                      ),
                    ),
                  ),
                ),
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Material(
                  color: AppTheme.surface1,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(10),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: Theme(
                    data: Theme.of(context)
                        .copyWith(dividerColor: Colors.transparent),
                    child: KnowledgeReviewHistorySection(slug: page.slug),
                  ),
                ),
              ),
              if (data.history.length > 1)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Material(
                    color: AppTheme.surface1,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(color: AppTheme.surface3),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Theme(
                      data: Theme.of(context)
                          .copyWith(dividerColor: Colors.transparent),
                      child: ExpansionTile(
                        tilePadding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 4,
                        ),
                        title: Row(
                          children: [
                            const Text(
                              '原文历史版本',
                              style: TextStyle(
                                fontSize: 13.5,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(width: 8),
                            _CountBadge(count: data.history.length),
                          ],
                        ),
                        subtitle: Text(
                          '共有 ${data.history.length} 个历史快照',
                          style: TextStyle(
                            fontSize: 12,
                            color: AppTheme.textTertiary,
                          ),
                        ),
                        children: [
                          Padding(
                            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                            child: Column(
                              children: [
                                for (final source in data.history)
                                  _SourceTile(source: source),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              Consumer(
                builder: (context, ref, _) {
                  final items = ref
                      .watch(knowledgeRevisionsProvider(page.slug))
                      .whenOrNull(data: (items) => items);
                  final count = items?.length;
                  final latest = items?.firstOrNull;
                  return Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Material(
                      color: AppTheme.surface1,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(10),
                        side: BorderSide(color: AppTheme.surface3),
                      ),
                      clipBehavior: Clip.antiAlias,
                      child: Theme(
                        data: Theme.of(context)
                            .copyWith(dividerColor: Colors.transparent),
                        child: ExpansionTile(
                          tilePadding: const EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 4,
                          ),
                          title: Row(
                            children: [
                              const Text(
                                '知识修订记录',
                                style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              if (count != null && count > 0) ...[
                                const SizedBox(width: 8),
                                _CountBadge(count: count),
                              ],
                            ],
                          ),
                          subtitle: Text(
                            latest != null
                                ? '最新：${latest.reason} · ${_shortTime(latest.createdAt)}'
                                : '查看本页正文的历次修订与恢复',
                            style: TextStyle(
                              fontSize: 12,
                              color: AppTheme.textTertiary,
                            ),
                          ),
                          children: [
                            Padding(
                              padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                              child: _RevisionList(
                                slug: page.slug,
                                canRestore: ![
                                  'source',
                                  'note',
                                ].contains(page.kind),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  );
                },
              ),
            ],
          ),
        );
  }
}

class KnowledgeOriginLinks extends ConsumerWidget {
  final String slug;
  const KnowledgeOriginLinks({super.key, required this.slug});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(knowledgeDetailsProvider(slug))
      .when(
        // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view）。
        skipLoadingOnReload: true,
        loading: () => const SizedBox.shrink(),
        error: (_, _) => const Text('来源暂时无法读取'),
        data: (data) => Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final page in data.sourcePages)
              _KnowledgePageLink(
                page: WikiPage.fromDto(page),
                prefix: ['source', 'note'].contains(page.kind) ? '原料' : '上游知识',
              ),
          ],
        ),
      );
}

class _KnowledgePageLink extends ConsumerWidget {
  final WikiPage page;
  final String prefix;
  const _KnowledgePageLink({required this.page, required this.prefix});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Container(
    margin: const EdgeInsets.only(bottom: 8),
    decoration: BoxDecoration(
      color: AppTheme.surface1,
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: AppTheme.surface3),
    ),
    child: InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: () => openWikiPageTab(ref, page),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Row(
          children: [
            Icon(Icons.link_outlined, size: 16, color: AppTheme.accentPrimary),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                '$prefix · ${page.title}',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.textPrimary,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            Icon(Icons.arrow_outward, size: 14, color: AppTheme.textSecondary),
          ],
        ),
      ),
    ),
  );
}

/// Original stays open while proposals and compiled references are inspected.
class KnowledgeOutputsSection extends ConsumerStatefulWidget {
  final WikiPage page;
  const KnowledgeOutputsSection({super.key, required this.page});
  @override
  ConsumerState<KnowledgeOutputsSection> createState() =>
      _KnowledgeOutputsSectionState();
}

class _KnowledgeOutputsSectionState
    extends ConsumerState<KnowledgeOutputsSection> {
  Timer? _poll;
  @override
  void initState() {
    super.initState();
    _poll = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted) ref.invalidate(knowledgeDetailsProvider(widget.page.slug));
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final page = widget.page;
    return ListView(
      padding: const EdgeInsets.only(bottom: 24),
      children: [
        if (page.kind == 'topic')
          TopicOrganizationSection(
            page: page,
            onChanged: () => _refresh(ref, page.slug),
          ),
        if (['source', 'note'].contains(page.kind))
          Padding(
            padding: const EdgeInsets.only(top: 4, bottom: 14),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  Icons.info_outline,
                  size: 14,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '后台会整理参考知识，并结合相关资料维护共同主题。按需提炼的新增产出直接保存；涉及人工修改或已确认内容时才需审阅。原文保留，个人规则由你确认。',
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textTertiary,
                      height: 1.45,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ref
            .watch(knowledgeDetailsProvider(page.slug))
            .when(
              // 知识号 bump 的 reload 不重回 loading（同 wiki_page_detail_view）。
              skipLoadingOnReload: true,
              loading: () => const LinearProgressIndicator(),
              error: (error, _) => Text(_error(error, context: '读取知识产出')),
              data: (data) {
                final pending = data.proposals
                    .where((p) => p.status == 'pending')
                    .toList();
                final outputs = data.outputPages
                    .where((p) => p.area != 'derivative')
                    .toList();
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (pending.isNotEmpty)
                      _outputsSectionTitle(
                        context,
                        '待审建议',
                        count: pending.length,
                      ),
                    for (final proposal in pending)
                      KnowledgeProposalCard(
                        key: ValueKey(proposal.id),
                        proposal: proposal,
                        sourceSlug: page.slug,
                      ),
                    if (outputs.isNotEmpty)
                      _outputsSectionTitle(
                        context,
                        '已整理的参考知识',
                        count: outputs.length,
                      ),
                    for (final output in outputs)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 8),
                        child: Material(
                          color: AppTheme.surface1,
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(10),
                            side: BorderSide(color: AppTheme.surface3),
                          ),
                          child: Theme(
                            data: Theme.of(context)
                                .copyWith(dividerColor: Colors.transparent),
                            child: ExpansionTile(
                              key: PageStorageKey(output.slug),
                              tilePadding: const EdgeInsets.symmetric(
                                horizontal: 14,
                                vertical: 4,
                              ),
                              leading: Container(
                                padding: const EdgeInsets.all(7),
                                decoration: BoxDecoration(
                                  color: AppTheme.surface2,
                                  borderRadius: BorderRadius.circular(8),
                                ),
                                child: Icon(
                                  output.slug.startsWith('knowledge-topic/')
                                      ? Icons.hub_outlined
                                      : Icons.menu_book_outlined,
                                  size: 16,
                                  color: AppTheme.accentPrimary,
                                ),
                              ),
                              title: Text(
                                output.title,
                                style: const TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              subtitle: Padding(
                                padding: const EdgeInsets.only(top: 2),
                                child: Text(
                                  output.slug.startsWith('knowledge-topic/')
                                      ? '跨资料主题 · 包含本页及相关原料'
                                      : '${WikiPage.fromDto(output).kindLabel} · 来自本页原料',
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    color: AppTheme.textTertiary,
                                  ),
                                ),
                              ),
                              children: [
                                Container(
                                  width: double.infinity,
                                  margin: const EdgeInsets.fromLTRB(
                                    10,
                                    0,
                                    10,
                                    10,
                                  ),
                                  decoration: BoxDecoration(
                                    color: AppTheme.surface2,
                                    borderRadius: BorderRadius.circular(8),
                                    border: Border.all(
                                      color: AppTheme.surface3,
                                    ),
                                  ),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Padding(
                                        padding: const EdgeInsets.fromLTRB(
                                          14,
                                          12,
                                          14,
                                          8,
                                        ),
                                        child: SelectionArea(
                                          child: MarkdownView(
                                            markdown: output.contentMd,
                                          ),
                                        ),
                                      ),
                                      Divider(
                                        height: 1,
                                        thickness: 1,
                                        color: AppTheme.surface3,
                                      ),
                                      Padding(
                                        padding: const EdgeInsets.symmetric(
                                          horizontal: 8,
                                          vertical: 4,
                                        ),
                                        child: _KnowledgePageLink(
                                          page: WikiPage.fromDto(output),
                                          prefix: '打开知识页',
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    if (pending.isEmpty && outputs.isEmpty)
                      Container(
                        margin: const EdgeInsets.symmetric(vertical: 8),
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: AppTheme.surface1,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: AppTheme.surface3),
                        ),
                        child: Row(
                          children: [
                            Icon(
                              Icons.auto_stories_outlined,
                              size: 20,
                              color: AppTheme.textTertiary,
                            ),
                            const SizedBox(width: 12),
                            Expanded(
                              child: Text(
                                '暂无知识产出。原文仍可直接使用；后台整理会按内容决定是否形成知识。',
                                style: TextStyle(
                                  fontSize: 12,
                                  color: AppTheme.textSecondary,
                                  height: 1.45,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    KnowledgeReviewHistorySection(slug: page.slug),
                  ],
                );
              },
            ),
        const SizedBox(height: 20),
        WikiDerivatives(slug: page.slug),
      ],
    );
  }
}

class _SourceTile extends StatelessWidget {
  final api.SourceSnapshot source;
  final WikiPage? page;
  const _SourceTile({required this.source, this.page});

  Future<void> _openLocator(String locator) async {
    final uri = Uri.tryParse(locator);
    if (uri != null && (uri.scheme == 'http' || uri.scheme == 'https')) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  @override
  Widget build(BuildContext context) {
    final isUrl =
        source.locator != null &&
        (source.locator!.startsWith('http://') ||
            source.locator!.startsWith('https://'));
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Material(
        color: AppTheme.surface1,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(color: AppTheme.surface3),
        ),
        clipBehavior: Clip.antiAlias,
        child: Theme(
          data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
          child: ExpansionTile(
            tilePadding: const EdgeInsets.symmetric(
              horizontal: 14,
              vertical: 4,
            ),
            leading: Container(
              padding: const EdgeInsets.all(7),
              decoration: BoxDecoration(
                color: AppTheme.surface2,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Icon(
                Icons.article_outlined,
                size: 18,
                color: AppTheme.accentPrimary,
              ),
            ),
            title: Row(
              children: [
                Expanded(
                  child: Text(
                    source.title,
                    style: const TextStyle(
                      fontSize: 13.5,
                      fontWeight: FontWeight.w600,
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 8),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 1,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.surface2,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    'v${source.version}',
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ),
              ],
            ),
            subtitle: Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Row(
                children: [
                  if (source.locator != null) ...[
                    Icon(
                      isUrl ? Icons.link : Icons.notes,
                      size: 13,
                      color: isUrl
                          ? AppTheme.accentPrimary
                          : AppTheme.textTertiary,
                    ),
                    const SizedBox(width: 4),
                    Flexible(
                      child: isUrl
                          ? InkWell(
                              onTap: () => _openLocator(source.locator!),
                              child: Text(
                                source.locator!,
                                style: TextStyle(
                                  fontSize: 11.5,
                                  color: AppTheme.accentPrimary,
                                  decoration: TextDecoration.underline,
                                ),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            )
                          : Text(
                              source.locator!,
                              style: TextStyle(
                                fontSize: 11.5,
                                color: AppTheme.textTertiary,
                              ),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      '·',
                      style: TextStyle(
                        fontSize: 12,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                    const SizedBox(width: 8),
                  ],
                  Text(
                    _shortTime(source.capturedAt),
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ),
            ),
            children: [
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12),
                margin: const EdgeInsets.fromLTRB(12, 0, 12, 10),
                decoration: BoxDecoration(
                  color: AppTheme.surface2,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: SelectionArea(
                  child: MarkdownView(markdown: source.contentMd),
                ),
              ),
              if (page != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
                  child: _OpinionControl(page: page!, opinion: source.opinion),
                ),
            ],
          ),
        ),
      ),
    );
  }
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

class _RevisionList extends ConsumerWidget {
  final String slug;
  final bool canRestore;
  const _RevisionList({required this.slug, this.canRestore = true});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(knowledgeRevisionsProvider(slug))
      .when(
        error: (error, _) =>
            Center(child: Text(_error(error, context: '读取修订记录'))),
        loading: () => const Center(
          child: Padding(
            padding: EdgeInsets.all(16),
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
        data: (items) => ListView(
          shrinkWrap: true,
          physics: const NeverScrollableScrollPhysics(),
          children: [
            for (final revision in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Material(
                  color: AppTheme.surface2,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                    side: BorderSide(color: AppTheme.surface3),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: Theme(
                    data: Theme.of(context)
                        .copyWith(dividerColor: Colors.transparent),
                    child: ExpansionTile(
                      tilePadding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 2,
                      ),
                      leading: Container(
                        padding: const EdgeInsets.all(6),
                        decoration: BoxDecoration(
                          color: AppTheme.surface3,
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: Icon(
                          Icons.history,
                          size: 16,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                      title: Text(
                        revision.reason,
                        style: const TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      subtitle: Text(
                        _shortTime(revision.createdAt),
                        style: TextStyle(
                          fontSize: 11.5,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                      children: [
                        Container(
                          width: double.infinity,
                          padding: const EdgeInsets.all(12),
                          margin: const EdgeInsets.fromLTRB(10, 0, 10, 8),
                          decoration: BoxDecoration(
                            color: AppTheme.surface1,
                            borderRadius: BorderRadius.circular(6),
                          ),
                          child: SelectionArea(
                            child: MarkdownView(markdown: revision.contentMd),
                          ),
                        ),
                        if (canRestore)
                          Padding(
                            padding: const EdgeInsets.only(
                              bottom: 8,
                              left: 10,
                              right: 10,
                            ),
                            child: Align(
                              alignment: Alignment.centerRight,
                              child: FilledButton.tonalIcon(
                                icon: const Icon(Icons.restore, size: 16),
                                label: const Text('生成恢复提案'),
                                style: FilledButton.styleFrom(
                                  visualDensity: VisualDensity.compact,
                                  textStyle: const TextStyle(fontSize: 12),
                                ),
                                onPressed: () async {
                                  try {
                                    await ref
                                        .read(knowledgeRepositoryProvider)
                                        .restore(slug, revision.id);
                                    if (context.mounted) {
                                      _refresh(ref, slug);
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                            const SnackBar(
                                              content: Text(
                                                '恢复提案已放入产出页，确认后生效；当前来源和规则设置保留。',
                                              ),
                                            ),
                                          );
                                    }
                                  } catch (error) {
                                    if (context.mounted) {
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                            SnackBar(
                                              content: Text(
                                                _error(
                                                  error,
                                                  context: '准备恢复',
                                                  showDetail: true,
                                                ),
                                              ),
                                            ),
                                          );
                                    }
                                  }
                                },
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
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
  Widget build(BuildContext context, WidgetRef ref) => Row(
    children: [
      Text(
        '材料评价',
        style: TextStyle(
          fontSize: 12.5,
          fontWeight: FontWeight.w600,
          color: AppTheme.textSecondary,
        ),
      ),
      const SizedBox(width: 10),
      for (final value in ['endorse', 'reject']) ...[
        ChoiceChip(
          label: Text(value == 'endorse' ? '认可' : '不认可'),
          avatar: Icon(
            value == 'endorse'
                ? (opinion == value ? Icons.thumb_up : Icons.thumb_up_outlined)
                : (opinion == value
                      ? Icons.thumb_down
                      : Icons.thumb_down_outlined),
            size: 14,
            color: opinion == value
                ? (value == 'endorse' ? AppTheme.accentPrimary : AppTheme.error)
                : AppTheme.textTertiary,
          ),
          visualDensity: VisualDensity.compact,
          labelStyle: TextStyle(
            fontSize: 12,
            fontWeight: opinion == value ? FontWeight.w600 : FontWeight.w400,
            color: opinion == value
                ? (value == 'endorse' ? AppTheme.accentPrimary : AppTheme.error)
                : AppTheme.textSecondary,
          ),
          selectedColor: value == 'endorse'
              ? AppTheme.accentPrimary.withValues(alpha: 0.15)
              : AppTheme.error.withValues(alpha: 0.15),
          backgroundColor: AppTheme.surface2,
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
        const SizedBox(width: 8),
      ],
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

/// 打开全局「知识审阅」对话框（待确认修改 + 来源检查 + 队列 + 记录）。
/// 供首页入口与按钮共用。
void showKnowledgeMaintenance(BuildContext context) => showDialog<void>(
  context: context,
  builder: (_) => const _MaintenanceDialog(),
);

class KnowledgeMaintenanceButton extends StatelessWidget {
  const KnowledgeMaintenanceButton({super.key});
  @override
  Widget build(BuildContext context) => TextButton.icon(
    icon: const Icon(Icons.fact_check_outlined, size: 18),
    label: const Text('知识审阅'),
    onPressed: () => showKnowledgeMaintenance(context),
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
              subtitle: const Text('后台自动整理参考知识；人工修改与规则确认由你决定'),
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
                        if (items.any(
                          (p) =>
                              p.pageId != null &&
                              p.baseHash != null &&
                              p.status == 'pending',
                        ))
                          KnowledgeBatchReview(
                            proposals: items,
                            preview: (p) => KnowledgeProposalCard(proposal: p),
                          ),
                        if (!items.any((p) => p.status == 'pending'))
                          const Text('暂无需要你确认的修改。新增参考知识已直接保存在产出中。'),
                        for (final p in items.where(
                          (p) => p.status == 'pending',
                        ))
                          KnowledgeProposalCard(proposal: p),
                      ],
                    ),
                  ),
                  const Divider(),
                  const Text('来源与内容检查'),
                  issues.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取检查项')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('暂无待核对提示。'),
                        for (final issue in items) _IssueTile(issue: issue),
                      ],
                    ),
                  ),
                  const Divider(),
                  KnowledgeWorkQueueSection(
                    onOpenPage: (slug) => _openPage(context, ref, slug),
                  ),
                  const Text('最近自动整理与洞察记录'),
                  runs.when(
                    loading: () => const LinearProgressIndicator(),
                    error: (e, _) => Text(_error(e, context: '读取洞察记录')),
                    data: (items) => Column(
                      children: [
                        if (items.isEmpty) const Text('积累事件后由后台自动检查，无需手动触发。'),
                        for (final run in items)
                          ListTile(
                            onTap: run.sourceSlug == null
                                ? null
                                : () =>
                                      _openPage(context, ref, run.sourceSlug!),
                            title: Text(
                              run.status == 'succeeded'
                                  ? '${switch (run.task) {
                                      'source-compilation' => '原料整理',
                                      'source-reading' => '全文阅读',
                                      'knowledge-refresh' => '来源更新检查',
                                      'wiki-integration' => '主题整理与检查',
                                      _ => '洞察',
                                    }}完成 · ${run.resultCount} 项'
                                  : run.status == 'running'
                                  ? '处理中'
                                  : '失败，稍后自动重试',
                            ),
                            subtitle: Text(
                              [
                                if (run.sourceTitle != null)
                                  '${run.sourceTitle}${run.sourceVersion == null ? '' : ' · v${run.sourceVersion}'}',
                                run.startedAt,
                                if (run.detail != null) run.detail!,
                                if (run.error != null) run.error!,
                                if (run.retryAt != null) '自动重试：${run.retryAt}',
                              ].join('\n'),
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
