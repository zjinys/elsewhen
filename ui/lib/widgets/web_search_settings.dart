import 'app_error_notice.dart';
import '../l10n/notice_messages.dart';
import '../l10n/localization.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../theme/app_theme.dart';

/// 「外网搜索」设置区块：开启开关、搜索引擎选择、API key、API 地址。
///
/// 未启用 / 未填 API key → web_search 工具被调用时如实告知「未配置」。DoH 网络传输
/// 已拆到独立的「网络（DoH）」卡片（`DohSettings`），本区块只管搜索引擎本身。
///
/// 这块是自包含的 StatefulWidget，自己读写配置，不污染 settings_screen 的状态。
class WebSearchSettings extends StatefulWidget {
  const WebSearchSettings({super.key});

  @override
  State<WebSearchSettings> createState() => _WebSearchSettingsState();
}

class _WebSearchSettingsState extends State<WebSearchSettings> {
  final _apiKeyController = TextEditingController();

  bool _loading = true;
  String? _loadError;

  bool _enabled = false;
  bool _hasApiKey = false;
  String _engine = 'tavily';
  String _baseUrl = 'https://api.tavily.com/search';
  int _maxResults = 5;

  /// 引擎选项：(存储标识, 显示名, 一句话说明)。
  List<(String, String, String)> get _engines => <(String, String, String)>[
    ('tavily', 'Tavily', context.l10n.l6HostedSearchOptimizedForAiWith46b67),
    (
      'brave',
      'Brave Search',
      context.l10n.l6HostedSearchWithAnIndependentPrivacy51ea0,
    ),
    ('searxng', 'SearxNG', context.l10n.l6SelfHostedMetasearchNoApiKey0926f),
  ];

  /// 各引擎默认 base_url（切换引擎且当前为默认/空时自动填入）。
  static const _engineDefaults = <String, String>{
    'tavily': 'https://api.tavily.com/search',
    'brave': 'https://api.search.brave.com/res/v1/web/search',
    'searxng': '',
  };

