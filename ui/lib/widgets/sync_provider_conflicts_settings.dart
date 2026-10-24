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
          title: 'Provider 同步冲突',
          explainer:
              '密钥已掩码。选择「保留本地」或「接受远端」整包应用，不做字段拼接；地址提示仅为静态分析，不代表实际可达。',
          loaded: conflicts != null,
          busy: controller.busy,
          error: controller.error,
          emptyText: '没有待审阅的 Provider 同步冲突。',
          cards: [
            for (final conflict in conflicts ?? const []) _ConflictCard(
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
              '来自 ${conflict.senderDeviceName ?? conflict.senderDeviceId} 的修订',
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 8),
            SyncConflictSides(
              children: [
                _SideView(
                  title: '本机当前配置',
                  profiles: conflict.localProfiles,
                  activeId: conflict.localActiveId,
                ),
                _SideView(
                  title: '远端配置',
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
                  label: const Text('保留本地'),
                  onPressed: busy ? null : () => onResolve(false),
                ),
                FilledButton.icon(
                  icon: const Icon(Icons.cloud_download_outlined),
                  label: const Text('接受远端'),
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
          Text('（无配置）', style: theme.textTheme.bodySmall)
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
                  '${isActive ? '（当前激活）' : ''}',
                  style: theme.textTheme.bodyMedium,
                ),
              ),
            ],
          ),
          SelectableText(profile.baseUrl, style: theme.textTheme.bodySmall),
          Text(
            profile.apiKeyPresent ? 'Key：${profile.apiKeyMasked}' : 'Key：未设置',
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
                      _hintText(hint.code),
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

String _hintText(String code) => switch (code) {
  'provider_url_loopback' => '环回地址：仅本机可达，同步到其他设备后无法访问',
  'provider_url_private_network' => '内网地址：仅同一网络内设备可达',
  'provider_url_missing_scheme' => '缺少 http(s):// 前缀，多数客户端无法识别',
  'provider_url_scheme_unusual' => '非常规协议，可能不被支持',
  'provider_url_missing_host' => '缺少主机名',
  _ => '地址提示：$code',
};
