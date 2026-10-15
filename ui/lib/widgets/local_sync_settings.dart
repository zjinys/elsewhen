import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

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
import '../sync/sync_object_conflicts_controller.dart';
import 'sync_object_conflicts_settings.dart';
import 'sync_wiki_conflicts_settings.dart';

class LocalSyncSettings extends StatefulWidget {
  const LocalSyncSettings({
    super.key,
    required this.controller,
    this.repository,
    this.conflictsController,
    this.wikiConflictsController,
    this.objectConflictsController,
    this.handoffController,
    this.rosterController,
  });
  final SyncSetupController? controller;
  final RustBridgeRepository? repository;

  /// 可注入测试替身；为空时由本 widget 自建并随自身销毁。
  final SyncProviderConflictsController? conflictsController;
  final SyncWikiConflictsController? wikiConflictsController;
  final SyncObjectConflictsController? objectConflictsController;

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
  late final SyncObjectConflictsController _objectConflicts;
  late final bool _ownsObjectConflicts;
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
    _ownsObjectConflicts = widget.objectConflictsController == null;
    _objectConflicts =
        widget.objectConflictsController ?? SyncObjectConflictsController();
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
    if (_ownsObjectConflicts) _objectConflicts.dispose();
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

        // —— 状态头卡：用户最关心的三件事（是否开启 / 本机是谁 / 用哪个
        // relay）放最上面，一眼可见；诊断细节收进高级区。——
        final statusLine = switch (state?.state) {
          'local_ready' => '同步已开启',
          'locked' => '同步已暂停（本机密钥已锁定）',
          'membership_inactive' => '同步已关闭（本机未被授权）',
          'requires_repair' => '同步需要修复',
          'not_configured' => '同步未开启',
          _ => controller.error == null ? '正在读取同步状态' : '同步状态暂不可用',
        };
        final roleLabel = switch ((
          state?.isOwner ?? false,
          state?.isProcessor ?? false,
        )) {
          (true, true) => '发起设备（当前加工）',
          (true, false) => '发起设备',
          (false, true) => '加工设备',
          (false, false) => '成员设备',
        };

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: theme.colorScheme.surfaceContainerHighest.withValues(
                  alpha: 0.4,
                ),
                borderRadius: BorderRadius.circular(10),
                border: Border.all(color: theme.colorScheme.outlineVariant),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(
                        ready
                            ? Icons.sync
                            : (configured
                                  ? Icons.sync_disabled
                                  : Icons.sync_problem),
                        size: 20,
                        color: ready
                            ? theme.colorScheme.primary
                            : theme.colorScheme.outline,
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          statusLine,
                          style: theme.textTheme.titleSmall?.copyWith(
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      if (controller.busy)
                        const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                    ],
                  ),
                  if (configured) ...[
                    const SizedBox(height: 8),
                    Text(
                      '本机：${state.deviceName ?? '未命名设备'} · $roleLabel',
                      style: theme.textTheme.bodyMedium,
                    ),
                    const SizedBox(height: 4),
                    _RelayLine(relayUrl: state.relayUrl, locked: !ready),
                  ],
                ],
              ),
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
            if (widget.repository != null && configured)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: SyncTickStatusLine(
                  ticks: widget.repository!.lastSyncTick,
                  errors: widget.repository!.lastSyncTickError,
                ),
              ),
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
            if (configured) ...[
              const SizedBox(height: 16),
              if (ready)
                SyncMembersSettings(controller: _roster)
              else
                Text('授权其他设备后，这里会显示设备列表。', style: theme.textTheme.bodySmall),
              const SizedBox(height: 16),
              if (ready)
                SyncProcessorHandoffSettings(
                  controller: _handoff,
                  isProcessor: state.isProcessor,
                  isOwner: state.isOwner,
                  candidates: _handoffCandidates(),
                  onChanged: _roster.refresh,
                ),
              if (ready && installation != null)
                SyncKeyHealthSettings(
                  api: FrbSyncKeyHealthApi(controller.keys),
                  installationId: installation,
                ),
              if (ready) SyncProviderConflictsSettings(controller: _conflicts),
              if (ready) SyncWikiConflictsSettings(controller: _wikiConflicts),
              if (ready)
                SyncObjectConflictsSettings(controller: _objectConflicts),
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
              // 诊断细节折叠：对排障有用，但不是日常要看的。
              ExpansionTile(
                tilePadding: EdgeInsets.zero,
                childrenPadding: const EdgeInsets.only(bottom: 8),
                title: Text('诊断信息', style: theme.textTheme.titleSmall),
                children: [
                  Align(
                    alignment: Alignment.centerLeft,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '待确认同步操作：${state.pendingOperations}',
                          style: theme.textTheme.bodySmall,
                        ),
                        if (state.datasetId != null) ...[
                          const SizedBox(height: 4),
                          Text('数据集标识', style: theme.textTheme.bodySmall),
                          SelectableText(
                            state.datasetId!,
                            style: theme.textTheme.bodySmall,
                          ),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
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

/// relay 行：有值时显示并可复制；锁定/未解锁时不猜值，只说「不可见」。
class _RelayLine extends StatelessWidget {
  const _RelayLine({required this.relayUrl, required this.locked});

  final String? relayUrl;
  final bool locked;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final relay = relayUrl;
    if (relay == null || relay.isEmpty) {
      return Text(
        locked ? '中继服务：解锁本机密钥后可见' : '中继服务：未连接',
        style: theme.textTheme.bodySmall?.copyWith(
          color: theme.colorScheme.outline,
        ),
      );
    }
    return Row(
      children: [
        Text('中继服务：', style: theme.textTheme.bodySmall),
        Expanded(
          child: SelectableText(
            relay,
            style: theme.textTheme.bodySmall,
            maxLines: 1,
          ),
        ),
        IconButton(
          visualDensity: VisualDensity.compact,
          iconSize: 16,
          tooltip: '复制中继地址',
          icon: const Icon(Icons.copy_outlined),
          onPressed: () {
            Clipboard.setData(ClipboardData(text: relay));
            ScaffoldMessenger.of(context)
                .showSnackBar(const SnackBar(content: Text('中继地址已复制')));
          },
        ),
      ],
    );
  }
}
