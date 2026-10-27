import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../bridge/rust_bridge_repository.dart';
import '../utils/device_name.dart';
import '../bridge/api.dart' as api;
import '../sync/sync_processor_handoff_controller.dart';
import '../sync/sync_dataset_switch_api.dart';
import '../sync/sync_repair_api.dart';
import '../sync/sync_roster_controller.dart';
import 'dataset_switch_card.dart';
import '../sync/sync_provider_conflicts_controller.dart';
import '../sync/sync_wiki_conflicts_controller.dart';
import '../sync/sync_setup_controller.dart';
import 'sync_tick_status.dart';
import 'owner_sync_pairing_settings.dart';
import 'joiner_sync_pairing_settings.dart';
import 'sync_key_health_settings.dart';
import 'sync_members_settings.dart';
import 'sync_processor_handoff_settings.dart';
import 'sync_provider_conflicts_settings.dart';
import '../sync/sync_object_conflicts_controller.dart';
import '../theme/app_theme.dart';
import 'sync_object_conflicts_settings.dart';
import 'sync_wiki_conflicts_settings.dart';

part 'local_sync_parts/local_sync_device_card.dart';

// ── 本机同步设置拆出设备信息卡（纯类段平移，零行为变化）──

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
    this.datasetApi,
    this.repairApi,
  });
  final SyncSetupController? controller;

  /// 数据集切换注入缝；为空用默认原生实现（见 DatasetSwitchCard）。
  final SyncDatasetSwitchApi? datasetApi;

  /// 「清除本机同步身份」注入缝；为空用默认原生实现。
  final SyncRepairApi? repairApi;
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

/// 同步域的两个独立关切：「绑定」（准备本机/邀请/加入，一次性流程）与
/// 「管理」（状态/设备列表/移交/密钥/冲突，日常运维）。曾挤在一列里，
/// 用户反馈绑定流程找不到重点——拆成 tab。
enum _SyncTab { bind, manage }

