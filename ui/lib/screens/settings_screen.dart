import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../providers/settings_provider.dart';
import '../models/settings.dart';
import '../models/token_usage.dart';
import '../bridge/rust_bridge_repository.dart';
import '../widgets/custom_title_bar.dart';
import '../theme/app_theme.dart';

class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});

  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends ConsumerState<SettingsScreen>
    with SingleTickerProviderStateMixin {
  final _formKey = GlobalKey<FormState>();

  late final TabController _tabController;

  late TextEditingController _baseUrlController;
  late TextEditingController _modelController;
  late TextEditingController _apiKeyController;
  late TextEditingController _temperatureController;
  late TextEditingController _maxTokensController;
  late TextEditingController _maxMessagesController;

  List<DailyTokenUsage> _dailyUsage = [];
  bool _usageLoading = true;

  // 推文抓取服务（当前仅支持 fxtwitter）
  String _tweetService = 'fxtwitter';

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 5, vsync: this);
    final settings = ref.read(settingsProvider);

    _baseUrlController = TextEditingController(text: settings.aiProvider.baseUrl);
    _modelController = TextEditingController(text: settings.aiProvider.model);
    _apiKeyController = TextEditingController(text: settings.aiProvider.apiKey);
    _temperatureController = TextEditingController(
      text: settings.aiProvider.temperature.toString(),
    );
    _maxTokensController = TextEditingController(
      text: settings.aiProvider.maxTokens?.toString() ?? '',
    );
    _maxMessagesController = TextEditingController(
      text: settings.memory.maxMessages?.toString() ?? '20',
    );

    // 从 Rust 侧读取当前生效的 AI provider（.env 导入的那份），覆写表单；
    // 同时拉取每日 token 用量统计
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _loadFromBridge();
      _loadTokenUsage();
      _loadTweetService();
    });
  }

  Future<void> _loadFromBridge() async {
    try {
      final notifier = ref.read(settingsProvider.notifier);
      await notifier.loadSettings();
      final ai = ref.read(settingsProvider).aiProvider;
      _baseUrlController.text = ai.baseUrl;
      _modelController.text = ai.model;
      _apiKeyController.text = ai.apiKey;
      if (mounted) setState(() {});
    } catch (e) {
      // 读取失败不阻塞表单，用户仍可手动填写
      debugPrint('loadSettings failed: $e');
    }
  }

  Future<void> _loadTokenUsage() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final daily = await repo.getDailyTokenUsage(7);
      if (!mounted) return;
      setState(() {
        _dailyUsage = daily;
        _usageLoading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _usageLoading = false);
      debugPrint('loadTokenUsage failed: $e');
    }
  }

  Future<void> _loadTweetService() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final svc = await repo.getTweetFetchService();
      if (!mounted) return;
      setState(() => _tweetService = svc);
    } catch (e) {
      debugPrint('loadTweetService failed: $e');
    }
  }

  @override
  void dispose() {
    _tabController.dispose();
    _baseUrlController.dispose();
    _modelController.dispose();
    _apiKeyController.dispose();
    _temperatureController.dispose();
    _maxTokensController.dispose();
    _maxMessagesController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final settings = ref.watch(settingsProvider);

    return Scaffold(
      body: Column(
        children: [
          CustomTitleBar(
            title: '设置',
            actions: [
              IconButton(
                icon: const Icon(Icons.arrow_back, size: 18),
                tooltip: '返回',
                color: const Color(0xFF9BA1AB),
                onPressed: () => Navigator.of(context).pop(),
              ),
              TextButton.icon(
                icon: const Icon(Icons.check, size: 18),
                label: const Text('保存'),
                onPressed: () => _saveSettings(),
                style: TextButton.styleFrom(
                  foregroundColor: AppTheme.accentPrimary,
                ),
              ),
            ],
          ),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [
                    AppTheme.surface0,
                    AppTheme.surface1,
                    AppTheme.surface2,
                  ],
                ),
              ),
              child: SafeArea(
                child: Form(
                  key: _formKey,
                  child: Column(
                    children: [
                      _buildTabBar(),
                      Expanded(
                        child: TabBarView(
                          controller: _tabController,
                          children: [
                            _buildModelTab(settings),
                            _buildAppearanceTab(settings),
                            _buildServiceTab(),
                            _buildStorageTab(settings),
                            _buildDataTab(),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildTabBar() {
    return Container(
      height: 44,
      color: AppTheme.surface1,
      child: TabBar(
        controller: _tabController,
        tabs: const [
          Tab(text: '模型'),
          Tab(text: '外观'),
          Tab(text: '服务'),
          Tab(text: '存储'),
          Tab(text: '数据'),
        ],
        labelColor: AppTheme.accentPrimary,
        unselectedLabelColor: AppTheme.textTertiary,
        indicatorColor: AppTheme.accentPrimary,
        indicatorSize: TabBarIndicatorSize.label,
        dividerColor: Colors.transparent,
        labelStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      ),
    );
  }

  Widget _buildModelTab(AppSettings settings) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: 'AI 提供商',
            icon: Icons.psychology_outlined,
            child: _buildAiProviderSettings(settings),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '记忆策略',
            icon: Icons.memory_outlined,
            child: _buildMemorySettings(settings),
          ),
        ],
      ),
    );
  }

  Widget _buildAppearanceTab(AppSettings settings) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '外观',
            icon: Icons.palette_outlined,
            child: _buildAppearanceSettings(settings),
          ),
        ],
      ),
    );
  }

  Widget _buildServiceTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '推文抓取',
            icon: Icons.link_outlined,
            child: _buildTweetFetchSettings(),
          ),
        ],
      ),
    );
  }

  Widget _buildStorageTab(AppSettings settings) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '存储适配器',
            icon: Icons.storage_outlined,
            child: _buildStorageSettings(settings),
          ),
        ],
      ),
    );
  }

  Widget _buildDataTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '每日 Token 使用',
            icon: Icons.analytics_outlined,
            child: _buildTokenUsageContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildSection({
    required String title,
    required IconData icon,
    required Widget child,
  }) {
    return Container(
      padding: const EdgeInsets.all(24),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(
          color: AppTheme.accentPrimary.withValues(alpha: 0.1),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: AppTheme.accentPrimary.withValues(alpha: 0.1),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Icon(icon, color: AppTheme.accentPrimary, size: 20),
              ),
              const SizedBox(width: 12),
              Text(
                title,
                style: TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
            ],
          ),
          const SizedBox(height: 20),
          child,
        ],
      ),
    );
  }

  Widget _buildAiProviderSettings(AppSettings settings) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: '提供商类型',
          value: settings.aiProvider.providerType,
          items: const [
            DropdownMenuItem(value: 'openai-compatible', child: Text('OpenAI 兼容')),
            DropdownMenuItem(value: 'ollama', child: Text('Ollama')),
          ],
          onChanged: (value) {
            if (value != null) {
              ref.read(settingsProvider.notifier).updateAiProvider(
                settings.aiProvider.copyWith(providerType: value),
              );
            }
          },
        ),
        const SizedBox(height: 16),
        _buildTextField(
          label: 'Base URL',
          controller: _baseUrlController,
          hint: 'https://api.openai.com/v1',
        ),
        const SizedBox(height: 16),
        _buildTextField(
          label: '模型',
          controller: _modelController,
          hint: 'gpt-3.5-turbo',
        ),
        const SizedBox(height: 16),
        _buildTextField(
          label: 'API Key',
          controller: _apiKeyController,
          obscure: true,
          hint: '输入你的 API Key',
        ),
        const SizedBox(height: 16),
        Row(
          children: [
            Expanded(
              child: _buildTextField(
                label: 'Temperature',
                controller: _temperatureController,
                hint: '0.7',
                keyboardType: TextInputType.number,
              ),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: _buildTextField(
                label: 'Max Tokens',
                controller: _maxTokensController,
                hint: '可选',
                keyboardType: TextInputType.number,
              ),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildMemorySettings(AppSettings settings) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: '策略类型',
          value: settings.memory.strategyType,
          items: const [
            DropdownMenuItem(value: 'simple', child: Text('简单记忆 (最近 N 条)')),
            DropdownMenuItem(value: 'sliding-window', child: Text('滑动窗口 (Token 限制)')),
          ],
          onChanged: (value) {
            if (value != null) {
              ref.read(settingsProvider.notifier).updateMemory(
                settings.memory.copyWith(strategyType: value),
              );
            }
          },
        ),
        const SizedBox(height: 16),
        if (settings.memory.strategyType == 'simple')
          _buildTextField(
            label: '最大消息数',
            controller: _maxMessagesController,
            hint: '20',
            keyboardType: TextInputType.number,
          ),
        if (settings.memory.strategyType == 'sliding-window')
          _buildTextField(
            label: '最大 Token 数',
            controller: TextEditingController(
              text: settings.memory.maxTokens?.toString() ?? '4096',
            ),
            hint: '4096',
            keyboardType: TextInputType.number,
          ),
      ],
    );
  }

  Widget _buildStorageSettings(AppSettings settings) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: '适配器类型',
          value: settings.storage.adapterType,
          items: const [
            DropdownMenuItem(value: 'sqlite', child: Text('SQLite')),
          ],
          onChanged: (value) {
            if (value != null) {
              ref.read(settingsProvider.notifier).updateStorage(
                settings.storage.copyWith(adapterType: value),
              );
            }
          },
        ),
        const SizedBox(height: 12),
        Text(
          '数据库路径: ${settings.storage.databasePath ?? "默认"}',
          style: TextStyle(
            fontSize: 13,
            color: AppTheme.textSecondary.withValues(alpha: 0.7),
          ),
        ),
      ],
    );
  }

  Widget _buildTokenUsageContent() {
    if (_usageLoading) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 16),
        child: Center(
          child: SizedBox(
            width: 22,
            height: 22,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
      );
    }
    if (_dailyUsage.isEmpty) {
      return Text(
        '暂无 AI 调用记录。发起对话并生成 AI 回复后，这里会按天统计 token 用量。',
        style: TextStyle(color: AppTheme.textSecondary, fontSize: 13, height: 1.6),
      );
    }

    final today = _isoDate(DateTime.now());
    final totalTokens = _dailyUsage.fold<int>(0, (s, d) => s + d.totalTokens);
    final totalCalls = _dailyUsage.fold<int>(0, (s, d) => s + d.callCount);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: _buildUsageMetric(label: '近 7 天 Total', value: _fmtInt(totalTokens)),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: _buildUsageMetric(label: '累计调用', value: '${_fmtInt(totalCalls)} 次'),
            ),
          ],
        ),
        const SizedBox(height: 20),
        Row(
          children: [
            SizedBox(
              width: 96,
              child: Text('日期', style: TextStyle(fontSize: 12, color: AppTheme.textTertiary)),
            ),
            Expanded(
              child: Text('Total', style: TextStyle(fontSize: 12, color: AppTheme.textTertiary)),
            ),
            Expanded(
              child: Text('调用', textAlign: TextAlign.right, style: TextStyle(fontSize: 12, color: AppTheme.textTertiary)),
            ),
          ],
        ),
        ..._dailyUsage.take(7).map(
          (d) => Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Row(
              children: [
                SizedBox(
                  width: 96,
                  child: Text(
                    d.date == today ? '今天（${d.date.substring(5)}）' : d.date.substring(5),
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w500,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
                Expanded(
                  child: Text(
                    '${_fmtInt(d.totalTokens)}',
                    style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
                  ),
                ),
                Expanded(
                  child: Text(
                    '${_fmtInt(d.callCount)} 次',
                    textAlign: TextAlign.right,
                    style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildUsageMetric({required String label, required String value}) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
        ),
        const SizedBox(height: 4),
        Text(
          value,
          style: TextStyle(
            fontSize: 22,
            fontWeight: FontWeight.w700,
            color: AppTheme.accentPrimary,
          ),
        ),
      ],
    );
  }

  /// 千分位格式化（避免依赖 intl locale 初始化）
  String _fmtInt(int n) {
    final s = n.toString();
    final buf = StringBuffer();
    for (var i = 0; i < s.length; i++) {
      if (i > 0 && (s.length - i) % 3 == 0) buf.write(',');
      buf.write(s[i]);
    }
    return buf.toString();
  }

  String _isoDate(DateTime dt) {
    final m = dt.month.toString().padLeft(2, '0');
    final d = dt.day.toString().padLeft(2, '0');
    return '${dt.year}-$m-$d';
  }

  Widget _buildTweetFetchSettings() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: '抓取服务',
          value: _tweetService,
          items: const [
            DropdownMenuItem(value: 'fxtwitter', child: Text('fxTwitter')),
          ],
          onChanged: (value) async {
            if (value == null || value == _tweetService) return;
            setState(() => _tweetService = value);
            try {
              final repo =
                  ref.read(storageRepositoryProvider) as RustBridgeRepository;
              await repo.updateTweetFetchService(value);
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: Text('推文抓取服务已更新'),
                  backgroundColor: AppTheme.accentPrimary,
                  behavior: SnackBarBehavior.floating,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.all(Radius.circular(999)),
                  ),
                ),
              );
            } catch (e) {
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: Text('保存失败：$e'),
                  backgroundColor: AppTheme.error,
                  behavior: SnackBarBehavior.floating,
                  shape: const RoundedRectangleBorder(
                    borderRadius: BorderRadius.all(Radius.circular(999)),
                  ),
                ),
              );
            }
          },
        ),
        const SizedBox(height: 12),
        Text(
          '当前支持的抓取服务：fxTwitter —— 把 x.com 推文链接自动解析为长文，\n接口 https://api.fxtwitter.com/status/{推文id}',
          style: TextStyle(
            fontSize: 12,
            color: AppTheme.textSecondary.withValues(alpha: 0.7),
            height: 1.6,
          ),
        ),
      ],
    );
  }

  Widget _buildAppearanceSettings(AppSettings settings) {
    final notifier = ref.read(settingsProvider.notifier);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: '主题模式',
          value: settings.themeMode.name,
          items: AppThemeMode.values.map((mode) {
            return DropdownMenuItem(
              value: mode.name,
              child: Text(mode.displayName),
            );
          }).toList(),
          onChanged: (value) {
            if (value != null) {
              final mode = AppThemeMode.values.firstWhere((m) => m.name == value);
              notifier
                ..updateTheme(mode)
                ..saveTheme();
            }
          },
        ),
        const SizedBox(height: 16),
        _buildDropdownField(
          label: '主题配色',
          value: settings.themePreset.name,
          items: AppThemePreset.values.map((preset) {
            return DropdownMenuItem(
              value: preset.name,
              child: Text(preset.displayName),
            );
          }).toList(),
          onChanged: (value) {
            if (value != null) {
              final preset = AppThemePreset.fromName(value);
              notifier
                ..updateThemePreset(preset)
                ..saveTheme();
            }
          },
        ),
      ],
    );
  }

  Widget _buildTextField({
    required String label,
    required TextEditingController controller,
    String? hint,
    bool obscure = false,
    TextInputType? keyboardType,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        TextFormField(
          controller: controller,
          obscureText: obscure,
          keyboardType: keyboardType,
          style: TextStyle(color: AppTheme.textPrimary),
          decoration: InputDecoration(
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
              borderSide: BorderSide(
                color: AppTheme.accentPrimary,
                width: 2,
              ),
            ),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 16,
              vertical: 14,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildDropdownField<T>({
    required String label,
    required T value,
    required List<DropdownMenuItem<T>> items,
    required ValueChanged<T?> onChanged,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        DropdownButtonFormField<T>(
          value: value,
          items: items,
          onChanged: onChanged,
          style: TextStyle(color: AppTheme.textPrimary),
          dropdownColor: AppTheme.surface2,
          decoration: InputDecoration(
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
              borderSide: BorderSide(
                color: AppTheme.accentPrimary,
                width: 2,
              ),
            ),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 16,
              vertical: 14,
            ),
          ),
        ),
      ],
    );
  }

  Future<void> _saveSettings() async {
    if (_formKey.currentState?.validate() ?? false) {
      final currentSettings = ref.read(settingsProvider);

      ref.read(settingsProvider.notifier).updateAiProvider(
        currentSettings.aiProvider.copyWith(
          baseUrl: _baseUrlController.text,
          model: _modelController.text,
          apiKey: _apiKeyController.text,
          temperature: double.tryParse(_temperatureController.text) ?? 0.7,
          maxTokens: int.tryParse(_maxTokensController.text),
        ),
      );

      ref.read(settingsProvider.notifier).updateMemory(
        currentSettings.memory.copyWith(
          maxMessages: int.tryParse(_maxMessagesController.text),
        ),
      );

      try {
        await ref.read(settingsProvider.notifier).saveSettings();
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('设置已保存'),
            backgroundColor: AppTheme.accentPrimary,
            behavior: SnackBarBehavior.floating,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.all(Radius.circular(999)),
            ),
          ),
        );
      } catch (e) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('保存失败：$e'),
            backgroundColor: AppTheme.error,
            behavior: SnackBarBehavior.floating,
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.all(Radius.circular(999)),
            ),
          ),
        );
      }
    }
  }
}
