import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/knowledge_provider.dart';

/// 「反馈这条建议」入口开关，当前默认关闭。
///
/// 关闭原因：实测价值远低于 UI 暗示的「AI 会根据反馈优化回复」。
/// 实际机制只是把最近 8 条拼成 JSON 注入一条 system 消息
/// （`knowledge/workflows.rs::feedback_context` → `conversation.rs:152`），
/// 既不写入长期记忆也不跨对话生效，且仓库内没有任何测试验证模型是否照做。
/// 四个标记里只有「改写」和「忽略」有实际语义，「接受」在 prompt 中
/// 只规定了「不代表行动已执行」，是空操作。
///
/// 保留的部分：数据层、`suggestion_feedback` 表、注入链路全部不动，
/// 库里已有的反馈仍会照常参与上下文——只是不再提供新增入口。
///
/// 重新开启：把此常量改为 `true`，无需改动任何其他代码。
/// 若要让它真正兑现「反馈会被记住」的承诺，需要另做两件事：
///   1. 把「忽略」接入规则提议流程（`handle_rule_proposal_confirmation`），
///      否则用户设了一次会在新对话里困惑；
///   2. 增加效果验证——现有测试只断言注入字符串包含标记，不断言模型行为。
const bool kSuggestionFeedbackEnabled = false;

/// Feedback is an explicit user decision about a selected piece of a reply.
class SuggestionFeedbackControl extends ConsumerStatefulWidget {
  final String messageId, content;
  const SuggestionFeedbackControl({
    super.key,
    required this.messageId,
    required this.content,
  });
  @override
  ConsumerState<SuggestionFeedbackControl> createState() => _FeedbackState();
}

class _FeedbackState extends ConsumerState<SuggestionFeedbackControl> {
  final _suggestion = TextEditingController(),
      _rewrite = TextEditingController();
  String? _decision, _error;
  bool _loaded = false, _busy = false, _open = false;
  @override
  void dispose() {
    _suggestion.dispose();
    _rewrite.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    if (_loaded) return;
    _loaded = true;
    _suggestion.text = widget.content.length <= 12000 ? widget.content : '';
    try {
      final value = await ref
          .read(knowledgeRepositoryProvider)
          .feedback(widget.messageId);
      if (mounted && value != null) {
        setState(() {
          _decision = value.decision;
          _suggestion.text = value.suggestion;
          _rewrite.text = value.rewrite ?? '';
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '读取反馈失败：$e');
    }
  }

  Future<void> _save(String decision) async {
    setState(() => _busy = true);
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .saveFeedback(
            widget.messageId,
            decision,
            _suggestion.text,
            decision == 'rewritten' ? _rewrite.text : null,
          );
      if (mounted) {
        setState(() {
          _decision = decision;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '反馈未保存：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      TextButton.icon(
        style: TextButton.styleFrom(
          minimumSize: const Size(0, 24),
          padding: const EdgeInsets.symmetric(horizontal: 4),
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        ),
        icon: Icon(
          _open ? Icons.expand_less : Icons.rate_review_outlined,
          size: 14,
        ),
        label: const Text('反馈这条建议', style: TextStyle(fontSize: 12)),
        onPressed: () {
          setState(() => _open = !_open);
          if (_open) _load();
        },
      ),
      if (_open) ...[
        const Text('选择回复中的建议原文。接受表示认可建议，实际行动仍需另行确认。'),
        TextField(
          controller: _suggestion,
          minLines: 2,
          maxLines: 5,
          decoration: const InputDecoration(labelText: '建议原文（可缩小到其中一段）'),
        ),
        TextField(
          controller: _rewrite,
          minLines: 1,
          maxLines: 4,
          maxLength: 6000,
          decoration: const InputDecoration(labelText: '我的改写（可选）'),
        ),
        Wrap(
          spacing: 8,
          children: [
            for (final e in {
              'accepted': '接受',
              'ignored': '忽略',
              'rewritten': '保存改写',
              'cleared': '撤回反馈',
            }.entries)
              TextButton(
                onPressed: _busy ? null : () => _save(e.key),
                child: Text(e.value),
              ),
          ],
        ),
        if (_busy) const LinearProgressIndicator(),
        if (_decision != null)
          Text(
            '已记录：${{'accepted': '接受', 'ignored': '忽略', 'rewritten': '改写', 'cleared': '撤回'}[_decision]}',
          ),
        if (_error != null) Text(_error!),
      ],
    ],
  );
}
