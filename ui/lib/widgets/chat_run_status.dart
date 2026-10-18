import 'dart:math';

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
    // 完成即静默：成功终态不再占屏；停止/中断保留条目（挂着重试入口）。
    if (status == 'completed') {
      return const SizedBox.shrink();
    }
final label = view.starting
    ? '正在启动生成…'
    : view.restoring
    ? '正在读取生成状态…'
    : switch (status) {
        'failed' => '生成失败，输入已保存',
        'cancelled' => '已停止生成',
        'interrupted' => '生成已中断',
        'cancelling' => '正在停止…',
        _ => ref.watch(
          _gerundPickProvider(
            '$conversationId|${run?.id ?? 'pending'}|${_gerundPhase(view)}',
          ),
        ),
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
            ],
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
      'run_skill_script': '执行技能脚本',
      'mcp_tool_call': 'MCP 工具调用',
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

/// 生成中状态词。按阶段取随机短语（Claude Code 风），同一 run 内稳定
/// 不闪变，新一轮生成换词；技术阶段细节不再面向用户。
const _gerundPreparing = [
  '翻阅记忆中',
  '整理思路中',
  '回顾对话中',
  '翻检知识库中',
  '搜集线索中',
  '梳理上下文中',
];

const _gerundAwaiting = [
  '正在酝酿',
  '正在构思',
  '正在推敲',
  '正在斟酌',
  '灵感涌现中',
  '编织回复中',
  '打磨措辞中',
  '抽丝剥茧中',
];

String _gerundPhase(ChatRunView view) =>
    view.phase == 'requesting' ? 'awaiting' : 'preparing';

final _gerundPickProvider = Provider.autoDispose.family<String, String>((
  ref,
  key,
) {
  final pool = key.endsWith('|awaiting') ? _gerundAwaiting : _gerundPreparing;
  return '${pool[Random().nextInt(pool.length)]}…';
});
