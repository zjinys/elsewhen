part of '../message_area.dart';

//! 待定关系横幅与「今天」分隔条。

class _PendingMemoryBanner extends ConsumerWidget {
  const _PendingMemoryBanner();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final conversationId = ref.watch(selectedConversationIdProvider);
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final count = actions
        .whereType<PendingActionDto>()
        .where((action) => action.action == 'propose_memory_candidates')
        .length;
    if (conversationId == null || count == 0) return const SizedBox.shrink();
    final dark = AppTheme.brightness == Brightness.dark;
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
      decoration: BoxDecoration(
        color: dark
            ? AppTheme.accentPrimary.withValues(alpha: .12)
            : AppTheme.accentPrimary.withValues(alpha: .08),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: AppTheme.accentPrimary.withValues(alpha: dark ? .30 : .20),
          width: 0.5,
        ),
        boxShadow: ShadowTokens.level1(dark),
      ),
      child: Row(
        children: [
          Icon(
            Icons.auto_awesome_outlined,
            size: DesignTokens.icon,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              '发现 $count 条新的${context.l10n.memoryManager}候选',
              style: const TextStyle(fontSize: 12, height: 1.5),
            ),
          ),
          ChatProposalReviewButton(
            conversationId: conversationId,
            label: context.l10n.reviewProposals,
            compact: true,
          ),
        ],
      ),
    );
  }
}

typedef _RelationChoice = ({
  int relationIndex,
  String slugKey,
  String name,
  List<WikiPageRef> pages,
});

