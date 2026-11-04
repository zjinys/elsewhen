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

part 'chat_proposal_parts/proposal_review_dialog.dart';
part 'chat_proposal_parts/proposal_batch_dialog.dart';
part 'chat_proposal_parts/proposal_detail_dialog.dart';

// ── 方案评审按对话框粒度拆分（纯类段平移，零行为变化）──

String _actionLabel(String action) =>
    const {
      'activate_rule': '个人规则',
      'create_todo': '创建待办',
      'save_knowledge_draft': '保存知识页',
      'save_wiki_revision': '知识页修订或派生',
      'rename_wiki_page': '知识页改名',
      'record_event': '保存记录',
      'propose_people_relations': '联系人与关系',
      'archive_conversations_by_title': '归档对话',
      'import_url_to_wiki': '导入来源',
      'import_directory_files': '导入目录文件',
      'import_directory_as_project': '导入项目摘要',
      'run_skill_script': '执行技能脚本',
      'mcp_tool_call': 'MCP 工具调用',
      'create_goal': '设立认知目标',
      'update_goal_facets': '更新目标切片',
      'create_plan': '长程计划拆解',
      'add_plan_steps': '计划追加步骤',
    }[action] ??
    action;

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

String _statusLabel(String status) =>
    const {
      'executing': '等待执行回执',
      'failed': '执行失败（已回滚）',
      'succeeded': '已完成',
      'declined': '已拒绝',
      'uncertain': '结果待核实',
    }[status] ??
    status;

class ChatProposalReviewButton extends ConsumerWidget {
  const ChatProposalReviewButton({
    super.key,
    required this.conversationId,
    this.onChanged,
    this.label = '审阅提议',
    this.compact = false,
  });
  final String? conversationId;
  final VoidCallback? onChanged;
  final String label;
  final bool compact;

  Future<void> _open(BuildContext context) async {
    // 与「今天」行其他入口（目标/待办/创建主题）同一形态：一律 bottom
    // sheet（全平台），不再分桌面 dialog / 移动 sheet。
    await showPanelSheet<bool>(
      context,
      sheet: (_) => AdaptivePanelSheet(
        title: conversationId == null ? '历史全局规则审阅' : '对话提议审阅',
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
            tooltip: label,
            icon: icon,
            visualDensity: VisualDensity.compact,
            onPressed: () => _open(context),
          )
        : TextButton.icon(
            icon: icon,
            label: Text(label),
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
