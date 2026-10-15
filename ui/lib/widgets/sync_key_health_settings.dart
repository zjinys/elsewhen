import 'package:flutter/material.dart';

import '../sync/sync_key_store.dart';

/// 密钥存储健康探针接口（可注入测试替身）。
///
/// 生产实现包装 [SyncKeyStore.probe]；探针只读，不改任何记录。
abstract interface class SyncKeyHealthApi {
  Future<SyncKeyStoreHealth> probe(String installationId);
}

class FrbSyncKeyHealthApi implements SyncKeyHealthApi {
  const FrbSyncKeyHealthApi(this._keys);

  final SyncKeyStore _keys;

  @override
  Future<SyncKeyStoreHealth> probe(String installationId) =>
      _keys.probe(installationId);
}

/// 设置页的密钥存储健康状态行（P4e 诊断项）。
///
/// 只展示状态与可读文案：健康时说明托管边界（OS 安全存储 + 本机限定），
/// 异常时给出处置指引。探针失败本身不会破坏已解锁会话。
class SyncKeyHealthSettings extends StatefulWidget {
  const SyncKeyHealthSettings({
    super.key,
    required this.api,
    required this.installationId,
  });

  final SyncKeyHealthApi api;
  final String installationId;

  @override
  State<SyncKeyHealthSettings> createState() => _SyncKeyHealthSettingsState();
}

class _SyncKeyHealthSettingsState extends State<SyncKeyHealthSettings> {
  SyncKeyStoreHealth? _health;
  bool _probing = false;

  @override
  void initState() {
    super.initState();
    // 首帧后再探针：探针会走安全存储/原生通道，不能在 build 事务里起步。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _probe();
    });
  }

  Future<void> _probe() async {
    setState(() => _probing = true);
    try {
      final health = await widget.api.probe(widget.installationId);
      if (mounted) setState(() => _health = health);
    } catch (_) {
      // 探针失败按不可用展示；诊断信息不外泄。
      if (mounted) setState(() => _health = SyncKeyStoreHealth.unavailable);
    } finally {
      if (mounted) setState(() => _probing = false);
    }
  }

  (IconData, Color, String) _presentation(ThemeData theme) => switch (_health) {
    SyncKeyStoreHealth.ok => (
      Icons.verified_user_outlined,
      theme.colorScheme.primary,
      '本机密钥由系统安全存储托管（仅限本设备，不进云备份/云同步）。',
    ),
    SyncKeyStoreHealth.missing => (
      Icons.key_off_outlined,
      theme.colorScheme.error,
      '缺少本机密钥记录。请通过原设备恢复；不会生成替代密钥。',
    ),
    SyncKeyStoreHealth.corrupt => (
      Icons.gpp_bad_outlined,
      theme.colorScheme.error,
      '本机密钥记录损坏，已停止解锁且不会覆盖原记录。',
    ),
    SyncKeyStoreHealth.unavailable => (
      Icons.lock_clock_outlined,
      theme.colorScheme.error,
      '系统安全存储不可用或已锁定，请解锁系统密钥库后刷新。',
    ),
    null => (
      Icons.hourglass_empty_outlined,
      theme.colorScheme.outline,
      '正在检查本机密钥存储…',
    ),
  };

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (icon, color, text) = _presentation(theme);
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 16, color: color),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              text,
              style: theme.textTheme.bodySmall?.copyWith(color: color),
            ),
          ),
          TextButton(
            onPressed: _probing ? null : _probe,
            child: const Text('检查'),
          ),
        ],
      ),
    );
  }
}
