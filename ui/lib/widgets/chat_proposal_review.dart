import '../l10n/notice_messages.dart';
import 'app_error_notice.dart';

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../providers/conversation_provider.dart';
import '../providers/goal_provider.dart';
import '../providers/todo_provider.dart';
import '../providers/wiki_provider.dart';
import 'adaptive_panel.dart';
import 'markdown_view.dart';
import '../utils/pretty_time.dart';
import '../l10n/localization.dart';

part 'chat_proposal_parts/proposal_review_dialog.dart';
part 'chat_proposal_parts/proposal_batch_dialog.dart';
part 'chat_proposal_parts/proposal_detail_dialog.dart';

// ── 方案评审按对话框粒度拆分（纯类段平移，零行为变化）──

String _actionLabel(BuildContext context, String action) => switch (action) {
  'activate_rule' => context.l10n.actionActivateRule,
  'create_todo' => context.l10n.actionCreateTodo,
  'save_knowledge_draft' => context.l10n.actionSavePage,
  'save_wiki_revision' => context.l10n.actionRevisePage,
  'rename_wiki_page' => context.l10n.actionRenamePage,
  'record_event' => context.l10n.actionRecordEvent,
  'propose_people_relations' => context.l10n.actionPeopleRelations,
  'archive_conversations_by_title' => context.l10n.actionArchiveChats,
  'import_url_to_wiki' => context.l10n.actionImportSource,
  'import_directory_files' => context.l10n.actionImportFiles,
  'import_directory_as_project' => context.l10n.actionImportProject,
  'run_skill_script' => context.l10n.toolSkillScript,
  'mcp_tool_call' => context.l10n.toolMcp,
  'propose_planning_profile' => context.l10n.actionPlanningProfile,
  'create_goal' => context.l10n.actionCreateGoal,
  'update_goal_facets' => context.l10n.actionUpdateGoal,
  'create_plan' => context.l10n.actionCreatePlan,
  'add_plan_steps' => context.l10n.actionAddSteps,
  _ => action,
};

Map<String, dynamic>? _args(String text) {
  try {
    final value = jsonDecode(text);
    return value is Map ? Map<String, dynamic>.from(value) : null;
  } catch (_) {
    return null;
  }
}

api.ChatReviewRequest _request(api.ChatProposalDto proposal) =>
    api.ChatReviewRequest(
      kind: proposal.kind,
      proposalId: proposal.id,
      conversationId: proposal.conversationId,
      expectedVersion: proposal.version,
      approvalToken: proposal.approvalToken,
    );

String _statusLabel(BuildContext context, String status) => switch (status) {
  'executing' => context.l10n.receiptWaiting,
  'failed' => context.l10n.receiptFailedRolledBack,
  'succeeded' => context.l10n.completed,
  'declined' => context.l10n.declined,
  'uncertain' => context.l10n.toolUncertain,
  _ => status,
};

class ChatProposalReviewButton extends ConsumerWidget {
  const ChatProposalReviewButton({
    super.key,
    required this.conversationId,
    this.onChanged,
    this.label,
    this.compact = false,
  });
  final String? conversationId;
  final VoidCallback? onChanged;
  final String? label;
  final bool compact;

  Future<void> _open(BuildContext context) async {
    // 与「今天」行其他入口（目标/待办/创建主题）同一形态：一律 bottom
    // sheet（全平台），不再分桌面 dialog / 移动 sheet。
    await showPanelSheet<bool>(
      context,
      sheet: (_) => AdaptivePanelSheet(
        title: conversationId == null
            ? context.l10n.globalRuleReview
            : context.l10n.conversationProposalReview,
        maxHeightFactor: 0.85,
        builder: (_) =>
            ChatProposalReviewDialog(conversationId: conversationId),
      ),
    );
    if (context.mounted) onChanged?.call();
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // 有待审提议时显示徽标（只探测是否有，不取全量）。
    final pending = ref.watch(_hasPendingProvider(conversationId));
    final hasPending = pending.value == true;
    final icon = hasPending
        ? Badge(
            label: const Text(' '),
            smallSize: 8,
            child: const Icon(Icons.fact_check_outlined, size: 16),
          )
        : const Icon(Icons.fact_check_outlined, size: 16);
    return compact
        ? IconButton(
            tooltip: label ?? context.l10n.reviewProposals,
            icon: icon,
            visualDensity: VisualDensity.compact,
            onPressed: () => _open(context),
          )
        : TextButton.icon(
            icon: icon,
            label: Text(label ?? context.l10n.reviewProposals),
            style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => _open(context),
          );
  }
}

/// 是否有待审提议（轻量探测，limit 1）。dialog 内 _afterReview 会
/// invalidate，触发徽标刷新。
final _hasPendingProvider = FutureProvider.family<bool, String?>((
  ref,
  conversationId,
) async {
  final repo = ref.watch(conversationRepositoryProvider);
  final rows = await repo.listChatProposals(
    conversationId,
    offset: 0,
    limit: 1,
  );
  return rows.isNotEmpty;
});
