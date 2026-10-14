import 'dart:io';

import 'package:flutter/material.dart';

import '../bridge/rust_bridge_repository.dart';
import '../bridge/api.dart' as api;
import '../sync/sync_processor_handoff_controller.dart';
import '../sync/sync_roster_controller.dart';
import '../sync/sync_provider_conflicts_controller.dart';
import '../sync/sync_setup_controller.dart';
import 'owner_sync_pairing_settings.dart';
import 'joiner_sync_pairing_settings.dart';
import 'sync_enrollment_progress.dart';
import 'sync_key_health_settings.dart';
import 'sync_members_settings.dart';
import 'sync_processor_handoff_settings.dart';
import 'sync_provider_conflicts_settings.dart';

class LocalSyncSettings extends StatefulWidget {
  const LocalSyncSettings({
    super.key,
    required this.controller,
    this.repository,
    this.conflictsController,
    this.handoffController,
    this.rosterController,
  });
  final SyncSetupController? controller;
  final RustBridgeRepository? repository;

  /// 可注入测试替身；为空时由本 widget 自建并随自身销毁。
  final SyncProviderConflictsController? conflictsController;

  /// 可注入测试替身；为空时由本 widget 自建并随自身销毁。
  final SyncProcessorHandoffController? handoffController;

  /// 可注入测试替身；为空时由本 widget 自建并随自身销毁。
  final SyncRosterController? rosterController;

  @override
  State<LocalSyncSettings> createState() => _LocalSyncSettingsState();
}

class _LocalSyncSettingsState extends State<LocalSyncSettings> {
  final _name = TextEditingController(text: '这台设备');
  bool _consent = false;
  late final SyncProviderConflictsController _conflicts;
  late final bool _ownsConflicts;
  late final SyncProcessorHandoffController _handoff;
  late final bool _ownsHandoff;
  late final SyncRosterController _roster;
  late final bool _ownsRoster;

  @override
  void initState() {
    super.initState();
    _ownsConflicts = widget.conflictsController == null;
    _conflicts =
        widget.conflictsController ?? SyncProviderConflictsController();
    _ownsHandoff = widget.handoffController == null;
    _handoff = widget.handoffController ?? SyncProcessorHandoffController();
    _ownsRoster = widget.rosterController == null;
    _roster = widget.rosterController ?? SyncRosterController();
  }