class _LocalSyncSettingsState extends State<LocalSyncSettings> {
  _SyncTab _tab = _SyncTab.bind;
  bool _repairBusy = false;
  bool _tabTouched = false;
  final _name = TextEditingController(text: '这台设备');
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
    // 缺省设备名用真机器名；用户已改过名将保持原值。
    defaultDeviceName().then((name) {
      if (name != null && mounted && _name.text == '这台设备') {
        setState(() => _name.text = name);
      }
    });
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
        // 未配置时落在「绑定」tab（当前要办事的地方），配置后默认「管理」；
        // 用户主动点过后以用户选择为准。
        final tab = _tabTouched
            ? _tab
            : (configured ? _SyncTab.manage : _SyncTab.bind);
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
        // 角色用用户语言：主设备=创建数据集的设备；「负责后台 AI 分析」
        // 是 processor 的职责说明（内部术语「发起/加工」曾让用户看不懂）。
        final roleLabel = switch ((
          state?.isOwner ?? false,
          state?.isProcessor ?? false,
        )) {
          (true, true) => '主设备 · 负责后台 AI 分析',
          (true, false) => '主设备',
          (false, true) => '成员设备 · 负责后台 AI 分析',
          (false, false) => '成员设备',
        };

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // 绑定前的第一步：先给本机起名，再点按钮走明确确认（用户要求：
            // 名称放在状态卡之前，按钮紧跟输入框，确认改成弹窗）。
            if (!configured && state != null && controller.previewEnabled) ...[
              _buildPrepareRow(controller),
              const SizedBox(height: 12),
            ],
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
            // 缺陷 D1：requires_repair 是状态机里唯一没有出口的状态。这里给出
            // 唯一的出口，且只在它出现 —— 正常身份不会看到这个按钮。
            if (state?.state == 'requires_repair') ...[
              const SizedBox(height: 12),
              _buildRepairExit(theme, controller),
            ],
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
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SyncTickStatusLine(
                      ticks: widget.repository!.lastSyncTick,
                      errors: widget.repository!.lastSyncTickError,
                    ),
                    // 投递状态紧跟 tick 行：一个是「这一拍做了什么」，
                    // 一个是「现在卡在哪一步」，两者分开看就无法对齐。
                    SyncDeliverStatusLine(
                      statuses: widget.repository!.syncDeliveryStatus,
                    ),
                    // D4：积压的数据库口径（对端未确认条数/字节）。投递流只说
                    // 「这一拍在等谁」，说不出「一共压着多少」。
                    SyncBacklogLine(
                      envelopes: state.pendingWireEnvelopes,
                      bytes: state.pendingBytes,
                    ),
                  ],
                ),
              ),
            const SizedBox(height: 12),
            SegmentedButton<_SyncTab>(
              segments: const [
                ButtonSegment(
                  value: _SyncTab.bind,
                  label: Text('绑定'),
                  icon: Icon(Icons.link_outlined),
                ),
                ButtonSegment(
                  value: _SyncTab.manage,
                  label: Text('管理'),
                  icon: Icon(Icons.tune_outlined),
                ),
              ],
              selected: {tab},
              onSelectionChanged: (selection) => setState(() {
                _tab = selection.first;
                _tabTouched = true;
              }),
            ),
            DatasetSwitchCard(
              api: widget.datasetApi ?? const FrbSyncDatasetSwitchApi(),
              catalogChanges: controller.joiner,
            ),
            const SizedBox(height: 12),
            if (tab == _SyncTab.bind)
              _buildBindContent(theme, controller, state, configured)
            else
              _buildManageContent(
                theme,
                controller,
                state,
                configured,
                installation,
              ),
          ],
        );
      },
    );
  }

  /// `requires_repair` 的唯一出口（roadmap C3 / 缺陷 D1）。
  ///
  /// 这个状态的真实现场：配对写到一半被中断 / 掉电 / 恢复备份 —— 本机有同步身份
  /// 但没有名册授权，此前既不能修复也不能重来，用户只能删库（连笔记一起没）。
  Widget _buildRepairExit(ThemeData theme, SyncSetupController controller) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.errorContainer.withValues(alpha: 0.35),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: theme.colorScheme.error.withValues(alpha: 0.4)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '本机的同步身份不完整（常见于配对过程中断、掉电或恢复备份）。'
            '可以清除本机同步身份后重新配对 —— 笔记、对话、知识库都会保留。',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 10),
          Align(
            alignment: Alignment.centerLeft,
            child: OutlinedButton.icon(
              onPressed: _repairBusy
                  ? null
                  : () => _confirmClearLocalSyncIdentity(theme, controller),
              icon: const Icon(Icons.build_circle_outlined, size: 18),
              label: Text(_repairBusy ? '正在清除…' : '清除本机同步身份'),
            ),
          ),
        ],
      ),
    );
  }

  /// 二次确认：先列清代价，再要求勾选承认「Provider API Key 会失效」。
  ///
  /// 不写内部术语（vault/authority/roster）：用户要能自己判断这一步的后果。
  Future<void> _confirmClearLocalSyncIdentity(
    ThemeData theme,
    SyncSetupController controller,
  ) async {
    var acknowledged = false;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => StatefulBuilder(
        builder: (dialog, setInner) => AlertDialog(
          title: const Text('清除本机同步身份？'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('清除后：'),
              const SizedBox(height: 6),
              const Text('· 本机退出当前数据集，需要重新配对或重新加入才能再同步；'),
              const Text('· 已保存的 Provider API Key 会失效，需要重新填写；'),
              const Text('· 还没同步到其它设备的改动不会自动补发。'),
              const SizedBox(height: 6),
              const Text('笔记、对话、知识库不会被删除。'),
              const SizedBox(height: 8),
              CheckboxListTile(
                contentPadding: EdgeInsets.zero,
                value: acknowledged,
                onChanged: (value) =>
                    setInner(() => acknowledged = value ?? false),
                title: const Text('我已知晓，Provider API Key 需要重新填写'),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(dialog).pop(false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: acknowledged
                  ? () => Navigator.of(dialog).pop(true)
                  : null,
              child: const Text('清除并重新配对'),
            ),
          ],
        ),
      ),
    );
    if (confirmed != true || !mounted) return;
    final repair = widget.repairApi ?? const FrbSyncRepairApi();
    setState(() => _repairBusy = true);
    try {
      await repair.clearLocalSyncIdentity();
      if (!mounted) return;
      await controller.refresh();
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(content: Text('清除失败：$error')),
      );
    } finally {
      if (mounted) setState(() => _repairBusy = false);
    }
  }

  /// 设备名称 + 准备本机（同一行）：命名是绑定前的必要决定，放在状态卡
  /// 之前，用户不用先在长页面里找。文本框走 [AppTheme.fieldDecoration]
  /// （全应用表单唯一尺子：填充 + 12 圆角 + surface3 细描边 + 聚焦强调色），
  /// 字段始终有值，所以用前缀图标给语境，不用浮动 label——与「保存到
  /// 知识库」弹窗的标题/标签字段同一副长相。
  Widget _buildPrepareRow(SyncSetupController controller) => Row(
    crossAxisAlignment: CrossAxisAlignment.center,
    children: [
      Expanded(
        child: TextField(
          controller: _name,
          enabled: !controller.busy,
          maxLength: 64,
          style: AppTheme.fieldTextStyle,
          cursorColor: AppTheme.accentPrimary,
          // 计数器会把按钮基线拖歪，且与全应用其他表单不一致。
          decoration: AppTheme.fieldDecoration(
            hintText: '设备名称',
            prefixIcon: Icon(
              Icons.badge_outlined,
              size: 16,
              color: AppTheme.textTertiary,
            ),
          ).copyWith(counterText: ''),
        ),
      ),
      const SizedBox(width: 8),
      FilledButton.icon(
        icon: const Icon(Icons.key_outlined),
        label: const Text('准备本机'),
        onPressed: controller.busy
            ? null
            : () => _confirmPrepareOwner(controller),
      ),
    ],
  );

  /// 准备本机前的明确确认：“共享 Provider 配置和 API Key”不适合用一个
  /// 常驻勾选框带过——改成点击后弹窗，用户读过再选。
  Future<void> _confirmPrepareOwner(SyncSetupController controller) async {
    final agreed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('准备本机加入同步？'),
        content: const Text(
          '我理解：后续授权设备加入时，会共享所有 Provider 配置和 API Key。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('我理解并准备'),
          ),
        ],
      ),
    );
    if (agreed != true || !mounted) return;
    await controller.createOwner(
      _name.text,
      Platform.operatingSystem,
      consent: true,
    );
  }

  /// 绑定 tab——一次性流程：准备本机、邀请新设备、本机加入既有数据集。
  /// 与「管理」是两个独立关切（用户反馈：挤在一列里找不到绑定流程的重点）。
  Widget _buildBindContent(
    ThemeData theme,
    SyncSetupController controller,
    api.LocalSyncStateDto? state,
    bool configured,
  ) {
    final ready = state?.state == 'local_ready';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (!configured && state != null && !controller.previewEnabled) ...[
          const SizedBox(height: 12),
          Text(
            '设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。',
            style: theme.textTheme.bodySmall,
          ),
        ],
        const SizedBox(height: 16),
        if (ready && state?.isOwner == true && controller.previewEnabled)
          OwnerSyncPairingSettings(
            controller: controller,
            deliveryStatuses: widget.repository?.syncDeliveryStatus,
          ),
        if (controller.previewEnabled)
          JoinerSyncPairingSettings(
            controller: controller.joiner,
            deliveryStatuses: widget.repository?.syncDeliveryStatus,
          ),
      ],
    );
  }

  /// 管理 tab——日常运维：设备列表、加工权移交、密钥健康、冲突、锁定。
  Widget _buildManageContent(
    ThemeData theme,
    SyncSetupController controller,
    api.LocalSyncStateDto? state,
    bool configured,
    String? installation,
  ) {
    // 未完成绑定时管理面无事可做——在这里提前返回，恢复 state 的非空提升。
    if (!configured || state == null) {
      return Text('完成绑定后，这里会显示设备列表与同步管理。', style: theme.textTheme.bodySmall);
    }
    final ready = state.state == 'local_ready';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (configured) ...[
          const SizedBox(height: 16),
          if (ready)
            _LocalDeviceInfoCard(
              state: state,
              roster: _roster.roster,
              onRename: (newName) async {
                final deviceId = state.deviceId;
                if (deviceId == null) return '本机设备 ID 不可见，请刷新。';
                return controller.renameLocalDevice(deviceId, newName);
              },
            ),
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
          if (ready) SyncObjectConflictsSettings(controller: _objectConflicts),
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
      ],
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

/// 本机信息卡：设备名（owner 可改）、本机设备 ID、角色。中继 URL 不放
/// 这里——它是传输配置，不属于「本机是谁」。
