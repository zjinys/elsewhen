import '../l10n/localization.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_provider_conflicts_controller.dart';
import 'sync_conflict_section.dart';

/// Provider 同步冲突的只读审阅列表 + 显式裁决入口。
///
/// 密钥以掩码展示；地址携带静态可达性提示。裁决只支持「保留本地」/
/// 「接受远端」整包二选一，绝不做字段拼接。每条冲突绑定用户审阅时所见
/// 的本地/远端版本，由 Rust 侧失败关闭。
class SyncProviderConflictsSettings extends StatefulWidget {
  const SyncProviderConflictsSettings({super.key, required this.controller});

  final SyncProviderConflictsController controller;

  @override
  State<SyncProviderConflictsSettings> createState() =>
      _SyncProviderConflictsSettingsState();
}

class _SyncProviderConflictsSettingsState
    extends State<SyncProviderConflictsSettings> {
  bool _requested = false;

  @override
  Widget build(BuildContext context) {
    if (!_requested) {
      _requested = true;
      // 首次构建后再触发，避免 build 中发起异步。
      WidgetsBinding.instance.addPostFrameCallback((_) {
        widget.controller.refresh();
      });
    }
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final conflicts = controller.conflicts;
        return SyncConflictSection(
          icon: Icons.settings_suggest_outlined,
          title: context.l10n.l6ProviderSyncConflicts31e02,
          explainer: context.l10n.l6KeysAreMaskedKeepTheLocal2e55f,
          loaded: conflicts != null,
          busy: controller.busy,
          error: controller.error,
          emptyText: context.l10n.l6NoProviderSyncConflictsNeedReview4dc86,
          onRetry: controller.refresh,
          cards: [
            for (final conflict in conflicts ?? const [])
              _ConflictCard(
                key: ValueKey(conflict.revisionId),
                conflict: conflict,
                busy: controller.busy,
                onResolve: (acceptRemote) =>
                    controller.resolve(conflict, acceptRemote: acceptRemote),
              ),
          ],
        );
      },
    );
  }
}

class _ConflictCard extends StatelessWidget {
  const _ConflictCard({
    super.key,
    required this.conflict,
    required this.busy,
    required this.onResolve,
  });

  final api.ProviderSyncConflictDto conflict;
  final bool busy;
  final Future<bool> Function(bool acceptRemote) onResolve;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(vertical: 8),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.l10n.l6RevisionFromArg0beef3(
                (conflict.senderDeviceName ?? conflict.senderDeviceId)
                    .toString(),
              ),
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 8),
            SyncConflictSides(
              children: [
                _SideView(
                  title: context.l10n.l6CurrentLocalConfiguration683a0,
                  profiles: conflict.localProfiles,
                  activeId: conflict.localActiveId,
                ),
                _SideView(
                  title: context.l10n.l6RemoteConfiguration04e8c,
                  profiles: conflict.remoteProfiles,
                  activeId: conflict.remoteActiveId,
                ),
              ],
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  icon: const Icon(Icons.computer_outlined),
                  label: Text(context.l10n.l6KeepLocal6d6a5),
                  onPressed: busy ? null : () => onResolve(false),
                ),
                FilledButton.icon(
                  icon: const Icon(Icons.cloud_download_outlined),
                  label: Text(context.l10n.l6AcceptRemote4546a),
                  onPressed: busy ? null : () => onResolve(true),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _SideView extends StatelessWidget {
  const _SideView({
    required this.title,
    required this.profiles,
    required this.activeId,
  });

  final String title;
  final List<api.ProviderConflictProfileDto> profiles;
  final String? activeId;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.labelLarge),
        const SizedBox(height: 4),
        if (profiles.isEmpty)
          Text(
            context.l10n.l6NoConfigurationa33dc,
            style: theme.textTheme.bodySmall,
          )
        else
          ...profiles.map(
            (p) => _ProfileRow(profile: p, isActive: p.id == activeId),
          ),
      ],
    );
  }
}

class _ProfileRow extends StatelessWidget {
  const _ProfileRow({required this.profile, required this.isActive});

  final api.ProviderConflictProfileDto profile;
  final bool isActive;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '${profile.name} · ${profile.model}'
                  '${isActive ? context.l10n.activeProviderSuffix : ''}',
                  style: theme.textTheme.bodyMedium,
                ),
              ),
            ],
          ),
          SelectableText(profile.baseUrl, style: theme.textTheme.bodySmall),
          Text(
            profile.apiKeyPresent
                ? 'Key：${profile.apiKeyMasked}'
                : context.l10n.l6KeyNotSetefd96,
            style: theme.textTheme.bodySmall,
          ),
          ...profile.reachabilityHints.map(
            (hint) => Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    Icons.info_outline,
                    size: 14,
                    color: theme.colorScheme.tertiary,
                  ),
                  const SizedBox(width: 4),
                  Expanded(
                    child: Text(
                      _hintText(context, hint.code),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.tertiary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

String _hintText(BuildContext context, String code) => switch (code) {
  'provider_url_loopback' =>
    context.l10n.l6LoopbackAddressReachableOnlyFromThis5a7ca,
  'provider_url_private_network' =>
    context.l10n.l6PrivateNetworkAddressReachableOnlyOnf9ec8,
  'provider_url_missing_scheme' =>
    context.l10n.l6MissingHttpSPrefixMostClients85171,
  'provider_url_scheme_unusual' =>
    context.l10n.l6UnusualProtocolMayNotBeSupportedf9ae0,
  'provider_url_missing_host' => context.l10n.l6MissingHostNameb705c,
  _ => context.l10n.l6AddressHintArg03f1aa((code).toString()),
};
