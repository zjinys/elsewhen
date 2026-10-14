import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_roster_controller.dart';

/// 设置页的已授权设备列表 + owner 撤销入口（P4e 撤销 UX）。
///
/// 撤销只针对活跃的非 owner、非加工、非本机成员，且仅 owner 本机可见；
/// 加工设备必须先移交（Rust 侧 `sync_membership_handoff_required` 硬拦）。
/// 撤销绑定审阅时所见的名册修订号，由 Rust 侧失败关闭。
class SyncMembersSettings extends StatefulWidget {
  const SyncMembersSettings({super.key, required this.controller});

  final SyncRosterController controller;

  @override
  State<SyncMembersSettings> createState() => _SyncMembersSettingsState();
}

class _SyncMembersSettingsState extends State<SyncMembersSettings> {
  bool _requested = false;

  @override
  Widget build(BuildContext context) {
    if (!_requested) {
      _requested = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        widget.controller.refresh();
      });
    }
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        final roster = controller.roster;
        final localIsOwner =
            roster != null &&
            roster.localDeviceId != null &&
            roster.localDeviceId == roster.ownerDeviceId;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 16),
            Text('已授权设备', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Text(
              '列表来自 owner 签名的名册；撤销后设备立即失去再同步资格。'
              '撤销不会回收该设备已持有的 Provider API Key。',
              style: theme.textTheme.bodySmall,
            ),
            if (controller.error != null) ...[
              const SizedBox(height: 8),
              Semantics(
                liveRegion: true,
                child: Text(
                  controller.error!,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ),
            ],
            const SizedBox(height: 8),
            if (roster == null && !controller.busy)
              TextButton(
                onPressed: controller.refresh,
                child: const Text('刷新设备列表'),
              )
            else if (roster == null)
              const Text('正在读取设备列表…')
            else
              ...roster.members.map(
                (member) => _MemberRow(
                  key: ValueKey(member.deviceId),
                  member: member,
                  revocable: memberRevocable(roster, member),
                  needsHandoffOffFirst:
                      localIsOwner &&
                      member.active &&
                      member.isProcessor &&
                      !member.isLocal,
                  busy: controller.busy,
                  onRevoke: () => _revoke(context, controller, member),
                ),
              ),
          ],
        );
      },
    );
  }

  Future<void> _revoke(
    BuildContext context,
    SyncRosterController controller,
    api.SyncRosterMemberDto member,
  ) async {
    controller.beginRevoke(member);
    final confirmed = await showRevokeConfirmDialog(context, member);
    if (!context.mounted) return;
    if (confirmed) {
      await controller.confirmRevoke();
    } else {
      controller.cancelRevoke();
    }
  }
}

/// 撤销资格：owner 本机 + 活跃 + 非 owner + 非加工 + 非本机。
bool memberRevocable(api.SyncRosterDto roster, api.SyncRosterMemberDto member) {
  final localIsOwner =
      roster.localDeviceId != null &&
      roster.localDeviceId == roster.ownerDeviceId;
  return localIsOwner &&
      member.active &&
      !member.isOwner &&
      !member.isProcessor &&
      !member.isLocal;
}

class _MemberRow extends StatelessWidget {
  const _MemberRow({
    super.key,
    required this.member,
    required this.revocable,
    required this.needsHandoffOffFirst,
    required this.busy,
    required this.onRevoke,
  });

  final api.SyncRosterMemberDto member;
  final bool revocable;
  final bool needsHandoffOffFirst;
  final bool busy;
  final Future<void> Function() onRevoke;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final roles = <String>[
      if (member.isOwner) 'owner',
      if (member.isProcessor) '加工',
      if (member.isLocal) '本机',
    ];
    final title = member.active
        ? (roles.isEmpty ? member.name : '${member.name}（${roles.join(' · ')}）')
        : '${member.name}（已撤销）';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Icon(
            member.active ? Icons.devices_other : Icons.phonelink_erase,
            size: 18,
            color: member.active
                ? theme.colorScheme.primary
                : theme.colorScheme.outline,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: member.active ? null : theme.colorScheme.outline,
                  ),
                ),
                SelectableText(
                  member.deviceId,
                  style: theme.textTheme.bodySmall,
                ),
              ],
            ),
          ),
          if (revocable) ...[
            const SizedBox(width: 8),
            OutlinedButton.icon(
              icon: const Icon(Icons.person_remove_outlined, size: 16),
              label: const Text('撤销'),
              onPressed: busy ? null : onRevoke,
            ),
          ] else if (needsHandoffOffFirst)
            Padding(
              padding: const EdgeInsets.only(left: 8),
              child: Text('需先移交加工权', style: theme.textTheme.bodySmall),
            ),
        ],
      ),
    );
  }
}

/// 撤销确认对话框（文案披露丢失设备的 Provider Key 边界）。
Future<bool> showRevokeConfirmDialog(
  BuildContext context,
  api.SyncRosterMemberDto member,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: const Text('撤销设备同步权限？'),
      content: Text(
        '设备 "${member.name}" 将立即失去再同步资格。\n\n'
        '丢失设备须知：撤销同步权限不会回收该设备已持有的 Provider API Key。'
        '请到对应 Provider 控制台吊销或轮换密钥，剩余设备会接收新版本。',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(false),
          child: const Text('取消'),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            foregroundColor: Theme.of(dialogContext).colorScheme.onError,
            backgroundColor: Theme.of(dialogContext).colorScheme.error,
          ),
          onPressed: () => Navigator.of(dialogContext).pop(true),
          child: const Text('撤销'),
        ),
      ],
    ),
  );
  return result == true;
}
