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
  });

  final bool isNew;
  final bool isDuplicate;
  final String initialName;

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

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: widget.initialName);
    _baseUrl = TextEditingController(text: widget.initialBaseUrl);
    _model = TextEditingController(text: widget.initialModel);
    _apiKey = TextEditingController(text: '');
    _temperature = TextEditingController(
      text: widget.initialTemperature.toString(),
    );
    _contextWindow = TextEditingController(text: widget.initialContextWindow);
    _maxTokens = TextEditingController(text: widget.initialMaxTokens);
    _providerType = widget.initialProviderType;
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
            ? (widget.isDuplicate ? '复制 AI Provider' : '添加 AI Provider')
            : '编辑 Provider',
      ),
      content: SingleChildScrollView(
        child: SizedBox(
          width: 440,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _UsageFonts._buildTextField(
                label: '配置名称 *',
                hint: '如：主用 GPT',
                controller: _name,
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildDropdownField<String>(
                label: '提供商类型',
                value: _providerType,
                items: const [
                  DropdownMenuItem(
                    value: 'openai-compatible',
                    child: Text('OpenAI 兼容'),
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
                label: '模型 *',
                hint: 'gpt-4o',
                controller: _model,
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildTextField(
                label: widget.isNew ? 'API Key *' : 'API Key',
                hint: widget.isNew ? '输入你的 API Key' : '留空表示保持不变',
                obscure: true,
                controller: _apiKey,
              ),
              const SizedBox(height: 12),
              _UsageFonts._buildTextField(
                label: '上下文窗口（token）',
                hint: '留空使用保守默认值；中转可填写实测上限',
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
                      label: '输出上限（token）',
                      hint: '可选',
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
              child: Text(
                _error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          ),
        TextButton(
          onPressed: _saving ? null : () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: _saving
              ? null
              : () async {
                  final draft = _ProviderDraft(
                    name: _name.text.trim(),
                    providerType: _providerType,
                    baseUrl: _baseUrl.text.trim(),
                    model: _model.text.trim(),
                    apiKey: _apiKey.text.trim(),
                    temperature: double.tryParse(_temperature.text) ?? 0.7,
                    maxTokens: int.tryParse(_maxTokens.text),
                    contextWindow: int.tryParse(_contextWindow.text),
                  );
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
                      // save_ai_provider_config 的三条校验是刻意写给用户看的
                      // （配置名称不能为空 / base_url 和 model 均不能为空 /
                      // 新建配置时必须填写 API Key），隐藏它们等于让用户瞎猜。
                      _error = reportUiError(
                        '保存 Provider',
                        e,
                        showDetail: true,
                      );
                    });
                  }
                },
          child: _saving
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('保存'),
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
                      _dateLabel(i, d, compact),
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
  String _dateLabel(int i, DailyTokenUsage d, bool compact) {
    if (d.date == today) return '今天';
    if (!compact) return d.date.substring(5);
    if (i == 0 || i == days.length - 1) return d.date.substring(5);
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
