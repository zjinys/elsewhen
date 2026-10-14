import 'dart:io';

import 'package:flutter/material.dart';

import '../bridge/rust_bridge_repository.dart';
import '../bridge/api.dart' as api;
import '../sync/sync_processor_handoff_controller.dart';
import '../sync/sync_roster_controller.dart';
import '../sync/sync_provider_conflicts_controller.dart';
import '../sync/sync_wiki_conflicts_controller.dart';
import '../sync/sync_setup_controller.dart';
import 'sync_tick_status.dart';
import 'owner_sync_pairing_settings.dart';
import 'joiner_sync_pairing_settings.dart';
import 'sync_enrollment_progress.dart';
import 'sync_key_health_settings.dart';
import 'sync_members_settings.dart';
import 'sync_processor_handoff_settings.dart';
import 'sync_provider_conflicts_settings.dart';
import 'sync_wiki_conflicts_settings.dart';

class LocalSyncSettings extends StatefulWidget {
  const LocalSyncSettings({
    super.key,
    required this.controller,
    this.repository,
    this.conflictsController,
    this.wikiConflictsController,
    this.handoffController,
    this.rosterController,
  });
  final SyncSetupController? controller;
  final RustBridgeRepository? repository;

  /// 可注入测试替身；为空时由本 widget 自建并随自身销毁。
  final SyncProviderConflictsController? conflictsController;
  final SyncWikiConflictsController? wikiConflictsController;

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
  late final SyncWikiConflictsController _wikiConflicts;
  late final bool _ownsWikiConflicts;
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
    _ownsWikiConflicts = widget.wikiConflictsController == null;
    _wikiConflicts =
        widget.wikiConflictsController ?? SyncWikiConflictsController();
    _ownsHandoff = widget.handoffController == null;
    _handoff = widget.handoffController ?? SyncProcessorHandoffController();
    _ownsRoster = widget.rosterController == null;
    _roster = widget.rosterController ?? SyncRosterController();
  }

  @override
  void dispose() {
    _name.dispose();
    if (_ownsConflicts) _conflicts.dispose();
    if (_ownsWikiConflicts) _wikiConflicts.dispose();
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
                  '本机不是指定加工设备，不认领后台任务。',
                  style: theme.textTheme.bodySmall,
                ),
              ],
              if (widget.repository != null)
                SyncTickStatusLine(
                  ticks: widget.repository!.lastSyncTick,
                  errors: widget.repository!.lastSyncTickError,
                ),
              if (ready)
                SyncProcessorHandoffSettings(
                  controller: _handoff,
                  isProcessor: state.isProcessor,
                  isOwner: state.isOwner,
                  candidates: _handoffCandidates(),
                  onChanged: _roster.refresh,
                ),
              if (ready) SyncProviderConflictsSettings(controller: _conflicts),
              if (ready) SyncWikiConflictsSettings(controller: _wikiConflicts),
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
