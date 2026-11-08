import 'app_error_notice.dart';
import '../l10n/notice_messages.dart';
import '../l10n/localization.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../theme/app_theme.dart';

/// 「网络（DoH）」设置区块：DoH 启用策略三态 + 服务地址 + 测试连接。
///
/// 三态（与后端 migration 26 一致），把「启用」与「地址」解耦：
/// * off   强制系统直连，忽略已填地址；
/// * auto  填了地址就优先 DoH+ECH，连不上自动回退直连（默认，可达性与可用性兼顾）；
/// * force 只用 DoH+ECH，失败即报错、不悄悄降级（受限网络必须走隧道时用）。
///
/// DoH 仅自定义、无内置默认；作用域本版仅作用于外网搜索（search_only）。
class DohSettings extends StatefulWidget {
  const DohSettings({super.key});

  @override
  State<DohSettings> createState() => _DohSettingsState();
}

class _DohSettingsState extends State<DohSettings> {
  final _dohController = TextEditingController();

  bool _loading = true;
  String? _loadError;

  String _mode = 'auto'; // off | auto | force
  bool _saving = false;
  bool _testing = false;
  String? _testResult;
  bool _testOk = false;

  List<(String, String)> get _modes => <(String, String)>[
    ('off', context.l10n.l6OffDirectConnection371c7),
    ('auto', context.l10n.l6AutomaticPreferDohFallBackTo02ddb),
    ('force', context.l10n.l6RequiredDohOnly763c2),
  ];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _dohController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final network = await api.getNetworkSettings();
      if (!mounted) return;
      setState(() {
        _mode = network.dohMode;
        _dohController.text = network.dohUri;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _loadError = appFailure(e);
      });
    }
  }

  Future<void> _save() async {
    setState(() => _saving = true);
    try {
      await api.saveDohSettings(
        dohUri: _dohController.text.trim(),
        dohMode: _mode,
      );
      if (!mounted) return;
      _toast(context.l10n.l6DohSettingsSavedeaeea, ok: true);
    } catch (e) {
      if (mounted) showAppError(context, context.l10n.save, e);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _testDoh() async {
    final doh = _dohController.text.trim();
    if (doh.isEmpty) {
      setState(() {
        _testOk = false;
        _testResult = context.l10n.l6EnterADohServiceUrlBeforeff97f;
      });
      return;
    }
    setState(() {
      _testing = true;
      _testResult = null;
    });
    try {
      await api.testDohConnection(dohUri: doh);
      if (!mounted) return;
      setState(() {
        _testOk = true;
        _testResult = appNotice('connectionSucceeded');
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _testOk = false;
        _testResult = appFailure(e);
      });
    } finally {
      if (mounted) setState(() => _testing = false);
    }
  }

  void _toast(String message, {required bool ok}) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(message),
        backgroundColor: ok ? AppTheme.accentPrimary : AppTheme.error,
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
        padding: EdgeInsets.symmetric(vertical: 12),
        child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
      );
    }
    if (_loadError != null) {
      return AppErrorMessage(
        action: context.l10n.load,
        error: _loadError!,
        style: TextStyle(color: AppTheme.error, fontSize: 12),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildModeDropdown(),
        const SizedBox(height: 20),
        _buildAddressField(),
        const SizedBox(height: 8),
        _buildHint(),
        if (_testResult != null) ...[
          const SizedBox(height: 8),
          _buildTestResult(),
        ],
        const SizedBox(height: 24),
        _buildSaveButton(),
      ],
    );
  }

  Widget _buildModeDropdown() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.l6ActivationPolicyfc94d,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          decoration: BoxDecoration(
            color: AppTheme.surface2.withValues(alpha: 0.5),
            borderRadius: BorderRadius.circular(12),
            border: Border.all(
              color: AppTheme.accentPrimary.withValues(alpha: 0.2),
            ),
          ),
          child: DropdownButtonHideUnderline(
            child: DropdownButton<String>(
              value: _mode,
              isExpanded: true,
              dropdownColor: AppTheme.surface2,
              style: TextStyle(color: AppTheme.textPrimary, fontSize: 14),
              items: _modes
                  .map(
                    (m) => DropdownMenuItem<String>(
                      value: m.$1,
                      child: Text(m.$2),
                    ),
                  )
                  .toList(),
              onChanged: (v) {
                if (v != null) setState(() => _mode = v);
              },
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildAddressField() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.l6DohServiceUrlae8b4,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: TextFormField(
                controller: _dohController,
                style: TextStyle(color: AppTheme.textPrimary),
                decoration: _inputDecoration(
                  'https://your-doh.example.com/dns-query',
                ),
              ),
            ),
            const SizedBox(width: 12),
            _buildTestButton(),
          ],
        ),
      ],
    );
  }

  Widget _buildHint() {
    final suffix = switch (_mode) {
      'off' => context.l10n.l6OffTheUrlIsSavedButa536d,
      'force' => context.l10n.l6RequiredADohFailureReturnsAnaabc1,
      _ => context.l10n.l6AutomaticPreferDohAndEchFallingfc42f,
    };
    return Text(
      context.l10n.l6SupplyYourOwnEndpointNoneIsd855e((suffix).toString()),
      style: TextStyle(
        fontSize: 12,
        color: AppTheme.textSecondary.withValues(alpha: 0.7),
        height: 1.5,
      ),
    );
  }

  Widget _buildTestResult() {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(
          _testOk ? Icons.check_circle_outline : Icons.error_outline,
          size: 16,
          color: _testOk ? AppTheme.accentPrimary : AppTheme.error,
        ),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            resolveNotice(_testResult!, context.l10n) ?? _testResult!,
            style: TextStyle(
              fontSize: 12,
              color: _testOk ? AppTheme.accentPrimary : AppTheme.error,
              height: 1.4,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildTestButton() {
    return OutlinedButton.icon(
      onPressed: _testing ? null : _testDoh,
      icon: _testing
          ? const SizedBox(
              width: 14,
              height: 14,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : const Icon(Icons.network_check, size: 16),
      label: Text(
        _testing
            ? context.l10n.l6Testingcf7e9
            : context.l10n.l6TestConnection10b7d,
      ),
      style: OutlinedButton.styleFrom(
        foregroundColor: AppTheme.accentPrimary,
        side: BorderSide(color: AppTheme.accentPrimary.withValues(alpha: 0.4)),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
    );
  }

  Widget _buildSaveButton() {
    return Align(
      alignment: Alignment.centerRight,
      child: FilledButton.icon(
        onPressed: _saving ? null : _save,
        icon: _saving
            ? const SizedBox(
                width: 14,
                height: 14,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: Colors.white,
                ),
              )
            : const Icon(Icons.save_outlined, size: 16),
        label: Text(_saving ? context.l10n.l6Savinga032e : context.l10n.save),
        style: FilledButton.styleFrom(
          backgroundColor: AppTheme.accentPrimary,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
          ),
        ),
      ),
    );
  }

  InputDecoration _inputDecoration(String hint) {
    return InputDecoration(
      hintText: hint,
      hintStyle: TextStyle(
        color: AppTheme.textSecondary.withValues(alpha: 0.4),
      ),
      filled: true,
      fillColor: AppTheme.surface2.withValues(alpha: 0.5),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: BorderSide(
          color: AppTheme.accentPrimary.withValues(alpha: 0.2),
        ),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: BorderSide(
          color: AppTheme.accentPrimary.withValues(alpha: 0.2),
        ),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: BorderSide(color: AppTheme.accentPrimary),
      ),
    );
  }
}
