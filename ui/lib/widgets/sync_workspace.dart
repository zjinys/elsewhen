import '../l10n/localization.dart';

import 'package:material_ui/material_ui.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';
import '../theme/design_tokens.dart';
import 'local_sync_settings.dart';

/// 主区「同步」工作区：健康度仪表（成员/心跳）+ LocalSyncSettings 整包。
/// 入口是侧栏一级「同步」；设置页不再有同步 tab——单入口，状态与操作同住。
/// 原侧栏概览列的内容已整体搬进本工作区，抽屉里不再重复。
class SyncWorkspace extends StatelessWidget {
  const SyncWorkspace({super.key, required this.repository});

  final RustBridgeRepository repository;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        children: [
          _buildSection(
            title: context.l10n.sync,
            icon: Icons.monitor_heart_outlined,
            child: SyncHealthPanel(repository: repository),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.syncNotificationChannel,
            icon: Icons.devices_outlined,
            child: LocalSyncSettings(
              controller: repository.localSync,
              repository: repository,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildSection({
    required String title,
    required IconData icon,
    required Widget child,
  }) {
    return Container(
      padding: const EdgeInsets.all(24),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(
          color: AppTheme.accentPrimary.withValues(alpha: 0.1),
        ),
      ),
      // 透明 Material 垫底（同 settings_screen._buildSection）：带背景色的
      // DecoratedBox 不能挡在 ListTile 与其 Material 祖先之间，否则同步设置里
      // 的 CheckboxListTile / 成员卡背景与水波纹不可见，debug 直接断言崩。
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, color: AppTheme.accentPrimary, size: 20),
                ),
                const SizedBox(width: 10),
                Text(
                  title,
                  style: TextStyle(
                    fontSize: TypographyTokens.title,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
              ],
            ),
            const SizedBox(height: AppTheme.space4),
            child,
          ],
        ),
      ),
    );
  }
}

/// 「同步」健康度仪表：谁在本机、成员齐不齐、最近心跳走没走三行判据。
/// 原侧栏概览列，现直接住在主区同步工作区的第一节卡片里；标题与外边距
/// 由 [_buildSection] 提供。接收任意 StorageRepository：非桌面后端显示
/// 「暂不可用」占位。冲突计数待冲突清单接口出便宜查询再点亮（本版如实
/// 不显示）。
class SyncHealthPanel extends StatelessWidget {
  const SyncHealthPanel({super.key, required this.repository});

  final dynamic repository;

  @override
  Widget build(BuildContext context) {
    if (repository is! RustBridgeRepository) {
      return Padding(
        padding: const EdgeInsets.all(AppTheme.space4),
        child: Text(
          context.l10n.syncPlatformUnavailable,
          style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
        ),
      );
    }
    final bridge = repository as RustBridgeRepository;
    return ListenableBuilder(
      listenable: Listenable.merge([
        bridge.localSync,
        bridge.localSync.enrollment,
        bridge.lastSyncTick,
      ]),
      builder: (context, _) => FutureBuilder<api.SyncRosterDto>(
        future: api.getSyncRoster(),
        builder: (context, snapshot) {
          if (!snapshot.hasData &&
              snapshot.connectionState != ConnectionState.done) {
            return const Center(
              child: CircularProgressIndicator(strokeWidth: 2),
            );
          }
          if (snapshot.hasError) {
            return Padding(
              padding: const EdgeInsets.all(AppTheme.space4),
              child: Text(
                context.l10n.l6CouldNotLoadSyncStatusSyncb8a20,
                textAlign: TextAlign.center,
                style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
              ),
            );
          }
          final roster = snapshot.data!;
          final members = roster.members;
          // 「设备 x/y 在线」的分母只数活跃成员：名册里保留的已撤销行是
          // 给 owner 审计用的，不是当前设备群——解绑后这里若仍按 members
          // 全量计数，数字永远不降，看起来像解绑没生效。
          final activeMembers = members.where((m) => m.active).toList();

          final revoked = members.length - activeMembers.length;
          final local = members.where((m) => m.isLocal).toList();
          final localName = local.isEmpty
              ? context.l10n.l6ThisDevice8b4c1
              : local.first.name;

          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _row(
                icon: Icons.computer_outlined,
                text: localName,
                badge: local.isNotEmpty && local.first.isProcessor
                    ? context.l10n.l6Processorb4366
                    : null,
              ),
              _row(
                icon: Icons.devices_outlined,
                text: context.l10n.syncConnectedDevices(
                  activeMembers.length.toString(),
                ),
                // 名册里保留的已撤销行是审计用的，不算在册数；有撤销时
                // 用 badge 提示，数字才不会在解绑后纹丝不动。
                badge: revoked > 0
                    ? '${context.l10n.l6Revoked61063} $revoked'
                    : null,
              ),
              ValueListenableBuilder<api.SyncTickResultDto?>(
                valueListenable: bridge.lastSyncTick,
                builder: (context, tick, _) {
                  if (tick == null) {
                    return _row(
                      icon: Icons.access_time_outlined,
                      text: context.l10n.l6NoSyncYet8ce44,
                      muted: true,
                    );
                  }
                  return _row(
                    icon: tick.ran
                        ? Icons.check_circle_outline
                        : Icons.pause_circle_outline,
                    text: tick.ran
                        ? context.l10n.syncLastRound
                        : context.l10n.l6HeartbeatOnStandbyIdleda794,
                    muted: !tick.ran,
                  );
                },
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _row({
    required IconData icon,
    required String text,
    String? badge,
    bool muted = false,
  }) {
    final color = muted ? AppTheme.textTertiary : AppTheme.textSecondary;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Row(
        children: [
          Icon(icon, size: 15, color: color),
          const SizedBox(width: 8),
          Flexible(
            child: Text(text, style: TextStyle(fontSize: 12, color: color)),
          ),
          if (badge != null) ...[
            const SizedBox(width: 6),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(999),
              ),
              child: Text(
                badge,
                style: TextStyle(fontSize: 11, color: AppTheme.accentPrimary),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
