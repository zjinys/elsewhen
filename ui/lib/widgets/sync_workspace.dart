import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';
import 'local_sync_settings.dart';

/// 主区「同步」工作区：LocalSyncSettings 整包（左栏仪表的完整操作面）。
/// 入口是侧栏一级「同步」；设置页不再有同步 tab——单入口，状态与操作同住。
class SyncWorkspace extends StatelessWidget {
  const SyncWorkspace({super.key, required this.repository});

  final RustBridgeRepository repository;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: _buildSection(
        title: '设备同步',
        icon: Icons.devices_outlined,
        child: LocalSyncSettings(
          controller: repository.localSync,
          repository: repository,
        ),
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
                    fontSize: 15,
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

/// 左栏「同步」概览列（280px 窄面）：只放健康度仪表，不放大段操作。
/// 接收任意 StorageRepository：非桌面后端显示「暂不可用」占位。
/// 三行判据：谁在本机、成员齐不齐、最近心跳走没走。冲突计数待
/// 冲突清单接口出便宜查询再点亮（本版如实不显示）。
class SyncNavSummary extends StatelessWidget {
  const SyncNavSummary({super.key, required this.repository});

  final dynamic repository;

  @override
  Widget build(BuildContext context) {
    if (repository is! RustBridgeRepository) {
      return Padding(
        padding: const EdgeInsets.all(AppTheme.space4),
        child: Text(
          '同步在此平台暂不可用',
          style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
        ),
      );
    }
    final bridge = repository as RustBridgeRepository;
    return FutureBuilder<api.SyncRosterDto>(
      future: api.getSyncRoster(),
      builder: (context, snapshot) {
        if (snapshot.connectionState != ConnectionState.done) {
          return const Center(child: CircularProgressIndicator(strokeWidth: 2));
        }
        if (snapshot.hasError) {
          return Padding(
            padding: const EdgeInsets.all(AppTheme.space4),
            child: Text(
              '同步状态读取失败\n（同步可能尚未启用）',
              textAlign: TextAlign.center,
              style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
            ),
          );
        }
        final roster = snapshot.data!;
        final members = roster.members;
        final online = members.where((m) => m.active).length;
        final local = members.where((m) => m.isLocal).toList();
        final localName = local.isEmpty ? '本机' : local.first.name;

        return Padding(
          padding: const EdgeInsets.all(AppTheme.space4),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '同步',
                style: TextStyle(
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(height: AppTheme.space4),
              _row(
                icon: Icons.computer_outlined,
                text: localName,
                badge: local.isNotEmpty && local.first.isProcessor
                    ? '处理器'
                    : null,
              ),
              _row(
                icon: Icons.devices_outlined,
                text: '设备 $online/${members.length} 在线',
              ),
              ValueListenableBuilder<api.SyncTickResultDto?>(
                valueListenable: bridge.lastSyncTick,
                builder: (context, tick, _) {
                  if (tick == null) {
                    return _row(
                      icon: Icons.access_time_outlined,
                      text: '尚未同步过',
                      muted: true,
                    );
                  }
                  return _row(
                    icon: tick.ran
                        ? Icons.check_circle_outline
                        : Icons.pause_circle_outline,
                    text: tick.ran
                        ? '最近心跳正常 · 待发 ${tick.stagedOperations}'
                        : '心跳待命（空闲）',
                    muted: !tick.ran,
                  );
                },
              ),
              const Spacer(),
            ],
          ),
        );
      },
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
          Text(text, style: TextStyle(fontSize: 12.5, color: color)),
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
                style: TextStyle(fontSize: 10, color: AppTheme.accentPrimary),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
