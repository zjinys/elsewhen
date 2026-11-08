part of '../settings_screen.dart';

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! Provider 编辑器对话框、用量柱状图与草稿结构（纯类平移）。

class _ProviderDraft {
  final String name;
  final String providerType;
  final String baseUrl;
  final String model;
  final String apiKey;
  final double temperature;
  final int? maxTokens;
  final int? contextWindow;

  _ProviderDraft({
    required this.name,
    required this.providerType,
    required this.baseUrl,
    required this.model,
    required this.apiKey,
    required this.temperature,
    required this.maxTokens,
    required this.contextWindow,
  });
}

/// AI Provider 编辑对话框。
///
/// 控制器必须归本 State 所有，而不是在 `_openProviderEditor` 里建局部变量
/// 再在 `await showDialog` 之后 dispose：`showDialog` 在 Navigator.pop 那一刻
/// 就返回，而对话框的退出动画还要再跑约 200ms，其间 widget 树仍然挂载。
/// ESC 关闭时会同时发生失焦 + 键盘收起 + pop，重建概率最高，动画帧里的
/// `_AnimatedState.didUpdateWidget` 会重新给已释放的 controller `addListener`，
/// 抛 "A TextEditingController was used after being disposed"。
/// 由 State 在真正卸载时释放，生命周期与使用方天然对齐。
class _ProviderEditorDialog extends StatefulWidget {
  const _ProviderEditorDialog({
    required this.isNew,
    required this.onSave,
    this.isDuplicate = false,
    required this.initialName,
    required this.initialBaseUrl,
    required this.initialModel,
    required this.initialTemperature,
    required this.initialContextWindow,
    required this.initialMaxTokens,
    required this.initialProviderType,
    required this.initialHasApiKey,
    this.initialSecretName,
  });

  final String? initialSecretName;
  final bool isNew;
  final bool isDuplicate;
  final String initialName;

  /// 该配置当前是否已有 key（密文丢失 / 未重填时为 false）。
  /// false 时 API Key 字段标警告色，提示必须重填才能恢复可用。
  final bool initialHasApiKey;

  /// 保存动作由对话框自己执行并由原生校验：失败时错误留在本框内，
  /// 输入不丢；成功时才 pop。此前保存发生在 pop 之后，校验失败
  /// 会让半填的表单整个消失。
  final Future<void> Function(_ProviderDraft draft) onSave;

  final String initialBaseUrl;
  final String initialModel;
  final double initialTemperature;
  final String initialContextWindow;
  final String initialMaxTokens;
  final String initialProviderType;

  @override
  State<_ProviderEditorDialog> createState() => _ProviderEditorDialogState();
}

