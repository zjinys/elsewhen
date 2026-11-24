import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../bridge/api.dart' as api;
import '../l10n/localization.dart';
import '../providers/scout_provider.dart';
import '../theme/app_theme.dart';

/// Scout 来源的离线可辨平台身份 + 图标组件。
///
/// 侧栏来源面板、主区产品卡、来源角标共用：未知来源保留服务端名称
/// 和通用图标。
class ScoutSourceIdentity {
  final String platform;
  final String topic;
  final String? asset;
  final Color color;
  const ScoutSourceIdentity(this.platform, this.topic, this.asset, this.color);
}

ScoutSourceIdentity scoutSourceIdentity(
  BuildContext context,
  String id,
  String? name,
) {
  final key = id.toLowerCase();
  if (key.startsWith('github')) {
    return ScoutSourceIdentity(
      'GitHub',
      key == 'github-newcomers'
          ? context.l10n.scoutRecentProjects
          : key.contains('rust')
          ? 'Rust'
          : key.contains('dart')
          ? 'Dart'
          : name ?? id,
      'github',
      const Color(0xFF9FAFC5),
    );
  }
  if (key.startsWith('producthunt') || key.startsWith('product-hunt')) {
    return ScoutSourceIdentity(
      'Product Hunt',
      key.contains('daily') || key == 'product-hunt'
          ? context.l10n.scoutDailyProducts
          : name ?? id,
      'producthunt',
      const Color(0xFFDA552F),
    );
  }
  if (key.startsWith('gumroad')) {
    return ScoutSourceIdentity(
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
    return ScoutSourceIdentity(
      'Chrome Web Store',
      key == 'cws-discover'
          ? context.l10n.scoutExtensionDiscovery
          : key == 'cws-watchlist'
          ? context.l10n.scoutExtensionWatchlist
          : name ?? id,
      'googlechrome',
      const Color(0xFF58A67B),
    );
  }
  if (key.startsWith('appstore')) {
    return ScoutSourceIdentity(
      'App Store',
      key == 'appstore-us-top-free' ? context.l10n.scoutUsTopFree : name ?? id,
      'appstore',
      const Color(0xFF4E9DE8),
    );
  }
  return ScoutSourceIdentity(name ?? id, id, null, AppTheme.textSecondary);
}

String scoutSourceDisplayName(WidgetRef ref, String id) {
  for (final s
      in ref.watch(scoutSourcesProvider).value ?? <api.ScoutSourceDto>[]) {
    if (s.id == id) return s.name;
  }
  return id;
}

class ScoutSourceIcon extends StatelessWidget {
  final ScoutSourceIdentity source;
  const ScoutSourceIcon({super.key, required this.source});
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
    final source = scoutSourceIdentity(
      context,
      sourceId,
      scoutSourceDisplayName(ref, sourceId),
    );
    return Row(
      children: [
        ScoutSourceIcon(source: source),
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
                    fontSize: 11,
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
