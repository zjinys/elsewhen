part of '../message_area.dart';

/// 主对话中的精选产品信号；最多三条，无可靠新证据不推送。
class ScoutBriefBubble extends ConsumerWidget {
  final Message message;

  const ScoutBriefBubble({super.key, required this.message});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scale = ref.watch(settingsProvider.select(chatTypeScaleOf));
    final bubbleStyle = ChatBubbleStyle.of(context);
    // Old volume-only cards remain stored, but no longer interrupt the chat.
    final content = message.content.trim();
    if (!content.startsWith('scout-brief-v2\n')) return const SizedBox.shrink();
    Map<String, dynamic> data;
    try {
      data = jsonDecode(
        content.substring('scout-brief-v2\n'.length),
      ) as Map<String, dynamic>;
    } catch (_) {
      return const SizedBox.shrink();
    }
    final items = (data['items'] as List? ?? [])
        .whereType<Map>()
        .take(3)
        .toList();
    if (items.isEmpty) return const SizedBox.shrink();

    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: bubbleStyle.isLight ? 32 : 30,
            height: bubbleStyle.isLight ? 32 : 30,
            decoration: BoxDecoration(
              color: AppTheme.accentPrimary.withValues(alpha: 0.12),
              shape: BoxShape.circle,
              border: Border.all(
                color: AppTheme.accentPrimary.withValues(alpha: 0.25),
              ),
            ),
            child: Icon(
              Icons.radar_outlined,
              size: DesignTokens.iconMicro,
              color: bubbleStyle.accentInk,
            ),
          ),
          const SizedBox(width: AppTheme.space3),
          Flexible(
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxWidth: bubbleStyle.assistantMaxWidth,
              ),
              child: InkWell(
                borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                onTap: () {
                  ref
                      .read(scoutWorkspaceProvider.notifier)
                      .set(ScoutWorkspace.discover);
                  ref.read(scoutProductViewProvider.notifier).set('signals');
                  ref.read(scoutProductQueryProvider.notifier).set('');
                  ref.read(scoutSourceFilterProvider.notifier).set(null);
                  ref.read(scoutProductSourcesProvider.notifier).clear();
                  ref.read(sidebarTabProvider.notifier).set(SidebarTab.scout);
                },
                child: Container(
                  padding: EdgeInsets.symmetric(
                    horizontal: bubbleStyle.isLight ? 16 : 14,
                    vertical: 12,
                  ),
                  decoration: bubbleStyle.assistantCard(
                    emphasized: true,
                    darkBorder: AppTheme.accentPrimary.withValues(alpha: 0.32),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        context.l10n.scoutBriefHighlights,
                        style: TextStyle(
                          fontSize: scale.meta,
                          fontWeight: FontWeight.w600,
                          color: bubbleStyle.accentInk,
                        ),
                      ),
                      for (final item in items)
                        Padding(
                          padding: const EdgeInsets.only(top: 12),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                '${item['title'] ?? ''}',
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: scale.bubble,
                                  fontWeight: FontWeight.w600,
                                  color: AppTheme.textPrimary,
                                ),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                _scoutBriefReason(context, item),
                                style: TextStyle(
                                  fontSize: scale.bubbleSecondary,
                                  color: AppTheme.textSecondary,
                                  height: 1.4,
                                ),
                              ),
                            ],
                          ),
                        ),
                      const SizedBox(height: AppTheme.space2),
                      Text(
                        context.l10n.scoutOpenFindings,
                        style: TextStyle(
                          fontSize: scale.meta,
                          color: bubbleStyle.accentInk,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

String _scoutBriefReason(BuildContext context, Map item) {
  final r = item['reason'] as Map? ?? {};
  final metric = switch (r['metric']) {
    'stargazers_count' => context.l10n.scoutMetricStars,
    'forks_count' => context.l10n.scoutMetricForks,
    'rank' => context.l10n.scoutMetricRank,
    _ => context.l10n.scoutMetricReviews,
  };
  String number(dynamic value) => value is num && value == value.roundToDouble()
      ? '${value.toInt()}'
      : '$value';
  final from = r['from'], to = r['to'];
  final text = r['kind'] == 'price_change' && from is num && to is num
      ? '${context.l10n.scoutKnownPrice} · ${item['currency'] ?? ''} ${(from / 100).toStringAsFixed(2)} → ${(to / 100).toStringAsFixed(2)}'
      : r['kind'] == 'rank_rise' && from is num && to is num
      ? '$metric ${number(from)} → ${number(to)} · ${context.l10n.scoutRankUp((from - to).toInt())}'
      : '${r['days']}d · $metric ${number(from)} → ${number(to)}';
  String date(dynamic value) {
    final d = DateTime.tryParse('$value');
    return d == null
        ? ''
        : DateFormat.Md(context.l10n.localeName).format(d.toLocal());
  }

  return '$text\n${date(r['from_at'])} – ${date(r['to_at'])}';
}
