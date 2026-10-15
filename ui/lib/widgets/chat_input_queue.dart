import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart';
import '../providers/conversation_provider.dart';
import 'chat_run_history.dart';

/// Read-only queue observation. Dispatch belongs to the repository lifecycle,
/// so changing pages neither stops an authorized queue nor resumes a paused one.
class ChatInputQueue extends ConsumerStatefulWidget {
  const ChatInputQueue({super.key, required this.conversationId});
  final String conversationId;

  @override
  ConsumerState<ChatInputQueue> createState() => _ChatInputQueueState();
}

String _supplementLabel(String? reason) => switch (reason) {
  'unsupported_context' => '未补充：材料不能安全合并，已完整保留；请单独处理或撤回',
  'input_changed' => '未补充：输入版本已变化，仍在队列',
  'context_changed' => '未补充：材料版本或任务范围不同，已保留为下一问',
  'budget_or_protocol' => '未补充：当前请求预算或协议不足，仍在队列',
  'target_closed' => '目标任务已结束；继续队列前不会生成',
  _ => '等待工具轮结束后补充；未加入时仍保留排队资格',
};

class _ChatInputQueueState extends ConsumerState<ChatInputQueue> {
  ChatInputQueueDto? _queue;
  String? _error;
  bool _loading = false;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    unawaited(_tick());
  }

  @override
  void didUpdateWidget(ChatInputQueue oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.conversationId != widget.conversationId) {
      _queue = null;
      _error = null;
      _timer?.cancel();
      unawaited(_tick());
    }
  }

  Future<void> _tick() async {
    if (_loading || !mounted) return;
    final id = widget.conversationId;
    final repository = ref.read(conversationRepositoryProvider);
    if (!repository.supportsChatQueue) return;
    _loading = true;
    try {
      final queue = await repository.getChatInputQueue(id);
      if (!mounted || widget.conversationId != id) return;
      setState(() {
        _queue = queue;
        _error = null;
      });
    } catch (_) {
      if (mounted && widget.conversationId == id) {
        setState(() => _error = '队列状态暂不可用；输入不会重新保存。');
      }
    } finally {
      _loading = false;
      if (mounted) {
        _timer?.cancel();
        _timer = Timer(const Duration(seconds: 1), _tick);
      }
    }
  }

  Future<void> _change(Future<void> Function() action) async {
    try {
      await action();
      await _tick();
    } catch (_) {
      if (mounted) setState(() => _error = '队列操作未确认，请刷新核实。');
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final repository = ref.watch(conversationRepositoryProvider);
    if (!repository.supportsChatQueue) return const SizedBox.shrink();
    final queue = _queue;
    if (_error != null) return Text(_error!);
    if (queue == null ||
        (queue.items.isEmpty && queue.recentInjections.isEmpty)) {
      return ChatRunHistory(conversationId: widget.conversationId);
    }
    final id = widget.conversationId;
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 8,
            children: [
              Text(queue.paused ? '待生成队列已暂停' : '待生成队列 · 按保存顺序处理'),
              TextButton(
                onPressed: () => _change(
                  () => queue.paused
                      ? repository.resumeChatInputQueue(id)
                      : repository.pauseChatInputQueue(id),
                ),
                child: Text(queue.paused ? '继续队列' : '暂停队列'),
              ),
            ],
          ),
          if (queue.items.isNotEmpty) const Text('待生成列表中的输入尚未发送给模型；撤回只取消生成资格。'),
          for (final item in queue.items.take(10))
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        item.preview,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      if (item.mode == 'steer')
                        Text(
                          _supplementLabel(item.deferReason),
                          style: const TextStyle(fontSize: 12),
                        ),
                    ],
                  ),
                ),
                TextButton(
                  onPressed: () => _change(() async {
                    await repository.withdrawChatInput(id, item.id);
                  }),
                  child: const Text('撤回生成'),
                ),
              ],
            ),
          if (queue.items.length > 10) const Text('另有待生成输入；前面的输入处理后会依次显示。'),
          if (queue.recentInjections.isNotEmpty)
            ExpansionTile(
              title: const Text('已加入任务的补充'),
              children: [
                const Text('仅表示已加入下一请求上下文，不保证网络送达或模型完成；不会再次排队。'),
                for (final item in queue.recentInjections)
                  ListTile(
                    dense: true,
                    title: Text(
                      item.preview,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
            ),
          ChatRunHistory(conversationId: id),
        ],
      ),
    );
  }
}
