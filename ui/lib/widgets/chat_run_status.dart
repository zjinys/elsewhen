import 'dart:math' show Random;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/design_tokens.dart';
import '../providers/chat_run_provider.dart';
import '../providers/conversation_provider.dart';
import '../utils/dismissed_runs_store.dart';
import '../theme/app_theme.dart';
import '../l10n/localization.dart';

/// Shared presentation for both chat entrances. Tool labels are allowlisted;
/// provider protocol, raw arguments/results and private reasoning are not UI.
class ChatRunStatus extends ConsumerWidget {
  final String conversationId;
  const ChatRunStatus({super.key, required this.conversationId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!ref.watch(conversationRepositoryProvider).supportsChatRuns) {
      return const SizedBox.shrink();
    }
    final view = ref.watch(chatRunStateProvider(conversationId)).value;
    if (view == null) return Text(context.l10n.readingGeneration);
    final run = view.run;
    if (run == null && !view.busy && view.connectionError == null) {
      return const SizedBox.shrink();
    }
    final controller = ref.read(chatRunControllerProvider(conversationId));
    final status = run?.status;
    // 完成即静默：成功终态或已被用户主动取消的 run 不再占屏。
    if (status == 'completed' || DismissedRunsStore.instance.isDismissed(run?.id)) {
      return const SizedBox.shrink();
    }
    final preparing = _gerundPhase(view) == 'preparing';
    final gerundIndex = ref.watch(
      _gerundPickProvider(
        '$conversationId|${run?.id ?? 'pending'}|${_gerundPhase(view)}',
      ),
    );
    final label = view.starting
        ? context.l10n.startGenerating
        : view.restoring
        ? context.l10n.readingGeneration
        : switch (status) {
            'failed' => context.l10n.generationFailedSaved,
            'cancelled' => context.l10n.generationStopped,
            'interrupted' => context.l10n.generationInterrupted,
            'cancelling' => context.l10n.stopping,
            _ => _gerundLabel(context, preparing, gerundIndex),
          };
    final dark = AppTheme.brightness == Brightness.dark;
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 4, 16, 8),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      decoration: BoxDecoration(
        color: dark ? AppTheme.surface2.withValues(alpha: 0.85) : AppTheme.surface1,
        borderRadius: BorderRadius.circular(10),
        border: DesignTokens.hairlineBorder(dark),
        boxShadow: ShadowTokens.level1(dark),
      ),
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 8,
              children: [
                Text(label, style: const TextStyle(fontSize: 12)),
                if (view.active)
                  TextButton.icon(
                    onPressed:
                        view.stopping ||
                            status == 'cancelling' ||
                            view.phase == 'cancelling'
                        ? null
                        : controller.stop,
                    icon: const Icon(Icons.stop_circle_outlined, size: 16),
                    label: Text(context.l10n.stopGeneration),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      minimumSize: const Size(0, 32),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                  ),
                if (view.canRetry && view.connectionError == null) ...[
                  TextButton.icon(
                    onPressed: controller.retry,
                    icon: const Icon(Icons.refresh, size: 16),
                    label: Text(context.l10n.regenerate),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      minimumSize: const Size(0, 32),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                  ),
                  TextButton.icon(
                    onPressed: () async {
                      final runId = view.run?.id;
                      if (runId != null) {
                        await DismissedRunsStore.instance.dismissRun(runId);
                      }
                      clearReplyFailed(ref, conversationId);
                      await controller.dismissOrCancel();
                    },
                    icon: const Icon(Icons.close, size: 15),
                    label: Text(context.l10n.cancel),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      minimumSize: const Size(0, 32),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      foregroundColor: AppTheme.textSecondary,
                    ),
                  ),
                ],
                if (view.connectionError != null)
                  TextButton(
                    onPressed: view.starting || view.restoring
                        ? null
                        : controller.reconnect,
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      minimumSize: const Size(0, 32),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                    child: Text(context.l10n.reconnect),
                  ),
              ],
            ),
            if (view.tools.isNotEmpty)
              ExpansionTile(
                tilePadding: EdgeInsets.zero,
                dense: true,
                title: Text(
                  context.l10n.actualToolSteps(view.tools.length),
                  style: const TextStyle(fontSize: 12),
                ),
                children: [
                  for (final tool in view.tools)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        _toolLabel(context, tool.name),
                        style: const TextStyle(fontSize: 12),
                      ),
                      subtitle: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            _outcomeLabel(context, tool.outcome, view.active),
                          ),
                          if (tool.details.isNotEmpty)
                            SelectableText(
                              tool.localizedDetails(context.l10n),
                              style: const TextStyle(fontSize: 11),
                            ),
                        ],
                      ),
                    ),
                ],
              ),
            if (view.draft.isNotEmpty && status != 'completed') ...[
              const SizedBox(height: 6),
              Text(
                context.l10n.unverifiedDraft,
                style: TextStyle(fontSize: 11),
              ),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 160),
                child: SingleChildScrollView(
                  child: SelectableText(
                    view.draft,
                    style: const TextStyle(fontSize: 12),
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

String _outcomeLabel(BuildContext context, String outcome, bool active) =>
    switch (outcome) {
      'running' =>
        active ? context.l10n.toolRunning : context.l10n.toolUncertainNoRetry,
      'read' => context.l10n.toolReadCompleted,
      'proposal_ready' => context.l10n.toolProposalReady,
      'executed' => context.l10n.toolExecuted,
      'failed' => context.l10n.toolFailed,
      _ => context.l10n.toolUncertain,
    };

String _toolLabel(BuildContext context, String name) => switch (name) {
  'list_rules' => context.l10n.toolListRules,
  'list_wiki_pages' => context.l10n.toolListPages,
  'get_wiki_page' => context.l10n.toolReadPage,
  'get_daily_token_usage' => context.l10n.toolUsage,
  'search_knowledge_base' => context.l10n.toolSearchKnowledge,
  'fetch_tweet' => context.l10n.toolReadLink,
  'fetch_page' => context.l10n.toolReadWeb,
  'record_event' => context.l10n.toolRecordEvent,
  'save_knowledge_draft' => context.l10n.toolDraftPage,
  'run_skill_script' => context.l10n.toolSkillScript,
  'mcp_tool_call' => context.l10n.toolMcp,
  'list_todos' => context.l10n.toolListTodos,
  'create_todo' => context.l10n.toolDraftTodo,
  'propose_people_relations' => context.l10n.toolDraftRelations,
  'batch_extract_people_relations' => context.l10n.toolExtractRelations,
  'list_events_by_date' => context.l10n.toolEventsByDate,
  'archive_conversations_by_title' => context.l10n.toolArchiveChats,
  'rename_wiki_page' => context.l10n.toolRenamePage,
  'import_url_to_wiki' => context.l10n.toolImportSource,
  'import_directory_as_project' => context.l10n.toolInspectDirectory,
  'import_directory_files' => context.l10n.toolImportFiles,
  'save_wiki_revision' => context.l10n.toolRevisePage,
  _ => context.l10n.toolStep,
};

String _gerundPhase(ChatRunView view) =>
    view.phase == 'requesting' ? 'awaiting' : 'preparing';

// Keep the choice stable within a run, while allowing locale changes to render
// the same semantic phrase without starting or replacing a run.
final _gerundPickProvider = Provider.autoDispose.family<int, String>(
  (ref, key) => Random().nextInt(key.endsWith('|awaiting') ? 8 : 6),
);

String _gerundLabel(BuildContext context, bool preparing, int index) {
  final s = context.l10n;
  final pool = preparing
      ? [
          s.gerundPreparing0,
          s.gerundPreparing1,
          s.gerundPreparing2,
          s.gerundPreparing3,
          s.gerundPreparing4,
          s.gerundPreparing5,
        ]
      : [
          s.gerundAwaiting0,
          s.gerundAwaiting1,
          s.gerundAwaiting2,
          s.gerundAwaiting3,
          s.gerundAwaiting4,
          s.gerundAwaiting5,
          s.gerundAwaiting6,
          s.gerundAwaiting7,
        ];
  return pool[index % pool.length];
}