class _PendingRelationsBanner extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final actions = ref.watch(pendingActionsProvider).value ?? const [];
    final proposals = actions
        .whereType<PendingActionDto>()
        .where((a) => a.action == 'propose_people_relations')
        .toList();
    if (proposals.isEmpty) return const SizedBox.shrink();
    final pages = ref.watch(wikiPagesProvider).value ?? const [];
    for (final proposal in proposals) {
      final payload = _decodePayload(proposal.argsJson);
      final relations = _items(payload['relations']);
      final choices = <_RelationChoice>[];
      var hasSavedChoice = false;
      for (final (index, relation) in relations.indexed) {
        if (relation is! Map<String, dynamic>) continue;
        for (final (nameKey, slugKey) in const [
          ('person', 'from_slug'),
          ('target', 'to_slug'),
        ]) {
          final slug = relation[slugKey];
          if (slug is String && slug.trim().isNotEmpty) {
            hasSavedChoice = true;
            continue;
          }
          final name = relation[nameKey]?.toString().trim() ?? '';
          if (name.isEmpty) continue;
          final normalized = name.toLowerCase();
          final matches = pages
              .where((page) => page.title.trim().toLowerCase() == normalized)
              .toList();
          if (matches.length > 1) {
            choices.add((
              relationIndex: index,
              slugKey: slugKey,
              name: name,
              pages: matches,
            ));
          }
        }
      }
      // 选好联系人后，提议仍待审；横幅只让位给下一条需要提示的提议。
      if (hasSavedChoice && choices.isEmpty) continue;
      return _buildBanner(
        context,
        payload,
        relations,
        choices.isEmpty
            ? null
            : () => _resolveAmbiguity(context, ref, proposal, payload, choices),
      );
    }
    return const SizedBox.shrink();
  }

  Map<String, dynamic> _decodePayload(String json) {
    try {
      final decoded = jsonDecode(json);
      if (decoded is Map<String, dynamic>) return decoded;
    } on FormatException {
      // 旧数据无法解析时，仍显示去审阅的提示。
    }
    return const {};
  }

  List<dynamic> _items(Object? value) => value is List ? value : const [];

  Widget _buildBanner(
    BuildContext context,
    Map<String, dynamic> payload,
    List<dynamic> relations,
    VoidCallback? onChoose,
  ) {
    final people = _items(payload['people'])
        .whereType<Map<String, dynamic>>()
        .map((item) => item['name']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .join('、');
    final targets = relations
        .whereType<Map<String, dynamic>>()
        .map((item) => item['target']?.toString() ?? '')
        .where((name) => name.isNotEmpty)
        .toSet()
        .join('、');
    final dark = AppTheme.brightness == Brightness.dark;
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        color: dark
            ? AppTheme.accentPrimary.withValues(alpha: .12)
            : AppTheme.accentPrimary.withValues(alpha: .08),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: AppTheme.accentPrimary.withValues(alpha: dark ? .30 : .20),
          width: 0.5,
        ),
        boxShadow: ShadowTokens.level1(dark),
      ),
      child: Row(
        children: [
          Icon(
            Icons.auto_awesome_outlined,
            size: DesignTokens.icon,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              payload.isEmpty
                  ? context.l10n.contactsSuggestionInvalid
                  : context.l10n.contactsSuggestion(people, targets),
              style: const TextStyle(fontSize: 12, height: 1.5),
            ),
          ),
          if (onChoose != null)
            TextButton(onPressed: onChoose, child: Text(context.l10n.choose)),
        ],
      ),
    );
  }

  /// 一次保存这条提议的所有选择；中途取消时保留原提议与横幅。
  Future<void> _resolveAmbiguity(
    BuildContext context,
    WidgetRef ref,
    PendingActionDto action,
    Map<String, dynamic> payload,
    List<_RelationChoice> choices,
  ) async {
    final conversationId = ref.read(selectedConversationIdProvider);
    if (conversationId == null) return;
    final relations = List.of(_items(payload['relations']));
    for (final choice in choices) {
      if (!context.mounted) return;
      final selected = await _pickPage(
        context,
        choice.slugKey == 'from_slug'
            ? context.l10n.chooseContactName(choice.name)
            : context.l10n.chooseTargetName(choice.name),
        choice.pages,
      );
      if (!context.mounted) return;
      if (selected == null) {
        _notifyAmbiguity(context, context.l10n.selectionCancelledUnsaved);
        return;
      }
      relations[choice.relationIndex] = {
        ...relations[choice.relationIndex] as Map<String, dynamic>,
        choice.slugKey: selected.slug,
      };
    }
    if (!context.mounted ||
        ref.read(selectedConversationIdProvider) != conversationId) {
      return;
    }
    try {
      final saved = await ref
          .read(conversationRepositoryProvider)
          .updatePendingActionArgs(
            conversationId,
            action,
            jsonEncode({...payload, 'relations': relations}),
          );
      if (!context.mounted) return;
      if (saved) {
        ref.invalidate(pendingActionsProvider);
      } else {
        _notifyAmbiguity(
          context,
          context.l10n.operationFailed(context.l10n.save),
        );
      }
    } catch (error) {
      if (!context.mounted) return;
      showAppError(context, context.l10n.save, error);
    }
  }

  /// 歧义选择流程的失败/取消提示（context 失效后静默丢弃，避免用已销毁的 context）。
  void _notifyAmbiguity(BuildContext context, String message) {
    if (!context.mounted) return;
    FeedbackToast.of(context).show(SnackBar(content: Text(message)));
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
                    '${context.pageKindLabel(page.kind)} · ${page.summary}',
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
            child: Text(context.l10n.cancel),
          ),
        ],
      ),
    );
  }
}

/// 会话内临时提示气泡（AI 回复失败等）。仅内存态——不写库，
/// 不进入对话历史 / 记忆注入 / AI 上下文；发送下一条消息后即清除。
/// 只承载失败原因文案；「重新生成」入口在最后一条用户消息上。
class _DayDivider extends ConsumerWidget {
  final String label;

  const _DayDivider({required this.label});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scale = ref.watch(settingsProvider.select(chatTypeScaleOf));
    final dark = Theme.of(context).brightness == Brightness.dark;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space4),
      child: Row(
        children: [
          Expanded(
            child: Divider(
              color: AppTheme.surface3.withValues(alpha: 0.6),
              height: 1,
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: AppTheme.space3),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
              decoration: BoxDecoration(
                color: dark ? AppTheme.surface1 : AppTheme.surface2,
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: AppTheme.surface3.withValues(alpha: 0.6),
                  width: 0.5,
                ),
              ),
              child: Text(
                label,
                style: TextStyle(
                  fontSize: scale.divider,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.textTertiary,
                ),
              ),
            ),
          ),
          Expanded(
            child: Divider(
              color: AppTheme.surface3.withValues(alpha: 0.6),
              height: 1,
            ),
          ),
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