  @override
  void dispose() {
    _name.dispose();
    if (_ownsConflicts) _conflicts.dispose();
    if (_ownsHandoff) _handoff.dispose();
    if (_ownsRoster) _roster.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    if (controller == null) {
      return const Text('当前存储后端不支持设备同步。');
    }
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final state = controller.state;
        final theme = Theme.of(context);
        final configured = state != null && state.state != 'not_configured';
        final installation = state?.installationId;
        final ready = state?.state == 'local_ready';
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(switch (state?.state) {
              'local_ready' => '本机密钥已解锁',
              'locked' => '本机同步密钥已锁定',
              'membership_inactive' => '本机尚未获授权，或授权已撤销',
              'requires_repair' => '本机同步身份需要恢复',
              'not_configured' => '尚未准备本机同步身份',
              _ => controller.error == null ? '正在读取同步状态' : '同步状态暂不可用',
            }, style: theme.textTheme.titleSmall),
            const SizedBox(height: 8),
            Text(
              '配对与跨设备传输由 owner 邀请发起。本机准备不会启动联网同步，原有记录和 Provider 配置会保留。',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(
              '完成本机准备后，Provider API Key 将加密保存。锁定后仍可查看配置和保存记录，模型调用需先解锁；已有备份中的旧明文不会因此清除。',
              style: theme.textTheme.bodySmall,
            ),
            if (controller.error != null) ...[
              const SizedBox(height: 12),
              Semantics(
                liveRegion: true,
                child: Text(
                  controller.error!,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ),
            ],
            if (controller.busy) ...[
              const SizedBox(height: 16),
              const Row(
                children: [
                  SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                  SizedBox(width: 10),
                  Expanded(child: Text('正在处理同步步骤，请等待…')),
                ],
              ),
            ],
            if (!configured && state != null && controller.previewEnabled) ...[
              const SizedBox(height: 16),
              TextField(
                controller: _name,
                enabled: !controller.busy,
                maxLength: 64,
                decoration: const InputDecoration(
                  labelText: '设备名称',
                  border: OutlineInputBorder(),
                ),
              ),
              CheckboxListTile(
                value: _consent,
                enabled: !controller.busy,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                title: const Text('我理解：后续授权设备加入时，会共享所有 Provider 配置和 API Key。'),
                onChanged: controller.busy
                    ? null
                    : (value) => setState(() => _consent = value ?? false),
              ),
              const SizedBox(height: 8),
              FilledButton.icon(
                icon: const Icon(Icons.key_outlined),
                label: const Text('准备本机'),
                onPressed: controller.busy || !_consent
                    ? null
                    : () => controller.createOwner(
                        _name.text,
                        Platform.operatingSystem,
                        consent: _consent,
                      ),
              ),
            ] else if (!configured && state != null) ...[
              const SizedBox(height: 12),
              Text(
                '设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。',
                style: theme.textTheme.bodySmall,
              ),
            ],
            if (state == null && !controller.busy)
              TextButton(
                onPressed: controller.refresh,
                child: const Text('刷新状态'),
              ),
            if (state != null && state.state != 'not_configured') ...[
              const SizedBox(height: 16),
              if (state.deviceName != null) Text('设备：${state.deviceName}'),
              if (state.datasetId != null) ...[
                const SizedBox(height: 8),
                const Text('数据集标识'),
                SelectableText(
                  state.datasetId!,
                  style: theme.textTheme.bodySmall,
                ),
              ],
              const SizedBox(height: 8),
              Text('授权的其他设备：${state.activePeers}'),
              const SizedBox(height: 4),
              Text('待确认同步操作：${state.pendingOperations}'),
              if (!state.isProcessor && state.isOwner) ...[
                const SizedBox(height: 4),
                Text(
                  '本设备不是指定处理设备：后台分析/消化任务由指定设备认领。',
                  style: theme.textTheme.bodySmall,
                ),
              ],
              _NetworkSyncStatus(repository: widget.repository),
              if (ready)
                SyncProcessorHandoffSettings(
                  controller: _handoff,
                  isProcessor: state.isProcessor,
                  isOwner: state.isOwner,
                  candidates: _handoffCandidates(),
                  onChanged: _roster.refresh,
                ),
              if (ready) SyncProviderConflictsSettings(controller: _conflicts),
              const SizedBox(height: 12),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  if (ready)
                    OutlinedButton.icon(
                      onPressed: controller.busy ? null : controller.lock,
                      icon: const Icon(Icons.lock_outline),
                      label: const Text('锁定本机密钥'),
                    )
                  else if (state.state == 'locked')
                    FilledButton.icon(
                      onPressed: controller.busy ? null : controller.unlock,
                      icon: const Icon(Icons.lock_open_outlined),
                      label: const Text('解锁本机密钥'),
                    ),
                  TextButton(
                    onPressed: controller.busy ? null : controller.refresh,
                    child: const Text('刷新状态'),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              if (ready && installation != null)
                SyncKeyHealthSettings(
                  api: FrbSyncKeyHealthApi(controller.keys),
                  installationId: installation,
                ),
              if (ready) SyncMembersSettings(controller: _roster),
            ],
            if (ready && state?.isOwner == true && controller.previewEnabled)
              OwnerSyncPairingSettings(controller: controller),
            if (controller.autoEnrollment)
              SyncEnrollmentProgress(controller: controller.enrollment),
            if (controller.previewEnabled)
              JoinerSyncPairingSettings(controller: controller.joiner),
          ],
        );
      },
    );
  }

  /// 可接收加工权的设备：活跃、非本机成员（owner 也可接收，用于移交回 owner）。
  List<api.SyncRosterMemberDto> _handoffCandidates() {
    final roster = _roster.roster;
    final local = roster?.localDeviceId;
    if (roster == null || local == null) return const [];
    return roster.members
        .where((member) => member.active && member.deviceId != local)
        .toList();
  }
}

/// P3 网络同步 tick 的只读状态（范围/最近结果/错误），由后台 worker 更新。
class _NetworkSyncStatus extends StatelessWidget {
  const _NetworkSyncStatus({required this.repository});
  final RustBridgeRepository? repository;

  @override
  Widget build(BuildContext context) {
    final repo = repository;
    if (repo == null) return const SizedBox.shrink();
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([repo.lastSyncTick, repo.lastSyncTickError]),
      builder: (context, _) {
        final tick = repo.lastSyncTick.value;
        final error = repo.lastSyncTickError.value;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (error != null) ...[
              const SizedBox(height: 4),
              Text('最近同步错误：$error', style: theme.textTheme.bodySmall),
            ] else if (tick != null) ...[
              const SizedBox(height: 4),
              Text(
                tick.ran
                    ? '最近同步：上行 ${tick.uploaded} · 到达 ${tick.pulledAccepted}'
                          '${tick.pulledWaiting > 0 ? ' · 等待依赖 ${tick.pulledWaiting}' : ''}'
                          '${tick.acknowledgementFailed ? ' · 回执失败' : ''}'
                    : '最近同步：本轮跳过（锁定或后台忙碌）',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        );
      },
    );
  }
}
