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

  static const _modes = <(String, String)>[
    ('off', '关闭（强制直连）'),
    ('auto', '自动（优先 DoH，失败回退直连）'),
    ('force', '强制（只用 DoH，不降级）'),
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
        _loadError = '$e';
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
      _toast('DoH 设置已保存', ok: true);
    } catch (e) {
      _toast('保存失败：${_firstLine(e)}', ok: false);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _testDoh() async {
    final doh = _dohController.text.trim();
    if (doh.isEmpty) {
      setState(() {
        _testOk = false;
        _testResult = '请先填写 DoH 服务地址再测试';
      });
      return;
    }
    setState(() {
      _testing = true;
      _testResult = null;
    });
    try {
      final result = await api.testDohConnection(dohUri: doh);
      if (!mounted) return;
      setState(() {
        _testOk = true;
        _testResult = result;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _testOk = false;
        _testResult = _firstLine(e);
      });
    } finally {
      if (mounted) setState(() => _testing = false);
    }
  }

  String _firstLine(Object e) {
    final s = '$e';
    return s
        .split('\n')
        .firstWhere((l) => l.trim().isNotEmpty, orElse: () => s.trim());
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
      return Text(
        'DoH 设置读取失败：${_firstLine(_loadError!)}',
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
          '启用策略',
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
          'DoH 服务地址',
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
      'off' => '当前为「关闭」：地址仅保存不生效，外网搜索走系统直连。',
      'force' => '当前为「强制」：DoH 失败将直接报错，不会回退直连。',
      _ => '当前为「自动」：优先 DoH+ECH，连不上自动回退直连。',
    };
    return Text(
      '仅自定义、无内置默认。填了地址后，外网搜索会经 DoH+ECH 传输，在国内网络下也能'
      '到达被 DNS 污染 / SNI 封锁的服务（如 x.com）。当前仅作用于外网搜索。$suffix',
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
            _testResult!,
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
      label: Text(_testing ? '测试中' : '测试连接'),
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
        label: Text(_saving ? '保存中' : '保存'),
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
