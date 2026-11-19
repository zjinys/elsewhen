import 'app_error_notice.dart';
import '../l10n/localization.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';

/// 「Scout 情报服务」设置区块（提案 2026-11-15 §4）。
///
/// 连接独立采集服务 scout：服务地址（HTTPS）、API token（存命名密钥
/// scout_token，不回传明文）、自动拉取间隔（≥5 分钟）、启停；附
/// 测试连接与清空本地镜像。源启用/间隔等采集配置在服务端管理页编辑，
/// 这里只读展示由「数据源」页承担。
///
/// 自包含 ConsumerStatefulWidget，自己读写配置，不污染 settings_screen。
class ScoutSettings extends ConsumerStatefulWidget {
  const ScoutSettings({super.key});

  @override
  ConsumerState<ScoutSettings> createState() => _ScoutSettingsState();
}

class _ScoutSettingsState extends ConsumerState<ScoutSettings> {
  final _baseUrlController = TextEditingController();
  final _tokenController = TextEditingController();

  bool _loading = true;
  String? _loadError;
  bool _configured = false;

  bool _enabled = true;
  bool _hasToken = false;
  int _interval = 15;
  String? _lastPolledAt;

  bool _saving = false;
  bool _testing = false;
  bool _clearing = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _baseUrlController.dispose();
    _tokenController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final s = await api.getScoutSettings();
      if (!mounted) return;
      setState(() {
        if (s != null) {
          _configured = true;
          _baseUrlController.text = s.baseUrl;
          _enabled = s.enabled;
          _hasToken = s.hasToken;
          _interval = s.pollIntervalMinutes;
          _lastPolledAt = s.lastPolledAt;
        }
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loadError = e.toString();
        _loading = false;
      });
    }
  }

  Future<void> _save() async {
    setState(() => _saving = true);
    try {
      await api.saveScoutSettings(
        baseUrl: _baseUrlController.text.trim(),
        token: _tokenController.text.trim(),
        pollIntervalMinutes: _interval,
        enabled: _enabled,
      );
      if (!mounted) return;
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await bridge.refreshScoutCounters();
      if (!mounted) return;
      _toast(context.l10n.scoutSettingsSaved);
      await _load();
    } catch (e) {
      if (mounted) showAppError(context, context.l10n.save, e);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _test() async {
    // 先保存再测试：保证测的是填进去的地址。
    await _save();
    setState(() => _testing = true);
    try {
      final result = await api.testScoutConnection();
      if (!mounted) return;
      _toast(result);
    } catch (e) {
      if (mounted) showAppError(context, context.l10n.scoutSettingsTest, e);
    } finally {
      if (mounted) setState(() => _testing = false);
    }
  }

  Future<void> _clearMirror() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.scoutSettingsClear),
        content: Text(context.l10n.scoutSettingsDesc),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l10n.confirm),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    setState(() => _clearing = true);
    try {
      await api.clearScoutMirror();
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await bridge.refreshScoutCounters();
      if (!mounted) return;
      _toast(context.l10n.scoutSettingsSaved);
    } catch (e) {
      if (mounted) showAppError(context, context.l10n.scoutSettingsClear, e);
    } finally {
      if (mounted) setState(() => _clearing = false);
    }
  }

  void _toast(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(message),
        behavior: SnackBarBehavior.floating,
        shape: const RoundedRectangleBorder(
          borderRadius: BorderRadius.all(Radius.circular(999)),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Padding(
        padding: EdgeInsets.all(16),
        child: Center(child: CircularProgressIndicator()),
      );
    }
    if (_loadError != null) {
      return Padding(
        padding: const EdgeInsets.all(16),
        child: Text(
          context.l10n.scoutNotConfigured,
          style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(context.l10n.scoutSettingsEnabled),
          value: _enabled,
          onChanged: (v) {
            setState(() => _enabled = v);
            _save();
          },
        ),
        TextField(
          controller: _baseUrlController,
          decoration: InputDecoration(
            labelText: context.l10n.scoutSettingsBaseUrl,
            hintText: 'https://elsewhen-scout.example.workers.dev',
          ),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _tokenController,
          obscureText: true,
          decoration: InputDecoration(
            labelText: context.l10n.scoutSettingsToken,
            helperText: _hasToken ? context.l10n.scoutSettingsToken : null,
          ),
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            Text(context.l10n.scoutSettingsInterval),
            const SizedBox(width: 12),
            SizedBox(
              width: 90,
              child: DropdownButtonFormField<int>(
                initialValue: _interval,
                decoration: const InputDecoration(border: OutlineInputBorder()),
                items: const [5, 15, 30, 60]
                    .map((m) => DropdownMenuItem(value: m, child: Text('$m')))
                    .toList(),
                onChanged: (v) {
                  if (v != null) setState(() => _interval = v);
                },
              ),
            ),
          ],
        ),
        if (_lastPolledAt != null) ...[
          const SizedBox(height: 8),
          Text(
            context.l10n.scoutLastPolled(_lastPolledAt!),
            style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
          ),
        ],
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          children: [
            FilledButton(
              onPressed: _saving ? null : _save,
              child: Text(context.l10n.save),
            ),
            OutlinedButton(
              onPressed: (_testing || _saving) ? null : _test,
              child: Text(context.l10n.scoutSettingsTest),
            ),
            TextButton(
              onPressed: (_clearing || !_configured) ? null : _clearMirror,
              child: Text(context.l10n.scoutSettingsClear),
            ),
          ],
        ),
      ],
    );
  }
}
