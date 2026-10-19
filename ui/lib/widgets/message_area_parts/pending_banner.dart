part of '../message_area.dart';

//! 待定关系横幅与「今天」分隔条。

class _PendingRelationsBanner extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final relation = actions
        .cast<dynamic>()
        .where((a) => a.action == 'propose_people_relations')
        .toList();
    if (relation.isEmpty) return const SizedBox.shrink();
    // argsJson 来自 DB，可能因旧数据/截断/版本迁移而非法：build 中 bare
    // jsonDecode 一旦抛错会拖垮整个消息列表渲染，解析失败改为渲染占位。
    Map<String, dynamic> payload = const <String, dynamic>{};
    try {
      final decoded = jsonDecode(relation.first.argsJson);
      if (decoded is Map<String, dynamic>) {
        payload = decoded;
      }
    } catch (_) {
      // 保持默认空 payload：badge 渲染"无法解析"占位，不崩消息列表。
    }
    final people = (payload['people'] as List? ?? const [])
        .map((item) => (item as Map)['name']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .join('、');
    final targets = (payload['relations'] as List? ?? const [])
        .map((item) => (item as Map)['target']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .toSet()
        .join('、');
    final degraded = payload.isEmpty && people.isEmpty && targets.isEmpty;
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: AppTheme.accentPrimary.withValues(alpha: .1),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Icon(
            Icons.auto_awesome_outlined,
            size: 18,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              degraded
                  ? '有一条联系人关联建议，但数据无法解析。'
                  : '发现联系人：$people\n关联事项：$targets\n请打开“审阅提议”，接受或拒绝这项建议。',
              style: const TextStyle(fontSize: 12, height: 1.5),
            ),
          ),
          if (!degraded && _hasAmbiguity(ref, payload))
            TextButton(
              onPressed: () => _resolveAmbiguity(context, ref, relation.first),
              child: const Text('选择'),
            ),
        ],
      ),
    );
  }

  bool _hasAmbiguity(WidgetRef ref, Map<String, dynamic> payload) {
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    final people = (payload['people'] as List? ?? const []).map(
      (item) => (item as Map)['name']?.toString() ?? '',
    );
    final targets = (payload['relations'] as List? ?? const []).map(
      (item) => (item as Map)['target']?.toString() ?? '',
    );
    return [...people, ...targets].any(
      (name) =>
          pages
              .where(
                (page) =>
                    page.title.trim().toLowerCase() ==
                    name.trim().toLowerCase(),
              )
              .length >
          1,
    );
  }

  /// 联系人/事项同名歧义的选择流程（P19）：
  /// - `argsJson` 来自库里存的动作参数，历史脏数据或未来方言都可能让它不是
  ///   JSON 对象——直接 `jsonDecode` 抛出会把整条流程炸掉，这里收口成提示。
  /// - 选择全部收集完再**一次性**落库：中途 return 会让用户已经点过的选择被
  ///   静默丢弃，所以取消时明确提示「未保存」，而不是无声退出。
  /// - 每个对话框前查 `mounted`：前一个对话框本身就是一次异步 gap。
  Future<void> _resolveAmbiguity(
    BuildContext context,
    WidgetRef ref,
    dynamic action,
  ) async {
    final conversationId = ref.read(selectedConversationIdProvider);
    if (conversationId == null) return;
    final Map<String, dynamic> payload;
    try {
      final decoded = jsonDecode(action.argsJson);
      if (decoded is! Map) {
        _notifyAmbiguity(context, '动作参数格式异常，无法选择联系人');
        return;
      }
      payload = Map<String, dynamic>.from(decoded);
    } catch (_) {
      _notifyAmbiguity(context, '动作参数无法解析，无法选择联系人');
      return;
    }
    final pages = ref.read(wikiPagesProvider).value ?? const [];
    final relations = (payload['relations'] as List? ?? const [])
        .map((item) => Map<String, dynamic>.from(item as Map))
        .toList();
    for (final relation in relations) {
      final person = relation['person']?.toString() ?? '';
      final target = relation['target']?.toString() ?? '';
      final peopleMatches = pages
          .where(
            (page) =>
                page.title.trim().toLowerCase() == person.trim().toLowerCase(),
          )
          .toList();
      if (peopleMatches.length > 1) {
        if (!context.mounted) return;
        final selected = await _pickPage(
          context,
          '选择联系人「$person」',
          peopleMatches,
        );
        if (selected == null) {
          if (!context.mounted) return;
          _notifyAmbiguity(context, '已取消选择，未保存本次修改');
          return;
        }
        relation['from_slug'] = selected.slug;
      }
      final targetMatches = pages
          .where(
            (page) =>
                page.title.trim().toLowerCase() == target.trim().toLowerCase(),
          )
          .toList();
      if (targetMatches.length > 1) {
        if (!context.mounted) return;
        final selected = await _pickPage(
          context,
          '选择事项「$target」',
          targetMatches,
        );
        if (selected == null) {
          if (!context.mounted) return;
          _notifyAmbiguity(context, '已取消选择，未保存本次修改');
          return;
        }
        relation['to_slug'] = selected.slug;
      }
    }
    if (!context.mounted) return;
    payload['relations'] = relations;
    await ref
        .read(conversationRepositoryProvider)
        .updatePendingActionArgs(conversationId, action, jsonEncode(payload));
    if (!context.mounted) return;
    ref.invalidate(pendingActionsProvider);
  }

  /// 歧义选择流程的失败/取消提示（context 失效后静默丢弃，避免用已销毁的 context）。
  void _notifyAmbiguity(BuildContext context, String message) {
    if (!context.mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(message)));
  }

  Future<WikiPageRef?> _pickPage(
    BuildContext context,
    String title,
    List<WikiPageRef> pages,
  ) {
    return showDialog<WikiPageRef>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final page in pages)
                ListTile(
                  title: Text(page.title),
                  subtitle: Text(
                    '${page.kindLabel} · ${page.summary}',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  onTap: () => Navigator.pop(dialogContext, page),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('取消'),
          ),
        ],
      ),
    );
  }
}

/// 会话内临时提示气泡（AI 回复失败等）。仅内存态——不写库，
/// 不进入对话历史 / 记忆注入 / AI 上下文；发送下一条消息后即清除。
/// 只承载失败原因文案；「重新生成」入口在最后一条用户消息上。
class _DayDivider extends StatelessWidget {
  final String label;

  const _DayDivider({required this.label});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space4),
      child: Row(
        children: [
          Expanded(child: Divider(color: AppTheme.surface3)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
            child: Text(
              label,
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
          ),
          Expanded(child: Divider(color: AppTheme.surface3)),
        ],
      ),
    );
  }
}

/// 会话顶部状态条。统计项（待办 / 话题 / provider / token / 待入库草稿）
/// 一律在**这里**订阅，而不是在消息列表所在的 State 里 watch：它们与消息
/// 内容无关，放在父级 build 里会让每次 todo/wiki/token 变化都重建整份消息
/// 列表（P13）。
/// 「今天」栏和标题栏「今天」行共用的紧凑图标按钮。
