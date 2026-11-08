import '../l10n/processing_result.dart';
import '../l10n/notice_messages.dart';
import 'app_error_notice.dart';
import '../l10n/localization.dart';
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
import '../theme/app_theme.dart';

// ── 知识面板拆分为 part（行为零变化，见 knowledge_panel_parts/）──
part 'knowledge_panel_parts/sources_links.dart';
part 'knowledge_panel_parts/outputs_sources.dart';
part 'knowledge_panel_parts/proposal_card.dart';
part 'knowledge_panel_parts/review_controls.dart';
part 'knowledge_panel_parts/maintenance_buttons.dart';

/// Buffered errors retain the original cause and resolve the UI locale at render.
String _error(
  BuildContext viewContext,
  Object error, {
  String? context,
  bool showDetail = false,
}) => appFailure(error);

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
    if (page == null) throw StateError(context.l10n.l6PageDoesNotExist55c9e);
    openWikiPageTab(ref, page);
    ref.read(sidebarTabProvider.notifier).set(SidebarTab.wiki);
    if (ModalRoute.of(context) is PopupRoute) Navigator.of(context).pop();
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _error(
              context,
              error,
              context: context.l10n.artifactOpenPage,
            ),
          ),
        ),
      );
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
    await showKnowledgeText(
      context,
      context.l10n.l6SourceRecords5bc8d,
      event?.rawText ?? context.l10n.l6SourceRecordDoesNotExist25551,
    );
  } catch (error) {
    if (context.mounted) {
      await showKnowledgeText(
        context,
        context.l10n.l6CouldNotLoadd9f60,
        context.errorMessage(context.l10n.l6LoadSourceRecordse26e1, error),
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
  String? _processingResult;
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
        setState(
          () => _failure = _error(
            context,
            error,
            context: context.l10n.l6DecideOnThisSource4e2fd,
            showDetail: true,
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 手动重整理（提案 P1）：单页再汇总，原位显示结果；详情随之刷新。
  Future<void> _reconsolidate() async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      final result = await ref
          .read(knowledgeRepositoryProvider)
          .reconsolidate(widget.page.slug);
      if (!mounted) return;
      setState(() => _processingResult = result);
      _refresh(ref, widget.page.slug);
    } catch (error) {
      if (mounted) {
        setState(
          () => _failure = _error(
            context,
            error,
            context: context.l10n.l6Reprocess796dd,
            showDetail: true,
          ),
        );
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
        title: Text(
          context.l10n.l6DiscardAsRejectedKnowledge14322,
          style: TextStyle(fontSize: 16),
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.l10n.l6WhyIsThisExcerptUnreliableOra83f2,
              style: TextStyle(fontSize: 12),
            ),
            const SizedBox(height: AppTheme.space2),
            TextField(
              controller: controller,
              autofocus: true,
              decoration: InputDecoration(
                hintText: context.l10n.l6RequiredReasonForDiscarding0dc5c,
              ),
              onSubmitted: (v) => Navigator.of(context).pop(v),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text.trim()),
            child: Text(context.l10n.l6Discardea489),
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
        setState(
          () => _failure = _error(
            context,
            error,
            context: context.l10n.l6OrganizeKnowledge210a3,
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final material = ['source', 'note'].contains(widget.page.kind);
    final incubating = isIncubatingPage(widget.page.kind, widget.page.opinion);
    // P1 可观测：派生读最近整理/新增证据；详情未就绪或失败时静默隐藏。
    final consolidation = ref
        .watch(knowledgeDetailsProvider(widget.page.slug))
        .value
        ?.consolidation;
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
            label: Text(context.l10n.wikiSourcesRevisions),
          ),
        if (_busy)
          SizedBox(
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
                Text(context.l10n.l6Processing1a5c2),
              ],
            ),
          )
        else if (material)
          PopupMenuButton<String>(
            tooltip: context.l10n.l6ExtractOnDemandSaveNewReference28c44,
            onSelected: _propose,
            itemBuilder: (_) => [
              PopupMenuItem(
                value: 'method',
                child: ListTile(
                  dense: true,
                  title: Text(context.l10n.l6ExtractAMethod9c1f5),
                  subtitle: Text(
                    context.l10n.l6ReusableStepsAndConditions4c52e,
                  ),
                ),
              ),
              PopupMenuItem(
                value: 'case',
                child: ListTile(
                  dense: true,
                  title: Text(context.l10n.l6OrganizeACaseac694),
                  subtitle: Text(context.l10n.l6ContextApproachAndOutcomeee83d),
                ),
              ),
              PopupMenuItem(
                value: 'principle',
                child: ListTile(
                  dense: true,
                  title: Text(context.l10n.l6ExtractAPrinciple14a19),
                  subtitle: Text(
                    context.l10n.l6PrinciplesEvidenceAndLimits440fe,
                  ),
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
                      context.l10n.l6ExtractKnowledge196cf,
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
            label: Text(context.l10n.l6CheckForUpdatesa6df3),
          ),
        // 手动重整理（提案 P1）：全量来源重看后增量修补，人工页会被拒。
        if (!_busy)
          TextButton.icon(
            style: style,
            onPressed: _reconsolidate,
            icon: const Icon(Icons.autorenew, size: 16),
            label: Text(context.l10n.l6ReprocessThisPaged752f),
          ),
        if (consolidation != null)
          Text(
            context.l10n.l6LastProcessedArg0NewEvidenceArg124fb9(
              (consolidation.lastConsolidatedAt == null
                      ? context.l10n.l6Unknownd9c32
                      : _shortTime(consolidation.lastConsolidatedAt!))
                  .toString(),
              (consolidation.newEvidence).toString(),
            ),
            style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
          ),
        if (_processingResult != null)
          ProcessingNotes(
            note:
                ProcessingResult.decode(_processingResult!)?.detail ??
                _processingResult!,
            status:
                ProcessingResult.decode(_processingResult!)
                    ?.message(context.l10n) ??
                context.l10n.reprocessingResult,
          ),
        if (_failure != null)
          AppErrorMessage(action: context.l10n.load, error: _failure!),
        if (_failure != null && !_busy)
          TextButton.icon(
            onPressed: () =>
                _lastKind == null ? _reconsolidate() : _propose(_lastKind!),
            icon: const Icon(Icons.refresh, size: 16),
            label: Text(context.l10n.l6RetryProcessingf8833),
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
            label: Text(context.l10n.l6IncludeInKnowledge29c9c),
          ),
          TextButton.icon(
            style: TextButton.styleFrom(
              minimumSize: const Size(0, 36),
              padding: const EdgeInsets.symmetric(horizontal: 10),
            ),
            onPressed: _confirmReject,
            icon: const Icon(Icons.block_outlined, size: 16),
            label: Text(context.l10n.l6Discardea489),
          ),
        ] else if (widget.page.opinion == 'endorse')
          _OpinionBadge(
            label: context.l10n.l6IncludedInKnowledge9a862,
            color: Color(0xFF9ECE6A),
          )
        else if (widget.page.opinion == 'reject')
          _OpinionBadge(
            label: context.l10n.l6DiscardedRejectedKnowledge57399,
            color: Color(0xFFF7768E),
          ),
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
      Text(label, style: TextStyle(fontSize: 11, color: color)),
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
