import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../theme/app_theme.dart';

/// 「网络」设置里的「出站代理」区块——标准单选列表（Chrome 代理设置同构）。
///
/// 三态（与后端 migration 42 一致）：
/// * 自动     跟随 HTTPS_PROXY / ALL_PROXY 环境变量，未设置则直连（默认）；
/// * 手动     用下方地址框（覆盖环境变量），http:// 或 socks5://；
/// * 不使用   强制直连，环境变量代理也被屏蔽。
///
/// 附带语义：手动代理开启期间 DoH 隧道让位（挂了代理就全走代理）。
/// 回环地址（127.0.0.1、localhost）永远直连，不会被送进代理。
class ProxySettings extends StatefulWidget {
  const ProxySettings({super.key});

  @override
  State<ProxySettings> createState() => _ProxySettingsState();
}

class _ProxySettingsState extends State<ProxySettings> {
  final _urlController = TextEditingController();

  bool _loading = true;
  String? _loadError;

  String _mode = 'auto'; // auto | custom | off
  bool _saving = false;
  bool _testing = false;
  String? _testResult;
  bool _testOk = false;

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
      _toast('代理设置已保存，出站请求即时生效', ok: true);
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
        _buildModeRadio('auto', '自动', '跟随时下环境：HTTPS_PROXY / ALL_PROXY，未设置则直连'),
        _buildModeRadio('custom', '手动配置代理', '以下方地址为准，覆盖环境变量；挂代理期间 DoH 隧道自动让位'),
        if (_mode == 'custom') _buildAddressField(),
        _buildModeRadio('off', '不使用代理', '强制直连，环境变量中的代理也被忽略'),
        if (_testResult != null) ...[
          const SizedBox(height: 8),
          _buildTestResult(),
        ],
        const SizedBox(height: 20),
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

  Widget _buildModeRadio(String value, String title, String subtitle) {
    final selected = _mode == value;
    return InkWell(
      borderRadius: BorderRadius.circular(10),
      onTap: () => setState(() => _mode = value),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 36,
              height: 40,
              child: Radio<String>(
                value: value,
                groupValue: _mode,
                onChanged: (v) {
                  if (v != null) setState(() => _mode = v);
                },
              ),
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: TextStyle(
                        fontSize: 13.5,
                        fontWeight: FontWeight.w500,
                        color: selected
                            ? AppTheme.textPrimary
                            : AppTheme.textSecondary,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      subtitle,
                      style: TextStyle(
                        fontSize: 11.5,
                        color: AppTheme.textTertiary,
                        height: 1.4,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildAddressField() {
    return Padding(
      // 缩进与 radio 内容对齐（radio 圆点右侧），Chrome「安全 DNS」的内联样式。
      padding: const EdgeInsets.only(left: 40, top: 2, bottom: 4),
      child: TextField(
        controller: _urlController,
        style: TextStyle(color: AppTheme.textPrimary, fontSize: 13),
        decoration: InputDecoration(
          hintText: 'http://127.0.0.1:7890 或 socks5://…',
          hintStyle: TextStyle(color: AppTheme.textTertiary, fontSize: 12.5),
          filled: true,
          fillColor: AppTheme.surface2.withValues(alpha: 0.5),
          contentPadding: const EdgeInsets.symmetric(
            horizontal: 14,
            vertical: 12,
          ),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(10),
            borderSide: BorderSide(
              color: AppTheme.accentPrimary.withValues(alpha: 0.2),
            ),
          ),
          enabledBorder: OutlineInputBorder(
            borderRadius: BorderRadius.circular(10),
            borderSide: BorderSide(
              color: AppTheme.accentPrimary.withValues(alpha: 0.2),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildTestResult() {
    return Padding(
      padding: const EdgeInsets.only(left: 4),
      child: Text(
        _testResult!,
        style: TextStyle(
          fontSize: 12,
          color: _testOk ? AppTheme.success : AppTheme.error,
        ),
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
              child: CircularProgressIndicator(
                strokeWidth: 2,
                color: Colors.white,
              ),
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
          : const Text('测试连接', style: TextStyle(fontSize: 13)),
    );
  }
}
