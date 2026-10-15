import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart';
import '../providers/chat_run_provider.dart';
import 'markdown_view.dart';
import '../providers/conversation_provider.dart';

/// A bounded, explicitly opened history keeps older interrupted inputs reachable
/// without replacing the live observer or automatically replaying their work.
class ChatRunHistory extends ConsumerWidget {
  const ChatRunHistory({super.key, required this.conversationId});
  final String conversationId;

  @override
  Widget build(BuildContext context, WidgetRef ref) => TextButton.icon(
    icon: const Icon(Icons.history, size: 16),
    label: const Text('生成历史与恢复'),
    onPressed: () => showDialog<void>(
      context: context,
      builder: (_) => _RunHistoryDialog(
        conversationId: conversationId,
        repository: ref.read(conversationRepositoryProvider),
        onRetry: (input) =>
            ref.read(chatRunControllerProvider(conversationId)).start(input),
        onQueued: () =>
            ref.read(chatRunControllerProvider(conversationId)).restore(),
      ),
    ),
  );
}

class _RunHistoryDialog extends StatefulWidget {
  const _RunHistoryDialog({
    required this.conversationId,
    required this.repository,
    required this.onRetry,
    required this.onQueued,
  });
  final String conversationId;
  final ConversationRepository repository;
  final Future<void> Function(String) onRetry;
  final Future<void> Function() onQueued;

  @override
  State<_RunHistoryDialog> createState() => _RunHistoryDialogState();
}

class _RunHistoryDialogState extends State<_RunHistoryDialog> {
  final List<ChatRunDto> _runs = [];
  final List<UnscheduledChatInputDto> _savedInputs = [];
  ChatRunDto? _selected;
  bool _loading = false;
  bool _more = true;
  bool _moreSaved = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _initialize();
  }

  Future<void> _initialize() async {
    await _load();
    if (mounted) await _loadSaved();
  }

  Future<void> _loadSaved() async {
    if (_loading || !widget.repository.supportsChatQueue) return;
    setState(() => _loading = true);
    try {
      final page = await widget.repository.listUnscheduledChatInputs(
        widget.conversationId,
        offset: _savedInputs.length,
      );
      if (!mounted) return;
      setState(() {
        _savedInputs.addAll(page);
        _moreSaved = page.length == 20;
      });
    } catch (_) {
      if (mounted) setState(() => _error = '已保存输入暂不可用；不会重新保存或生成。');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _scheduleSaved(UnscheduledChatInputDto input) async {
    if (_loading) return;
    setState(() => _loading = true);
    try {
      await widget.repository.scheduleChatInput(
        widget.conversationId,
        input.inputMessageId,
      );
      await widget.onQueued();
      if (mounted) Navigator.pop(context);
    } catch (_) {
      if (mounted) {
        setState(() => _error = '生成资格待核实；可重试此输入排队，不要重新发送。重复排队不会重复保存或生成。');
      }
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _load() async {
    if (_loading) return;
    setState(() => _loading = true);
    try {
      final page = await widget.repository.listChatRuns(
        widget.conversationId,
        offset: _runs.length,
      );
      if (!mounted) return;
      setState(() {
        _runs.addAll(page);
        _more = page.length == 20;
        _error = null;
      });
    } catch (_) {
      if (mounted) setState(() => _error = '历史暂不可用；不会重新生成。');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _select(ChatRunDto run) async {
    setState(() => _loading = true);
    try {
      final snapshot = await widget.repository.getChatRun(
        widget.conversationId,
        run.id,
      );
      if (mounted)
        setState(() {
          _selected = snapshot;
          _error = null;
        });
    } catch (_) {
      if (mounted) setState(() => _error = '无法读取该次运行，请重试查看。');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _retry() async {
    final selected = _selected;
    if (selected == null || _loading) return;
    setState(() => _loading = true);
    try {
      final latest = await widget.repository.getLatestChatRun(
        widget.conversationId,
      );
      if (latest != null && chatRunIsActive(latest.status)) {
        if (mounted) setState(() => _error = '请先等待或停止当前生成；不会打断另一条输入。');
        return;
      }
      await widget.onRetry(selected.inputMessageId);
      if (mounted) Navigator.pop(context);
    } catch (_) {
      if (mounted) setState(() => _error = '重试结果待核实，请查看当前生成状态，不要重新发送输入。');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final selected = _selected;
    return AlertDialog(
      title: const Text('生成历史与恢复'),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('重试只生成所选原输入，不重新保存消息。已提交操作不会撤销；不确定结果请先核实，草稿仍需审阅。'),
              if (_error != null) Text(_error!),
              if (_savedInputs.isNotEmpty) ...[
                const Divider(),
                const Text('已保存但无生成关联的输入'),
                const Text(
                  '可能是仅保存、登记失败或旧版输入；不代表从未回复。只有点击才为所选原输入排队，不重新保存、不补充旧任务、不批准业务动作。已有队列按顺序处理；暂停后仍需明确继续。',
                ),
                for (final input in _savedInputs)
                  ListTile(
                    dense: true,
                    title: Text(
                      input.preview,
                      maxLines: 3,
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(
                      '${input.createdAt}\n输入 ${input.inputMessageId}',
                    ),
                    trailing: TextButton(
                      key: ValueKey('recover-saved-${input.inputMessageId}'),
                      onPressed: _loading ? null : () => _scheduleSaved(input),
                      child: const Text('为此输入排队'),
                    ),
                  ),
                if (_moreSaved)
                  TextButton(
                    onPressed: _loading ? null : _loadSaved,
                    child: const Text('加载更多已保存输入'),
                  ),
                const Divider(),
              ],
              for (final run in _runs)
                ListTile(
                  dense: true,
                  selected: selected?.id == run.id,
                  title: Text('${_label(run.status)} · ${run.createdAt}'),
                  subtitle: Text(
                    '输入 ${run.inputMessageId}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  onTap: _loading ? null : () => _select(run),
                ),
              if (_runs.isEmpty && !_loading) const Text('尚无生成记录'),
              if (_more)
                TextButton(
                  onPressed: _loading ? null : _load,
                  child: const Text('加载更多记录'),
                ),
              if (selected != null) ...[
                const Divider(),
                Text(selected.errorText ?? _label(selected.status)),
                if (selected.content.isNotEmpty)
                  SelectionArea(
                    child: MarkdownView(markdown: selected.content),
                  ),
                if (const {
                  'failed',
                  'cancelled',
                  'interrupted',
                }.contains(selected.status))
                  TextButton(
                    onPressed: _loading ? null : _retry,
                    child: const Text('重试所选原输入'),
                  ),
              ],
              if (_loading) const LinearProgressIndicator(),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('关闭'),
        ),
      ],
    );
  }
}

String _label(String status) => switch (status) {
  'completed' => '已完成',
  'failed' => '生成失败',
  'interrupted' => '已中断',
  'cancelled' => '已停止',
  'cancelling' => '正在停止',
  _ => '生成中',
};
