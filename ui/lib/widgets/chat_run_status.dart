import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/chat_run_provider.dart';
import '../providers/conversation_provider.dart';
import '../theme/app_theme.dart';

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
    if (view == null) return const Text('正在读取生成状态…');
    final run = view.run;
    if (run == null && !view.busy && view.connectionError == null) {
      return const SizedBox.shrink();
    }
    final controller = ref.read(chatRunControllerProvider(conversationId));
    final status = run?.status;
    final label = view.starting
        ? '正在启动生成…'
        : view.restoring
        ? '正在读取生成状态…'
        : switch (status) {
            'completed' => '回复已完成',
            'failed' => '生成失败，输入已保存',
            'cancelled' => '已停止生成',
            'interrupted' => '生成已中断',
            'cancelling' => '正在停止…',
            _ =>
              view.phase == 'cancelling'
                  ? '正在停止…'
                  : view.phase == 'requesting'
                  ? '正在等待模型回复…'
                  : '正在准备上下文…',
          };
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
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
                if (run != null && run.requestCount > 0)
                  Text(
                    '请求 ${run.requestCount} 次',
                    style: const TextStyle(fontSize: 11),
                  ),
                if (view.active)
                  TextButton.icon(
                    onPressed:
                        view.stopping ||
                            status == 'cancelling' ||
                            view.phase == 'cancelling'
                        ? null
                        : controller.stop,
                    icon: const Icon(Icons.stop_circle_outlined, size: 16),
                    label: const Text('停止生成'),
                  ),
                if (view.canRetry && view.connectionError == null)
                  TextButton.icon(
                    onPressed: controller.retry,
                    icon: const Icon(Icons.refresh, size: 16),
                    label: const Text('重新生成本条'),
                  ),
                if (view.connectionError != null)
                  TextButton(
                    onPressed: view.starting || view.restoring
                        ? null
                        : controller.reconnect,
                    child: const Text('重新连接'),
                  ),
              ],
            ),
            if (view.connectionError != null)
              Text(
                view.connectionError!,
                style: TextStyle(fontSize: 12, color: AppTheme.error),
              ),
            if (run?.errorText != null && !view.active)
              Text(run!.errorText!, style: const TextStyle(fontSize: 12)),
            if (status == 'cancelled' || status == 'interrupted')
              const Text(
                '已提交的操作不会撤销；未完成步骤不会自动重做。',
                style: TextStyle(fontSize: 11),
              ),
            if (view.historyResynced)
              const Text(
                '进度已重新同步，较早步骤可能未在此列出。',
                style: TextStyle(fontSize: 11),
              ),
            if (view.tools.isNotEmpty)
              ExpansionTile(
                tilePadding: EdgeInsets.zero,
                dense: true,
                title: Text(
                  '实际工具步骤（最近 ${view.tools.length} 项）',
                  style: const TextStyle(fontSize: 12),
                ),
                children: [
                  for (final tool in view.tools)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        _toolLabel(tool.name),
                        style: const TextStyle(fontSize: 12),
                      ),
                      subtitle: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(_outcomeLabel(tool.outcome, view.active)),
                          if (tool.details.isNotEmpty)
                            SelectableText(
                              tool.details,
                              style: const TextStyle(fontSize: 11),
                            ),
                        ],
                      ),
                    ),
                ],
              ),
            if (view.draft.isNotEmpty && status != 'completed') ...[
              const SizedBox(height: 6),
              const Text(
                '未核验草稿 · 不代表操作成功或引用有效',
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
            ] else if (view.active)
              const Text(
                '正文在安全检查后显示；工具进度不代表最终答案。',
                style: TextStyle(fontSize: 11),
              ),
          ],
        ),
      ),
    );
  }
}

String _outcomeLabel(String outcome, bool active) => switch (outcome) {
  'running' => active ? '进行中' : '结果待核实；不会自动重做',
  'read' => '查询已完成',
  'proposal_ready' => '草稿已生成，等待审阅；尚未执行',
  'executed' => '操作已执行（业务回执）',
  'failed' => '执行失败',
  _ => '结果待核实',
};

String _toolLabel(String name) =>
    const {
      'list_rules': '查询规则',
      'list_wiki_pages': '浏览知识页',
      'get_wiki_page': '读取知识页',
      'get_daily_token_usage': '查询用量',
      'search_knowledge_base': '检索知识库',
      'fetch_tweet': '读取链接',
      'fetch_page': '读取网页',
      'record_event': '记录事件',
      'save_knowledge_draft': '草拟知识页',
      'list_todos': '查询待办',
      'create_todo': '草拟待办',
      'propose_people_relations': '草拟联系人关联',
      'batch_extract_people_relations': '整理联系人关联',
      'list_events_by_date': '按日期查询事件',
      'archive_conversations_by_title': '准备归档对话',
      'rename_wiki_page': '准备知识页重命名',
      'import_url_to_wiki': '准备来源导入',
      'import_directory_as_project': '分析项目目录',
      'import_directory_files': '准备目录文件导入',
      'save_wiki_revision': '草拟知识页修订',
    }[name] ??
    '处理工具步骤';
