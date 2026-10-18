import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../theme/app_theme.dart';

/// 「网络」设置里的「代理」区块：出站代理三态 + 地址 + 连通性测试。
///
/// 三态（与后端 migration 42 一致），把「用不用代理」与「用哪个」解耦：
/// * off    强制直连——环境变量里的代理也会被屏蔽（用户显式关）；
/// * auto   跟随 HTTPS_PROXY/ALL_PROXY 环境变量，没有就直连（默认，兼容既有行为）；
/// * custom 用这里填的地址（覆盖环境变量），http:// 或 socks5://。
///
/// 回环地址（本机 Ollama、127.0.0.1）永远直连，不会被送进代理。
class ProxySettings extends StatefulWidget {
  const ProxySettings({super.key});

  @override
  State<ProxySettings> createState() => _ProxySettingsState();
}

class _ProxySettingsState extends State<ProxySettings> {
  final _urlController = TextEditingController();

  bool _loading = true;
  String? _loadError;

  String _mode = 'auto'; // off | auto | custom
  bool _saving = false;
  bool _testing = false;
  String? _testResult;
  bool _testOk = false;

  static const _modes = <(String, String)>[
    ('auto', '跟随环境（HTTPS_PROXY / ALL_PROXY）'),
    ('custom', '自定义代理地址'),
    ('off', '强制直连（忽略环境变量）'),
  ];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _urlController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final network = await api.getNetworkSettings();
      if (!mounted) return;
      setState(() {
        _mode = network.proxyMode;
        _urlController.text = network.proxyUrl;
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
      await api.saveNetworkProxy(
        proxyMode: _mode,
        proxyUrl: _urlController.text.trim(),
      );
      _toast('代理设置已保存，AI 请求即时生效', ok: true);
    } catch (e) {
      _toast('保存失败：${_firstLine(e)}', ok: false);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _test() async {
    final url = _mode == 'custom' ? _urlController.text.trim() : '';
    if (_mode == 'custom' && url.isEmpty) {
      setState(() {
        _testOk = false;
        _testResult = '请先填写代理地址再测试';
      });
      return;
    }
    setState(() {
      _testing = true;
      _testResult = null;
    });
    try {
      final result = await api.testProxyConnection(proxyUrl: url);
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
        '代理设置读取失败：${_firstLine(_loadError!)}',
        style: TextStyle(color: AppTheme.error, fontSize: 12),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildModeDropdown(),
        const SizedBox(height: 20),
        if (_mode == 'custom') ...[
          _buildAddressField(),
          const SizedBox(height: 8),
        ],
        _buildHint(),
        if (_testResult != null) ...[
          const SizedBox(height: 8),
          _buildTestResult(),
        ],
        const SizedBox(height: 24),
        Row(
          children: [
            _buildSaveButton(),
            const SizedBox(width: 12),
            _buildTestButton(),
          ],
        ),
      ],
    );
  }

  Widget _buildModeDropdown() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '出站策略',
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
          '代理地址',
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        TextField(
          controller: _urlController,
          style: TextStyle(color: AppTheme.textPrimary, fontSize: 13),
          decoration: InputDecoration(
            hintText: 'http://127.0.0.1:7890 或 socks5://…',
            hintStyle: TextStyle(color: AppTheme.textTertiary, fontSize: 13),
            filled: true,
            fillColor: AppTheme.surface2.withValues(alpha: 0.5),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 16,
              vertical: 13,
            ),
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
          ),
        ),
      ],
    );
  }

  Widget _buildHint() {
    final hint = switch (_mode) {
      'off' => '所有出站请求强制直连，环境变量中的代理不再生效。',
      'custom' => 'AI 请求、外网搜索、wiki 外链与同步 relay 都走此代理；本机（127.0.0.1）地址例外。',
      _ => '与启动脚本探测的 clash 端口相同来源：HTTPS_PROXY / ALL_PROXY，未设置则直连。',
    };
    return Text(
      hint,
      style: TextStyle(color: AppTheme.textTertiary, fontSize: 11.5, height: 1.45),
    );
  }

  Widget _buildTestResult() {
    return Text(
      _testResult!,
      style: TextStyle(
        fontSize: 12,
        color: _testOk ? AppTheme.success : AppTheme.error,
      ),
    );
  }

  Widget _buildSaveButton() {
    return ElevatedButton(
      onPressed: _saving ? null : _save,
      style: ElevatedButton.styleFrom(
        backgroundColor: AppTheme.accentPrimary,
        foregroundColor: Colors.white,
        padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
      child: _saving
          ? const SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2, color: Colors.white),
            )
          : const Text('保存', style: TextStyle(fontSize: 13)),
    );
  }

  Widget _buildTestButton() {
    return OutlinedButton(
      onPressed: _testing ? null : _test,
      style: OutlinedButton.styleFrom(
        foregroundColor: AppTheme.textSecondary,
        side: BorderSide(color: AppTheme.accentPrimary.withValues(alpha: 0.3)),
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 12),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
      child: _testing
          ? const SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : const Text('测试连通', style: TextStyle(fontSize: 13)),
    );
  }
}
