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

// ── 知识面板拆分为 part（行为零变化，见 knowledge_panel_parts/）──
part 'knowledge_panel_parts/sources_links.dart';
part 'knowledge_panel_parts/outputs_sources.dart';
part 'knowledge_panel_parts/proposal_card.dart';
part 'knowledge_panel_parts/review_controls.dart';
part 'knowledge_panel_parts/maintenance_buttons.dart';

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
      setState(() => _failure = result);
      _refresh(ref, widget.page.slug);
    } catch (error) {
      if (mounted) {
        setState(() => _failure = _error(error, context: '重整理', showDetail: true));
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
        // 手动重整理（提案 P1）：全量来源重看后增量修补，人工页会被拒。
        if (!_busy)
          TextButton.icon(
            style: style,
            onPressed: _reconsolidate,
            icon: const Icon(Icons.autorenew, size: 16),
            label: const Text('重新整理此页'),
          ),
        if (consolidation != null)
          Text(
            '最近整理 '
            '${consolidation.lastConsolidatedAt == null ? '未知' : _shortTime(consolidation.lastConsolidatedAt!)}'
            ' · 新增证据 ${consolidation.newEvidence} 条',
            style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
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