  bool get _needsKey => _engine == 'tavily' || _engine == 'brave';

  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _apiKeyController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final search = await api.getWebSearchConfig();
      if (!mounted) return;
      setState(() {
        _enabled = search.enabled;
        _hasApiKey = search.hasApiKey;
        _engine = search.engine;
        _baseUrl = search.baseUrl;
        _maxResults = search.maxResults.toInt();
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
      await api.saveWebSearchConfig(
        enabled: _enabled,
        engine: _engine,
        baseUrl: _baseUrl,
        // 空串 = 不改动已存 key（后端约定）；只有输入了新值才覆盖。
        apiKey: _apiKeyController.text,
        // PlatformInt64 在 IO 平台就是 int 的 typedef，直接传 int。
        maxResults: _maxResults,
      );
      // 保存后 key 一定已配置（除非从没填过），重新拉一次拿到准确的 hasApiKey。
      final search = await api.getWebSearchConfig();
      if (!mounted) return;
      setState(() {
        _hasApiKey = search.hasApiKey;
        _apiKeyController.clear();
      });
      _toast(context.l10n.l6WebSearchSettingsSaved30dee, ok: true);
    } catch (e) {
      if (mounted) showAppError(context, context.l10n.save, e);
    } finally {
      if (mounted) setState(() => _saving = false);
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
        _buildEnableSwitch(),
        const SizedBox(height: 20),
        _buildEngineSelector(),
        const SizedBox(height: 20),
        _buildBaseUrlField(),
        const SizedBox(height: 20),
        if (_needsKey) ...[_buildApiKeyField(), const SizedBox(height: 20)],
        const SizedBox(height: 24),
        _buildSaveButton(),
      ],
    );
  }

  Widget _buildEngineSelector() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.l6SearchEnginee5837,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        // 平铺可选列表（非下拉）：每个引擎一行，点击选中。
        Column(
          children: [
            for (var i = 0; i < _engines.length; i++) ...[
              if (i > 0) const SizedBox(height: 8),
              _buildEngineOption(_engines[i]),
            ],
          ],
        ),
      ],
    );
  }

  Widget _buildEngineOption((String, String, String) engine) {
    final (value, label, desc) = engine;
    final selected = _engine == value;
    return InkWell(
      borderRadius: BorderRadius.circular(12),
      onTap: () {
        setState(() {
          _engine = value;
          // 切引擎时，若 base_url 为空或还是某个引擎的默认值，自动换成新引擎默认。
          final isDefault =
              _baseUrl.trim().isEmpty ||
              _engineDefaults.values.contains(_baseUrl.trim());
          if (isDefault) {
            _baseUrl = _engineDefaults[value] ?? '';
          }
        });
      },
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        decoration: BoxDecoration(
          color: selected
              ? AppTheme.accentPrimary.withValues(alpha: 0.12)
              : AppTheme.surface2.withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: selected
                ? AppTheme.accentPrimary
                : AppTheme.accentPrimary.withValues(alpha: 0.2),
            width: selected ? 1.5 : 1,
          ),
        ),
        child: Row(
          children: [
            Icon(
              selected ? Icons.radio_button_checked : Icons.radio_button_off,
              size: 20,
              color: selected
                  ? AppTheme.accentPrimary
                  : AppTheme.textSecondary.withValues(alpha: 0.5),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    label,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                      color: selected
                          ? AppTheme.accentPrimary
                          : AppTheme.textPrimary,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    desc,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textSecondary.withValues(alpha: 0.7),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBaseUrlField() {
    final isSearxng = _engine == 'searxng';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          isSearxng
              ? context.l10n.l6SearxngInstanceUrla3267
              : context.l10n.l6ApiUrlacd8a,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        TextFormField(
          initialValue: _baseUrl,
          key: ValueKey('base_$_engine'),
          style: TextStyle(color: AppTheme.textPrimary),
          decoration: _inputDecoration(
            isSearxng
                ? context.l10n.l6HttpLocalhost8080OrYourSearxngba754
                : _engineDefaults[_engine] ?? '',
          ),
          onChanged: (v) => _baseUrl = v.trim(),
        ),
        if (isSearxng) ...[
          const SizedBox(height: 6),
          Text(
            context.l10n.l6SearxngIsSelfHostedMetasearchNoe6c57,
            style: TextStyle(
              fontSize: 12,
              color: AppTheme.textSecondary.withValues(alpha: 0.7),
              height: 1.5,
            ),
          ),
        ],
      ],
    );
  }

  Widget _buildEnableSwitch() {
    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                context.l10n.l6EnableWebSearch9ee9a,
                style: TextStyle(
                  fontSize: 14,
                  fontWeight: FontWeight.w500,
                  color: AppTheme.textPrimary,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                context.l10n.l6AllowAiToSearchTheWeb1ea6a,
                style: TextStyle(
                  fontSize: 12,
                  color: AppTheme.textSecondary.withValues(alpha: 0.7),
                ),
              ),
            ],
          ),
        ),
        Switch(
          value: _enabled,
          activeThumbColor: AppTheme.accentPrimary,
          onChanged: (v) => setState(() => _enabled = v),
        ),
      ],
    );
  }

  Widget _buildApiKeyField() {
    final hint = switch (_engine) {
      'tavily' => context.l10n.l6TvlyFreeAllowanceAtTavilyCom279ec,
      'brave' => context.l10n.l6BsaApplyAtBraveComSearch6c59c,
      _ => 'API key',
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '${_engine == 'tavily' ? 'Tavily' : 'Brave'} API Key',
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        TextFormField(
          controller: _apiKeyController,
          obscureText: true,
          style: TextStyle(color: AppTheme.textPrimary),
          decoration: _inputDecoration(
            _hasApiKey
                ? context.l10n.l6ConfiguredEnterAValueToReplace1d9d7
                : hint,
          ),
        ),
      ],
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
