import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/settings_provider.dart';
import '../models/settings.dart';
import '../models/token_usage.dart';
import '../models/rule.dart';
import '../bridge/rust_bridge_repository.dart';
import '../bridge/generated.dart/api.dart' as api;
import '../widgets/custom_title_bar.dart';
import '../widgets/font_picker_dialog.dart';
import '../widgets/knowledge_digest_dialog.dart';
import '../utils/system_fonts.dart';
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

  late TextEditingController _maxMessagesController;

  late TextEditingController _maxTokensController;

  List<DailyTokenUsage> _dailyUsage = [];
  bool _usageLoading = true;
  api.AnalysisJobStatsDto? _analysisJobStats;
  bool _analysisJobStatsLoading = true;
  api.KnowledgeDigestStatsDto? _digestStats;
  bool _digestStatsLoading = true;

  // 个人经验规则库
  List<Rule> _rules = [];
  bool _rulesLoading = true;

  // AI provider 多配置（仅一个激活）
  List<api.AiProviderConfigDto> _providers = [];
  bool _providersLoading = true;

  // 推文抓取服务（当前仅支持 fxtwitter）
  String _tweetService = 'fxtwitter';

  // 启动期各区块读取失败的原因（区块名 → 异常文本）。
  //
  // 空列表区分不了「本来就没有」和「读取失败」：后者原本被 catch 掉后只 debugPrint，
  // 于是渲染成「还没有规则」「未配置」这类**正常空状态**，把故障说成了没数据。
  // 这里单独记下原因，空状态分支先判它，失败与真空才分得开。
  final Map<String, String> _loadErrors = {};

  static const _secProviders = 'AI Provider 配置';
  static const _secTokenUsage = '每日 Token 用量';
  static const _secAnalysisJobs = '事件分析队列';
  static const _secDigest = '知识消化';
  static const _secRules = '规则库';
  static const _secTweetService = '推文抓取服务';

  /// 失败提示文案；null 表示该区块读取成功（或还在读）。
  ///
  /// 只取异常的首行：Rust 侧 `anyhow` 的 `toString()` 会带 "Caused by:" 多行细节，
  /// 整段塞进单行 Text 很难读。
  String? _loadErrorOf(String section) {
    final raw = _loadErrors[section];
    if (raw == null) return null;
    final firstLine = raw
        .split('\n')
        .map((s) => s.trim())
        .firstWhere((s) => s.isNotEmpty, orElse: () => raw.trim());
    return '$section读取失败：$firstLine';
  }

  /// 区块级失败提示控件。各区块的「空状态」分支应先判它再决定是否显示空状态。
  Widget? _buildLoadErrorHint(String section) {
    final message = _loadErrorOf(section);
    if (message == null) return null;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AppTheme.space3),
      child: Text(
        message,
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(color: AppTheme.error, fontSize: 12),
      ),
    );
  }

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 5, vsync: this);
    final settings = ref.read(settingsProvider);

    _maxMessagesController = TextEditingController(
      text: settings.memory.maxMessages?.toString() ?? '20',
    );

    _maxTokensController = TextEditingController(
      text: settings.memory.maxTokens?.toString() ?? '4096',
    );

    // 从 Rust 侧读取各区块数据（AI provider 多配置 / token 用量 / 推文服务 / 规则库）
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _loadProviders();
      _loadTokenUsage();
      _loadAnalysisJobStats();
      _loadDigestStats();
      _loadTweetService();
      _loadRules();
    });
  }

  Future<void> _loadProviders() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final list = await repo.listAiProviderConfigs();
      if (!mounted) return;
      setState(() {
        _providers = list;
        _providersLoading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _providersLoading = false;
        _loadErrors[_secProviders] = '$e';
      });
      debugPrint('listAiProviderConfigs failed: $e');
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
      setState(() {
        _usageLoading = false;
        _loadErrors[_secTokenUsage] = '$e';
      });
      debugPrint('loadTokenUsage failed: $e');
    }
  }

  Future<void> _loadAnalysisJobStats() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final stats = await repo.getAnalysisJobStats();
      if (!mounted) return;
      setState(() {
        _analysisJobStats = stats;
        _analysisJobStatsLoading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _analysisJobStatsLoading = false;
        _loadErrors[_secAnalysisJobs] = '$e';
      });
      debugPrint('loadAnalysisJobStats failed: $e');
    }
  }

  Future<void> _loadDigestStats() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final stats = await repo.getKnowledgeDigestStats();
      if (!mounted) return;
      setState(() {
        _digestStats = stats;
        _digestStatsLoading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _digestStatsLoading = false;
        _loadErrors[_secDigest] = '$e';
      });
      debugPrint('loadDigestStats failed: $e');
    }
  }

  Future<void> _loadTweetService() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final svc = await repo.getTweetFetchService();
      if (!mounted) return;
      setState(() => _tweetService = svc);
    } catch (e) {
      if (!mounted) return;
      setState(() => _loadErrors[_secTweetService] = '$e');
      debugPrint('loadTweetService failed: $e');
    }
  }

  Future<void> _loadRules() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final all = await repo.listRules();
      if (!mounted) return;
      setState(() {
        _rules = all.where((r) => r.isActive).toList();
        _rulesLoading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _rulesLoading = false;
        _loadErrors[_secRules] = '$e';
      });
      debugPrint('loadRules failed: $e');
    }
  }

  Future<void> _deleteRule(Rule rule) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteRule(rule.id);
      if (!mounted) return;
      setState(() => _rules.removeWhere((r) => r.id == rule.id));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('删除失败：$e')));
    }
  }

  @override
  void dispose() {
    _tabController.dispose();
    _maxMessagesController.dispose();
    _maxTokensController.dispose();
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
                tooltip: '返回主界面',
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
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      final compact = constraints.maxWidth < 700;
                      final content = TabBarView(
                        controller: _tabController,
                        children: [
                          _buildModelTab(settings),
                          _buildAppearanceTab(settings),
                          _buildServiceTab(),
                          _buildStorageTab(settings),
                          _buildDataTab(),
                        ],
                      );
                      if (compact) {
                        return Column(
                          children: [
                            _buildCompactTabBar(),
                            Expanded(child: content),
                          ],
                        );
                      }
                      return Row(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _buildVerticalTabBar(),
                          Expanded(child: content),
                        ],
                      );
                    },
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildVerticalTabBar() {
    return Container(
      width: 176,
      padding: const EdgeInsets.all(AppTheme.space2),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(right: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: AnimatedBuilder(
        animation: _tabController,
        builder: (context, _) => ListView(
          children: [
            _buildSettingsTabItem(0, '模型', Icons.smart_toy_outlined),
            const SizedBox(height: AppTheme.space1),
            _buildSettingsTabItem(1, '外观', Icons.palette_outlined),
            const SizedBox(height: AppTheme.space1),
            _buildSettingsTabItem(2, '服务', Icons.cloud_outlined),
            const SizedBox(height: AppTheme.space1),
            _buildSettingsTabItem(3, '存储', Icons.storage_outlined),
            const SizedBox(height: AppTheme.space1),
            _buildSettingsTabItem(4, '数据', Icons.dataset_outlined),
          ],
        ),
      ),
    );
  }

  Widget _buildCompactTabBar() {
    return Material(
      color: AppTheme.surface1,
      child: TabBar(
        controller: _tabController,
        isScrollable: true,
        tabAlignment: TabAlignment.start,
        labelColor: AppTheme.accentPrimary,
        unselectedLabelColor: AppTheme.textTertiary,
        indicatorColor: AppTheme.accentPrimary,
        labelPadding: const EdgeInsets.symmetric(horizontal: 16),
        tabs: const [
          Tab(text: '模型', icon: Icon(Icons.smart_toy_outlined, size: 17)),
          Tab(text: '外观', icon: Icon(Icons.palette_outlined, size: 17)),
          Tab(text: '服务', icon: Icon(Icons.cloud_outlined, size: 17)),
          Tab(text: '存储', icon: Icon(Icons.storage_outlined, size: 17)),
          Tab(text: '数据', icon: Icon(Icons.dataset_outlined, size: 17)),
        ],
      ),
    );
  }

  Widget _buildSettingsTabItem(int index, String label, IconData icon) {
    final selected = _tabController.index == index;
    return Material(
      color: selected ? AppTheme.surface2 : Colors.transparent,
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      child: InkWell(
        onTap: () => _tabController.animateTo(index),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space3,
            vertical: 10,
          ),
          child: Row(
            children: [
              Icon(
                icon,
                size: 16,
                color: selected
                    ? AppTheme.accentPrimary
                    : AppTheme.textTertiary,
              ),
              const SizedBox(width: AppTheme.space2 + 4),
              Text(
                label,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                  color: selected
                      ? AppTheme.textPrimary
                      : AppTheme.textTertiary,
                ),
              ),
            ],
          ),
        ),
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

  Widget _buildAnalysisJobStatsContent() {
    if (_analysisJobStatsLoading) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    // 失败优先于「没有数据」：否则读取失败会被渲染成空队列，看不出是故障。
    final jobsError = _buildLoadErrorHint(_secAnalysisJobs);
    if (jobsError != null) return jobsError;
    final stats = _analysisJobStats;
    if (stats == null) {
      return Text(
        '分析队列状态读取失败',
        style: TextStyle(color: AppTheme.textSecondary),
      );
    }

    final waiting = stats.pending + stats.running + stats.retry;
    return Wrap(
      spacing: AppTheme.space4,
      runSpacing: AppTheme.space3,
      children: [
        _buildUsageMetric(label: '待处理', value: '$waiting'),
        _buildUsageMetric(label: '处理中', value: '${stats.running}'),
        _buildUsageMetric(label: '等待重试', value: '${stats.retry}'),
        _buildUsageMetric(label: '已完成', value: '${stats.succeeded}'),
        _buildUsageMetric(label: '失败', value: '${stats.failed}'),
      ],
    );
  }

  Widget _buildDigestStatsContent() {
    if (_digestStatsLoading) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    final digestError = _buildLoadErrorHint(_secDigest);
    if (digestError != null) return digestError;
    final stats = _digestStats;
    if (stats == null) {
      return Text(
        '知识消化状态读取失败',
        style: TextStyle(color: AppTheme.textSecondary),
      );
    }
    String when(String? rfc3339) {
      final parsed = rfc3339 == null ? null : DateTime.tryParse(rfc3339);
      if (parsed == null) return '—';
      final local = parsed.toLocal();
      String two(int v) => v.toString().padLeft(2, '0');
      return '${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
    }

    final waiting = stats.pending + stats.running + stats.retry;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '记录的事件会在后台自动整理进知识库；失败项冷却后会自动重试。',
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 13,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        Wrap(
          spacing: AppTheme.space4,
          runSpacing: AppTheme.space3,
          children: [
            _buildUsageMetric(label: '待处理', value: '$waiting'),
            _buildUsageMetric(label: '已完成', value: '${stats.succeeded}'),
            _buildUsageMetric(label: '失败', value: '${stats.failed}'),
            _buildUsageMetric(label: '跳过', value: '${stats.skipped}'),
            _buildUsageMetric(label: '最近成功', value: when(stats.lastSuccessAt)),
          ],
        ),
        if (stats.lastError != null && stats.lastError!.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space3),
          Text(
            '最近失败（${when(stats.lastErrorAt)}）：${stats.lastError}',
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(color: AppTheme.error, fontSize: 12),
          ),
        ],
        const SizedBox(height: AppTheme.space3),
        OutlinedButton.icon(
          icon: const Icon(Icons.list_alt_outlined, size: 16),
          label: const Text('查看队列与运行日志'),
          onPressed: () async {
            await showKnowledgeDigestDialog(context);
            if (mounted) _loadDigestStats();
          },
        ),
      ],
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
            title: '事件分析队列',
            icon: Icons.sync_outlined,
            child: _buildAnalysisJobStatsContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '知识消化',
            icon: Icons.auto_stories_outlined,
            child: _buildDigestStatsContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '每日 Token 使用',
            icon: Icons.analytics_outlined,
            child: _buildTokenUsageContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '个人规则库',
            icon: Icons.rule_outlined,
            child: _buildRulesContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildRulesContent() {
    if (_rulesLoading) {
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
    // 读取失败时不能落进下面的「还没有规则」引导——那会把故障说成用户没沉淀过经验。
    final rulesError = _buildLoadErrorHint(_secRules);
    if (rulesError != null) return rulesError;
    if (_rules.isEmpty) {
      return Text(
        '还没有规则。在对话里分享踩坑或心得时，AI 会建议把其中的经验沉淀成规则，你回复「好」确认后就会出现在这里，以后遇到类似情况 AI 会主动引用并提醒你。',
        style: TextStyle(
          color: AppTheme.textSecondary,
          fontSize: 13,
          height: 1.6,
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < _rules.length; i++) ...[
          if (i > 0) Divider(height: 1, color: AppTheme.surface3),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Padding(
                padding: EdgeInsets.only(top: 14),
                child: Icon(Icons.check_circle_outline, size: 16),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Text(
                    _rules[i].content,
                    style: TextStyle(
                      color: AppTheme.textPrimary,
                      fontSize: 13,
                      height: 1.5,
                    ),
                  ),
                ),
              ),
              IconButton(
                icon: const Icon(Icons.delete_outline, size: 18),
                color: AppTheme.textTertiary,
                tooltip: '删除规则',
                onPressed: () => _deleteRule(_rules[i]),
              ),
            ],
          ),
        ],
      ],
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
    // 读取失败时不能落进 `_buildEmptyProviders()`（那里写着「尚未配置」）——
    // 那会让「配置读不出来」显示成「用户没配」，与首次运行引导横幅的判断相矛盾。
    final providersError = _providersLoading
        ? null
        : _buildLoadErrorHint(_secProviders);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                '可配置多个 AI Provider，仅一个处于激活状态，对话使用激活项。',
                style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
              ),
            ),
            FilledButton.icon(
              onPressed: () => _openProviderEditor(null),
              icon: const Icon(Icons.add, size: 18),
              label: const Text('添加配置'),
              style: FilledButton.styleFrom(
                backgroundColor: AppTheme.accentPrimary,
                foregroundColor: Colors.white,
              ),
            ),
          ],
        ),
        const SizedBox(height: 16),
        if (_providersLoading)
          const Padding(
            padding: EdgeInsets.all(24),
            child: Center(child: CircularProgressIndicator()),
          )
        else if (providersError != null)
          providersError
        else if (_providers.isEmpty)
          _buildEmptyProviders()
        else
          ..._providers.map(_buildProviderCard),
      ],
    );
  }

  Widget _buildEmptyProviders() {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(24),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Column(
        children: [
          Icon(Icons.cloud_off_rounded, color: AppTheme.textTertiary, size: 28),
          const SizedBox(height: 8),
          Text(
            '尚未配置任何 AI Provider，点击右上角「添加配置」开始。',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          ),
        ],
      ),
    );
  }

  String _providerTypeLabel(String type) {
    switch (type) {
      case 'openai-compatible':
        return 'OpenAI 兼容';
      case 'ollama':
        return 'Ollama';
      default:
        return type;
    }
  }

  Widget _buildProviderCard(api.AiProviderConfigDto p) {
    return Container(
      margin: const EdgeInsets.only(bottom: AppTheme.space3),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: p.isActive
            ? AppTheme.accentPrimary.withValues(alpha: 0.08)
            : AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(
          color: p.isActive
              ? AppTheme.accentPrimary.withValues(alpha: 0.5)
              : AppTheme.surface3,
        ),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        p.name,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          color: AppTheme.textPrimary,
                          fontSize: 14,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    if (p.isActive) ...[
                      const SizedBox(width: 8),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 2,
                        ),
                        decoration: BoxDecoration(
                          color: AppTheme.accentPrimary.withValues(alpha: 0.2),
                          borderRadius: BorderRadius.circular(999),
                        ),
                        child: Text(
                          '激活中',
                          style: TextStyle(
                            color: AppTheme.accentPrimary,
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 4),
                Text(
                  '${_providerTypeLabel(p.providerType)} · ${p.model}',
                  style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
                ),
                const SizedBox(height: 2),
                Text(
                  p.baseUrl,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
                ),
              ],
            ),
          ),
          if (!p.isActive)
            TextButton(
              onPressed: () => _activateProvider(p),
              child: const Text('设为激活'),
            ),
          IconButton(
            tooltip: '编辑',
            icon: const Icon(Icons.edit_outlined, size: 18),
            color: AppTheme.textSecondary,
            onPressed: () => _openProviderEditor(p),
          ),
          IconButton(
            tooltip: '删除',
            icon: const Icon(Icons.delete_outline, size: 18),
            color: AppTheme.error,
            onPressed: () => _deleteProvider(p),
          ),
        ],
      ),
    );
  }

  Future<void> _activateProvider(api.AiProviderConfigDto p) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setActiveAiProviderConfig(p.id);
      await _loadProviders();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('切换激活失败：$e')));
    }
  }

  Future<void> _deleteProvider(api.AiProviderConfigDto p) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('删除配置'),
        content: Text(
          '确定删除 Provider「${p.name}」吗？'
          '${p.isActive ? '该配置正处于激活状态，删除后剩余第一个配置将自动激活。' : ''}',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('取消'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: AppTheme.error),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('删除'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteAiProviderConfig(p.id);
      await _loadProviders();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('删除失败：$e')));
    }
  }

  Future<void> _openProviderEditor(api.AiProviderConfigDto? existing) async {
    final isNew = existing == null;
    final nameCtrl = TextEditingController(text: existing?.name ?? '');
    final baseUrlCtrl = TextEditingController(text: existing?.baseUrl ?? '');
    final modelCtrl = TextEditingController(text: existing?.model ?? '');
    // 密钥不回填明文：留空即保留原 key（新建则必填）
    final apiKeyCtrl = TextEditingController(text: '');
    final temperatureCtrl = TextEditingController(
      text: (existing?.temperature ?? 0.7).toString(),
    );
    final maxTokensCtrl = TextEditingController(
      text: existing?.maxTokens?.toString() ?? '',
    );
    var providerType = existing?.providerType ?? 'openai-compatible';

    final saved = await showDialog<_ProviderDraft>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) => AlertDialog(
          title: Text(isNew ? '添加 AI Provider' : '编辑 Provider'),
          content: SingleChildScrollView(
            child: SizedBox(
              width: 440,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _buildTextField(
                    label: '配置名称 *',
                    hint: '如：主用 GPT',
                    controller: nameCtrl,
                  ),
                  const SizedBox(height: 12),
                  _buildDropdownField<String>(
                    label: '提供商类型',
                    value: providerType,
                    items: const [
                      DropdownMenuItem(
                        value: 'openai-compatible',
                        child: Text('OpenAI 兼容'),
                      ),
                      DropdownMenuItem(value: 'ollama', child: Text('Ollama')),
                    ],
                    onChanged: (value) {
                      if (value != null) {
                        setDialogState(() => providerType = value);
                      }
                    },
                  ),
                  const SizedBox(height: 12),
                  _buildTextField(
                    label: 'Base URL *',
                    hint: 'https://api.openai.com/v1',
                    controller: baseUrlCtrl,
                  ),
                  const SizedBox(height: 12),
                  _buildTextField(
                    label: '模型 *',
                    hint: 'gpt-4o',
                    controller: modelCtrl,
                  ),
                  const SizedBox(height: 12),
                  _buildTextField(
                    label: isNew ? 'API Key *' : 'API Key',
                    hint: isNew ? '输入你的 API Key' : '留空表示保持不变',
                    obscure: true,
                    controller: apiKeyCtrl,
                  ),
                  const SizedBox(height: 12),
                  Row(
                    children: [
                      Expanded(
                        child: _buildTextField(
                          label: 'Temperature',
                          hint: '0.7',
                          keyboardType: TextInputType.number,
                          controller: temperatureCtrl,
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: _buildTextField(
                          label: 'Max Tokens',
                          hint: '可选',
                          keyboardType: TextInputType.number,
                          controller: maxTokensCtrl,
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(
                ctx,
                _ProviderDraft(
                  name: nameCtrl.text.trim(),
                  providerType: providerType,
                  baseUrl: baseUrlCtrl.text.trim(),
                  model: modelCtrl.text.trim(),
                  apiKey: apiKeyCtrl.text.trim(),
                  temperature: double.tryParse(temperatureCtrl.text) ?? 0.7,
                  maxTokens: int.tryParse(maxTokensCtrl.text),
                ),
              ),
              child: const Text('保存'),
            ),
          ],
        ),
      ),
    );
    // 对话框关闭后释放全部输入控制器（草稿值已在 pop 时读取完毕）。
    for (final controller in [
      nameCtrl,
      baseUrlCtrl,
      modelCtrl,
      apiKeyCtrl,
      temperatureCtrl,
      maxTokensCtrl,
    ]) {
      controller.dispose();
    }
    if (saved == null || !mounted) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.saveAiProviderConfig(
        api.AiProviderConfigDto(
          id: existing?.id ?? '',
          name: saved.name,
          providerType: saved.providerType,
          baseUrl: saved.baseUrl,
          model: saved.model,
          apiKeySource: existing?.apiKeySource ?? '',
          apiKey: saved.apiKey,
          isActive: existing?.isActive ?? false,
          temperature: saved.temperature,
          maxTokens: saved.maxTokens,
        ),
      );
      await _loadProviders();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('保存失败：$e')));
    }
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
            DropdownMenuItem(
              value: 'sliding-window',
              child: Text('滑动窗口 (Token 限制)'),
            ),
          ],
          onChanged: (value) {
            if (value != null) {
              ref
                  .read(settingsProvider.notifier)
                  .updateMemory(settings.memory.copyWith(strategyType: value));
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
            controller: _maxTokensController,
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
              ref
                  .read(settingsProvider.notifier)
                  .updateStorage(settings.storage.copyWith(adapterType: value));
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
    // 读取失败时不能落进「暂无 AI 调用记录」——那会把故障说成从没调用过 AI。
    final usageError = _buildLoadErrorHint(_secTokenUsage);
    if (usageError != null) return usageError;
    if (_dailyUsage.isEmpty) {
      return Text(
        '暂无 AI 调用记录。发起对话并生成 AI 回复后，这里会按天统计 token 用量。',
        style: TextStyle(
          color: AppTheme.textSecondary,
          fontSize: 13,
          height: 1.6,
        ),
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
              child: _buildUsageMetric(
                label: '近 7 天 Total',
                value: _fmtInt(totalTokens),
              ),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: _buildUsageMetric(
                label: '累计调用',
                value: '${_fmtInt(totalCalls)} 次',
              ),
            ),
          ],
        ),
        const SizedBox(height: 20),
        Row(
          children: [
            SizedBox(
              width: 96,
              child: Text(
                '日期',
                style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
              ),
            ),
            Expanded(
              child: Text(
                'Total',
                style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
              ),
            ),
            Expanded(
              child: Text(
                '调用',
                textAlign: TextAlign.right,
                style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
              ),
            ),
          ],
        ),
        ..._dailyUsage
            .take(7)
            .map(
              (d) => Padding(
                padding: const EdgeInsets.symmetric(vertical: 6),
                child: Row(
                  children: [
                    SizedBox(
                      width: 96,
                      child: Text(
                        d.date == today
                            ? '今天（${d.date.substring(5)}）'
                            : d.date.substring(5),
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
                        style: TextStyle(
                          fontSize: 13,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        '${_fmtInt(d.callCount)} 次',
                        textAlign: TextAlign.right,
                        style: TextStyle(
                          fontSize: 13,
                          color: AppTheme.textSecondary,
                        ),
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
        // 这块没有 loading 态，字段值默认 'fxtwitter'：读取失败时若不提示，
        // 用户会以为库里就是这个值，而实际是读失败了。
        if (_buildLoadErrorHint(_secTweetService) case final tweetError?) ...[
          tweetError,
          const SizedBox(height: 12),
        ],
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
              final mode = AppThemeMode.values.firstWhere(
                (m) => m.name == value,
              );
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
        const SizedBox(height: 16),
        _buildFontField(settings, notifier),
        const SizedBox(height: 16),
        _buildFontSizeField(settings, notifier),
      ],
    );
  }

  /// 正文字号（仅知识库正文）：拖拽实时预览，松手才持久化
  Widget _buildFontSizeField(AppSettings settings, SettingsNotifier notifier) {
    final size = settings.fontSize;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              '正文字号',
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: AppTheme.textSecondary,
              ),
            ),
            const Spacer(),
            Text(
              '${size.toStringAsFixed(size == size.roundToDouble() ? 0 : 1)} pt',
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textTertiary,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
        const SizedBox(height: 4),
        Slider(
          value: size,
          min: AppFonts.minFontSize,
          max: AppFonts.maxFontSize,
          divisions: (AppFonts.maxFontSize - AppFonts.minFontSize).round(),
          label: size.toStringAsFixed(0),
          onChanged: (v) => notifier.updateFontSize(v),
          onChangeEnd: (v) => notifier
            ..updateFontSize(v)
            ..saveTheme(),
        ),
      ],
    );
  }

  /// 内容字体选择：主按钮打开自研选择框（系统默认 / 本地字体，
  /// 每行自身字体预览）；「系统默认」chip 单独提供。存值格式见 [parseStoredFont]。
  /// 界面文案字体固定，不在此设置（见 [AppFonts.uiFont]）。
  Widget _buildFontField(AppSettings settings, SettingsNotifier notifier) {
    final fontName = settings.fontName;
    final isSystem = fontName == AppFonts.system;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '内容字体',
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          '作用于知识库正文、对话消息等内容区；界面文字使用固定字体',
          style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: OutlinedButton(
                onPressed: () => _showFontPicker(notifier, fontName),
                style: OutlinedButton.styleFrom(
                  alignment: Alignment.centerLeft,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 14,
                  ),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: FontNameLabel(
                        stored: fontName,
                        fallbackStyle: null,
                      ),
                    ),
                    Icon(
                      Icons.font_download_outlined,
                      size: 18,
                      color: AppTheme.textSecondary,
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(width: 8),
            ChoiceChip(
              label: const Text(AppFonts.systemDisplayName),
              selected: isSystem,
              onSelected: (selected) {
                if (selected) {
                  notifier
                    ..updateFontName(AppFonts.system)
                    ..saveTheme();
                }
              },
            ),
          ],
        ),
      ],
    );
  }

  /// 打开字体选择对话框：选中后先预热本地字体再落库，避免主题闪默认字体。
  Future<void> _showFontPicker(
    SettingsNotifier notifier,
    String current,
  ) async {
    final picked = await showFontPickerDialog(context, current: current);
    if (picked == null || !mounted) return;
    await notifier.setGlobalFont(picked);
    await notifier.saveTheme();
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
              borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
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
              borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
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

      // AI Provider 为多配置列表，单独即时保存，不在此处处理
      ref
          .read(settingsProvider.notifier)
          .updateMemory(
            currentSettings.memory.copyWith(
              maxMessages: int.tryParse(_maxMessagesController.text),
              maxTokens: int.tryParse(_maxTokensController.text),
            ),
          );

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
    }
  }
}

/// AI Provider 编辑对话框的草稿数据
class _ProviderDraft {
  final String name;
  final String providerType;
  final String baseUrl;
  final String model;
  final String apiKey;
  final double temperature;
  final int? maxTokens;

  _ProviderDraft({
    required this.name,
    required this.providerType,
    required this.baseUrl,
    required this.model,
    required this.apiKey,
    required this.temperature,
    required this.maxTokens,
  });
}
