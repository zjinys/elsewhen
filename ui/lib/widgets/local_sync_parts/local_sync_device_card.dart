part of '../local_sync_settings.dart';

//! 本机设备信息卡 + relay 行（bind tab 渲染细节）

class _LocalDeviceInfoCard extends StatelessWidget {
  const _LocalDeviceInfoCard({
    required this.state,
    required this.roster,
    required this.onRename,
  });

  final api.LocalSyncStateDto state;
  final api.SyncRosterDto? roster;

  /// 改名后返回可读错误（null = 成功）；仅 owner 本机提供入口。
  final Future<String?> Function(String newName) onRename;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final roster = this.roster;
    final deviceId = roster?.localDeviceId;
    final roles = <String>[
      if (state.isOwner) '主设备',
      if (state.isProcessor) '负责 AI 分析',
    ];
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest.withValues(alpha: 0.4),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text('本机信息', style: theme.textTheme.titleSmall),
              const Spacer(),
              if (state.isOwner)
                TextButton.icon(
                  onPressed: () => _renameDialog(context),
                  icon: const Icon(Icons.edit_outlined, size: 16),
                  label: const Text('重命名'),
                ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '设备名：${state.deviceName ?? '未命名设备'}',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 2),
          Text(
            '角色：${roles.isEmpty ? '成员' : roles.join(' · ')}',
            style: theme.textTheme.bodyMedium,
          ),
          if (deviceId != null) ...[
            const SizedBox(height: 2),
            Row(
              children: [
                Text('本机 ID：', style: theme.textTheme.bodySmall),
                Expanded(
                  child: SelectableText(
                    deviceId,
                    style: theme.textTheme.bodySmall,
                    maxLines: 1,
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  /// 改名弹窗：预填当前名；空名不提交。
  Future<void> _renameDialog(BuildContext context) async {
    final input = TextEditingController(text: state.deviceName ?? '');
    final newName = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('重命名本机设备'),
        content: TextField(
          controller: input,
          autofocus: true,
          maxLength: 256,
          decoration: const InputDecoration(labelText: '设备名'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(input.text.trim()),
            child: const Text('保存'),
          ),
        ],
      ),
    );
    if (newName == null || newName.isEmpty || !context.mounted) return;
    final error = await onRename(newName);
    if (!context.mounted) return;
    if (error != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(error)));
    }
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
      // 「未连接」曾是误导（用户读作「连了但断了」）：值为空的真实含义
      // 是还没有已配对成员经中继交换过路由信息，不是链路故障。
      return Text(
        locked ? '中继服务：解锁本机密钥后可见' : '尚未配置中继（首次邀请成员时填写）',
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
