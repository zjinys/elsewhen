part of '../scout_view.dart';

/// 离线可辨的平台身份；未知来源保留服务端名称和通用图标。
class _SourceIdentity {
  final String platform;
  final String topic;
  final String? asset;
  final Color color;
  const _SourceIdentity(this.platform, this.topic, this.asset, this.color);
}

_SourceIdentity _sourceIdentity(BuildContext context, String id, String? name) {
  final key = id.toLowerCase();
  if (key.startsWith('github')) {
    return _SourceIdentity(
      'GitHub',
      key.contains('rust')
          ? 'Rust'
          : key.contains('dart')
          ? 'Dart'
          : name ?? id,
      'github',
      const Color(0xFF9FAFC5),
    );
  }
  if (key.startsWith('producthunt') || key.startsWith('product-hunt')) {
    return _SourceIdentity(
      'Product Hunt',
      key.contains('daily') || key == 'product-hunt'
          ? context.l10n.scoutDailyProducts
          : name ?? id,
      'producthunt',
      const Color(0xFFDA552F),
    );
  }
  if (key.startsWith('gumroad')) {
    return _SourceIdentity(
      'Gumroad',
      key.contains('notion')
          ? context.l10n.scoutNotionTemplates
          : key.contains('planner')
          ? context.l10n.scoutDigitalPlanners
          : name ?? id,
      'gumroad',
      const Color(0xFFDF7CB2),
    );
  }
  if (key.startsWith('cws')) {
    return _SourceIdentity(
      'Chrome Web Store',
      key == 'cws-watchlist'
          ? context.l10n.scoutExtensionWatchlist
          : name ?? id,
      'googlechrome',
      const Color(0xFF58A67B),
    );
  }
  if (key.startsWith('appstore')) {
    return _SourceIdentity(
      'App Store',
      key == 'appstore-us-top-free' ? context.l10n.scoutUsTopFree : name ?? id,
      'appstore',
      const Color(0xFF4E9DE8),
    );
  }
  return _SourceIdentity(name ?? id, id, null, AppTheme.textSecondary);
}

class _SourceIcon extends StatelessWidget {
  final _SourceIdentity source;
  const _SourceIcon({required this.source});
  @override
  Widget build(BuildContext context) => Semantics(
    label: source.platform,
    image: true,
    child: Container(
      width: 34,
      height: 34,
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: source.color.withValues(alpha: .12),
        borderRadius: BorderRadius.circular(9),
      ),
      child: source.asset == null
          ? Icon(Icons.public, size: 18, color: source.color)
          : SvgPicture.asset(
              'assets/scout/${source.asset}.svg',
              excludeFromSemantics: true,
              colorFilter: ColorFilter.mode(source.color, BlendMode.srcIn),
            ),
    ),
  );
}

class ScoutSourceBadge extends ConsumerWidget {
  final String sourceId;
  const ScoutSourceBadge({super.key, required this.sourceId});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final source = _sourceIdentity(
      context,
      sourceId,
      _sourceName(ref, sourceId),
    );
    return Row(
      children: [
        _SourceIcon(source: source),
        const SizedBox(width: 9),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                source.platform,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textSecondary,
                ),
              ),
              if (source.topic != source.platform)
                Text(
                  source.topic,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 10.5,
                    color: AppTheme.textTertiary,
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }
}