class _ProviderEditorDialogState extends State<_ProviderEditorDialog> {
  late final TextEditingController _name;
  late final TextEditingController _baseUrl;
  late final TextEditingController _model;
  // 密钥不回填明文：留空即保留原 key（新建则必填）
  late final TextEditingController _apiKey;
  late final TextEditingController _temperature;
  late final TextEditingController _contextWindow;
  late final TextEditingController _maxTokens;
  late String _providerType;
  bool _saving = false;
  String? _error;
  bool _useSecret = false;
  String? _selectedSecret;
  String? _selectedSecretTitle;

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: widget.initialName);
    _baseUrl = TextEditingController(text: widget.initialBaseUrl);
    _model = TextEditingController(text: widget.initialModel);
    _selectedSecret = widget.initialSecretName;
    _useSecret = _selectedSecret != null;
    _apiKey = TextEditingController(text: _selectedSecret ?? '');
    _temperature = TextEditingController(
      text: widget.initialTemperature.toString(),
    );
    _contextWindow = TextEditingController(text: widget.initialContextWindow);
    _maxTokens = TextEditingController(text: widget.initialMaxTokens);
    _providerType = widget.initialProviderType;
    if (_useSecret) _readSecretTitle();
  }

  Future<void> _readSecretTitle() async {
    try {
      final secrets = await api.listNamedSecrets();
      if (!mounted) return;
      for (final secret in secrets) {
        if (secret.name == _selectedSecret) {
          setState(() {
            _selectedSecretTitle = secret.title;
            _apiKey.text = secret.title.isEmpty ? secret.name : secret.title;
          });
          break;
        }
      }
    } catch (_) {
      /* The selected reference stays visible; choosing again can retry. */
    }
  }

  Future<void> _chooseSecret() async {
    try {
      final secrets = await api.listNamedSecrets();
      if (!mounted) return;
      final selected = await showDialog<String>(
        context: context,
        builder: (context) => SimpleDialog(
          title: Text(context.l10n.selectSecret),
          children: secrets.isEmpty
              ? [
                  Padding(
                    padding: EdgeInsets.all(24),
                    child: Text(context.l10n.noSecretsHint),
                  ),
                ]
              : secrets
                    .map(
                      (s) => SimpleDialogOption(
                        onPressed: () => Navigator.pop(context, s.name),
                        child: ListTile(
                          contentPadding: EdgeInsets.zero,
                          title: Text(s.title.isEmpty ? s.name : s.title),
                          subtitle: Text(s.name),
                          trailing: s.name == _selectedSecret
                              ? const Icon(Icons.check, size: 18)
                              : null,
                        ),
                      ),
                    )
                    .toList(),
        ),
      );
      if (selected != null && mounted) {
        setState(() {
          _selectedSecret = selected;
          _useSecret = true;
          _selectedSecretTitle = secrets
              .firstWhere((s) => s.name == selected)
              .title;
          _apiKey.text = _selectedSecretTitle!.isEmpty
              ? selected
              : _selectedSecretTitle!;
        });
      }
    } catch (_) {
      if (mounted) setState(() => _error = context.l10n.secretLoadFailed);
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _baseUrl.dispose();
    _model.dispose();
    _apiKey.dispose();
    _temperature.dispose();
    _contextWindow.dispose();
    _maxTokens.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(
        widget.isNew
            ? (widget.isDuplicate
                  ? context.l10n.duplicateProvider
                  : context.l10n.addProvider)
            : context.l10n.editProvider,
      ),
      content: SingleChildScrollView(
        child: SizedBox(
          width: 440,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _UsageFonts._buildTextField(
                label: context.l10n.configurationNameRequired,
                hint: context.l10n.configurationNameHint,
                controller: _name,
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildDropdownField<String>(
                label: context.l10n.providerType,
                value: _providerType,
                items: [
                  DropdownMenuItem(
                    value: 'openai-compatible',
                    child: Text(context.l10n.openAiCompatible),
                  ),
                  DropdownMenuItem(value: 'ollama', child: Text('Ollama')),
                ],
                onChanged: (value) {
                  if (value != null) {
                    setState(() => _providerType = value);
                  }
                },
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildTextField(
                label: 'Base URL *',
                hint: 'https://api.openai.com/v1',
                controller: _baseUrl,
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildTextField(
                label: context.l10n.modelRequired,
                hint: 'gpt-4o',
                controller: _model,
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Text(
                    'API Key',
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.w500,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                  const Spacer(),
                  Text(
                    _useSecret
                        ? context.l10n.secretVault
                        : context.l10n.manualInput,
                    key: const ValueKey('provider-key-source'),
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              TextFormField(
                key: const ValueKey('provider-api-key'),
                controller: _apiKey,
                obscureText: !_useSecret,
                enableSuggestions: false,
                autocorrect: false,
                onChanged: (_) {
                  if (_useSecret) {
                    setState(() {
                      _useSecret = false;
                      _selectedSecret = null;
                      _selectedSecretTitle = null;
                    });
                  }
                },
                decoration: InputDecoration(
                  hintText: widget.isNew
                      ? context.l10n.apiKeyHintNew
                      : context.l10n.apiKeyHintExisting,
                  filled: true,
                  fillColor: AppTheme.surface2.withValues(alpha: 0.5),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                  ),
                  suffixIcon: IconButton(
                    key: const ValueKey('provider-secret-picker'),
                    tooltip: context.l10n.chooseFromVault,
                    onPressed: _saving ? null : _chooseSecret,
                    icon: const Icon(Icons.key_outlined, size: 20),
                  ),
                ),
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildTextField(
                label: context.l10n.contextWindow,
                hint: context.l10n.contextWindowHint,
                keyboardType: TextInputType.number,
                controller: _contextWindow,
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: _UsageFonts._buildTextField(
                      label: 'Temperature',
                      hint: '0.7',
                      keyboardType: TextInputType.number,
                      controller: _temperature,
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: _UsageFonts._buildTextField(
                      label: context.l10n.maximumOutput,
                      hint: context.l10n.optional,
                      keyboardType: TextInputType.number,
                      controller: _maxTokens,
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
      actions: [
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: Align(
              alignment: Alignment.centerLeft,
              child: AppErrorMessage(
                action: context.l10n.load,
                error: _error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          ),
        TextButton(
          onPressed: _saving ? null : () => Navigator.pop(context),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: _saving
              ? null
              : () async {
                  if (_useSecret && _selectedSecret == null) {
                    setState(() => _error = context.l10n.selectSecretFirst);
                    return;
                  }
                  final draft = _ProviderDraft(
                    name: _name.text.trim(),
                    providerType: _providerType,
                    baseUrl: _baseUrl.text.trim(),
                    model: _model.text.trim(),
                    apiKey: _useSecret && _selectedSecret != null
                        ? '{{secret:$_selectedSecret}}'
                        : _apiKey.text.trim(),
                    temperature: double.tryParse(_temperature.text) ?? 0.7,
                    maxTokens: int.tryParse(_maxTokens.text),
                    contextWindow: int.tryParse(_contextWindow.text),
                  );
                  final validationError = draft.name.isEmpty
                      ? context.l10n.providerNameMissing
                      : draft.baseUrl.isEmpty || draft.model.isEmpty
                      ? context.l10n.providerFieldsMissing
                      : widget.isNew && draft.apiKey.isEmpty
                      ? context.l10n.apiKeyRequiredForNew
                      : null;
                  if (validationError != null) {
                    setState(() => _error = validationError);
                    return;
                  }
                  setState(() {
                    _saving = true;
                    _error = null;
                  });
                  try {
                    await widget.onSave(draft);
                    if (!context.mounted) return;
                    Navigator.pop(context, draft);
                  } catch (e) {
                    if (!mounted) return;
                    setState(() {
                      _saving = false;
                      // Required fields are explained above in the UI language;
                      // unexpected failures retain the diagnostic logging path.
                      _error = appFailure(e);
                    });
                  }
                },
          child: _saving
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : Text(context.l10n.save),
        ),
      ],
    );
  }
}

/// 每日 Token 用量柱状图。不引图表库（项目无此依赖），CustomPaint 手绘：
/// 柱高 = 当日 totalTokens / 7 天峰值，今天的柱子用强调色，其余用次级色。
/// 柱底标 MM-dd，今天额外标「今天」。
class _TokenUsageBarChart extends StatelessWidget {
  const _TokenUsageBarChart({required this.days, required this.today});

  final List<DailyTokenUsage> days;
  final String today;

  @override
  Widget build(BuildContext context) {
    final peak = days.fold<int>(
      1,
      (m, d) => d.totalTokens > m ? d.totalTokens : m,
    );
    // >10 根柱子时柱顶数字与每日日期会挤成一团：只留今天与首尾日期，
    // 柱顶数字仅最高柱保留。
    final compact = days.length > 10;
    final maxIndex = days.indexWhere((d) => d.totalTokens == peak);
    return SizedBox(
      height: 140,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (final (i, d) in days.indexed)
            Expanded(
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: compact ? 1.5 : 4),
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    if (!compact || i == maxIndex)
                      Text(
                        _fmtShort(d.totalTokens),
                        style: TextStyle(
                          fontSize: 10,
                          color: AppTheme.textTertiary,
                        ),
                      )
                    else
                      const SizedBox(height: 13),
                    const SizedBox(height: 4),
                    Flexible(
                      child: FractionallySizedBox(
                        heightFactor: (d.totalTokens / peak).clamp(0.02, 1.0),
                        child: Container(
                          decoration: BoxDecoration(
                            color: d.date == today
                                ? AppTheme.accentPrimary
                                : AppTheme.accentPrimary.withValues(
                                    alpha: 0.35,
                                  ),
                            borderRadius: const BorderRadius.vertical(
                              top: Radius.circular(4),
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 6),
                    Text(
                      _dateLabel(context, i, d, compact),
                      style: TextStyle(
                        fontSize: compact ? 9 : 11,
                        fontWeight: d.date == today
                            ? FontWeight.w600
                            : FontWeight.w400,
                        color: d.date == today
                            ? AppTheme.accentPrimary
                            : AppTheme.textTertiary,
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }

  /// 紧凑模式只标今天与首末两根柱子的日期，其余留空，防止日期串粘在一起。
  String _dateLabel(
    BuildContext context,
    int i,
    DailyTokenUsage d,
    bool compact,
  ) {
    if (d.date == today) return context.l10n.today;
    final date = DateFormat.Md(context.l10n.localeName)
        .format(DateTime.parse(d.date));
    if (!compact) return date;
    if (i == 0 || i == days.length - 1) return date;
    return '';
  }

  /// 柱顶数字：>=1000 缩写为 x.xK，避免长数字挤在一起。
  static String _fmtShort(int n) {
    if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(1)}M';
    if (n >= 1000) return '${(n / 1000).toStringAsFixed(1)}K';
    return '$n';
  }
}

/// MCP 服务器表单对话框的保存结果。
class _McpServerFormResult {
  final String name;
  final String keywords;

  /// 平台标签（迁移 48）：定义随人走，激活看平台。同一名 server 可有
  /// 跨平台变体（如 Windows 上 npx.cmd）；远程服务器通常标 all。
  final String platform;
  final String transport;
  final String command;
  final String argsJson;
  final String envJson;
  final String url;
  final String headersJson;
  final bool trusted;
  const _McpServerFormResult({
    required this.name,
    required this.keywords,
    required this.platform,
    required this.transport,
    required this.command,
    required this.argsJson,
    required this.envJson,
    required this.url,
    required this.headersJson,
    required this.trusted,
  });
}
