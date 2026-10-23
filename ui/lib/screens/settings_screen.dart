import 'dart:io' show Platform, Process;
import 'package:file_picker/file_picker.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../providers/app_provider.dart';
import '../providers/settings_provider.dart';
import '../models/settings.dart';
import '../models/token_usage.dart';
import '../models/relation.dart';
import '../models/rule.dart';
import '../bridge/rust_bridge_repository.dart';
import '../bridge/api.dart' as api;
import '../widgets/chat_proposal_review.dart';
import '../widgets/custom_title_bar.dart';
import '../widgets/font_picker_dialog.dart';
import '../widgets/habit_nudge_settings.dart';
import '../widgets/provider_endpoint_notice.dart';
import '../widgets/knowledge_digest_dialog.dart';
import '../widgets/web_search_settings.dart';
import '../widgets/doh_settings.dart';
import '../widgets/proxy_settings.dart';
import '../utils/app_lock.dart';
import '../utils/error_report.dart';
import '../utils/system_fonts.dart';
import '../utils/window_service.dart';
import '../theme/app_theme.dart';
import '../app_update.dart';
// ── 大类方法按功能域拆分为 part extension（行为零变化，见 settings_parts/）──
part 'settings_parts/data_rules_loads.dart';
part 'settings_parts/rules_prefs.dart';
part 'settings_parts/tabs_model.dart';
part 'settings_parts/integrations.dart';
part 'settings_parts/skills.dart';
part 'settings_parts/habits.dart';
part 'settings_parts/providers.dart';
part 'settings_parts/usage_fonts.dart';
part 'settings_parts/provider_editor_dialog.dart';
part 'settings_parts/security.dart';
// ── 分区标题常量（原 static const 平移为 library 级，供全部 part 使用）──
const _secProviders = 'AI Provider 配置';
const _secTokenUsage = '每日 Token 用量';
const _secAnalysisJobs = '事件分析队列';
const _secDigest = '知识消化';
const _secRules = '规则库';
const _secTweetService = '推文抓取服务';
class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});
  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}
class _SettingsScreenState extends ConsumerState<SettingsScreen>
    with SingleTickerProviderStateMixin {
  final _formKey = GlobalKey<FormState>();
  late final TabController _tabController;
  /// 应用版本（pubspec version），宽屏在左 tab 栏底部、窄屏在 tab 条下方。
  String _version = '';
  /// 「检查更新」在途标记。runFromSettings 自身弹窗闭环，本页只防重入。
  bool _checkingUpdate = false;
  late TextEditingController _maxMessagesController;
  late TextEditingController _maxTokensController;
  List<DailyTokenUsage> _dailyUsage = [];
  bool _usageLoading = true;
  api.AnalysisJobStatsDto? _analysisJobStats;
  bool _analysisJobStatsLoading = true;
  api.KnowledgeDigestStatsDto? _digestStats;
  bool _digestStatsLoading = true;
  bool _exportingWiki = false;
  api.DatabaseStatsDto? _dbStats;
  String? _dbStatsError;
  List<api.UserSkillDto>? _userSkills;
  String? _userSkillsError;
  List<api.McpServerDto>? _mcpServers;
  String? _mcpError;
  bool _installingSkill = false;
  bool _vacuuming = false;
  // 打开密码（安全 tab）：服务 + 状态。哈希记录存系统安全存储，不落库。
  final AppLockService _appLockService = AppLockService();
  bool _appLockEnabled = false;
  bool _appLockLoading = true;
  // 指纹/面容解锁（仅移动端）：设备能力 + 用户开关。
  bool _biometricAvailable = false;
  bool _biometricEnabled = false;
  // 个人经验规则库
  List<Rule> _rules = [];
  bool _rulesLoading = true;
  // 规则触发提醒（决策辅助 D1)：偏好模式 + 最近提醒列表
  String _ruleTriggerMode = 'instant';
  List<RuleHit> _recentRuleHits = [];
  // 对话影子校验命中（迁移 54）：聊天轮预筛 → 后台 LLM 判定，只读审查面
  List<api.ChatRuleHitDto> _recentChatRuleHits = [];
  /// 情感回访偏好（D6）：开关 + 频率上限（天）。加载失败保持默认且不阻塞。
  bool _companionRevisitEnabled = true;
  int _companionIntervalDays = 7;
  bool _companionPrefsLoading = true;
  // 用户模型面板（决策辅助 D2 + D5 全维度收口）：关系边 + 情绪标注。
  // 其余维度（承诺/复盘/节奏/健康/财务/地点/身份/创作线）按 D0 映射
  // 以知识库页型承载，在知识库中查看/编辑（面板只给入口提示）。
  List<Relation> _userModelRelations = [];
  List<api.EventEmotionDto> _userModelEmotions = [];
  bool _userModelLoading = true;
  // AI provider 多配置（仅一个激活）
  List<api.AiProviderConfigDto> _providers = [];
  bool _providersLoading = true;
  // 推文抓取服务（当前仅支持 fxtwitter）
  String _tweetService = 'fxtwitter';
  /// 桌面「关闭窗口时」偏好（本地 desktop.json）。
  CloseAction _closeAction = CloseAction.minimizeToTray;
  // 启动期各区块读取失败的原因（区块名 → 异常文本）。
  //
  // 空列表区分不了「本来就没有」和「读取失败」：后者原本被 catch 掉后只 debugPrint，
  // 于是渲染成「还没有规则」「未配置」这类**正常空状态**，把故障说成了没数据。
  // 这里单独记下原因，空状态分支先判它，失败与真空才分得开。
  final Map<String, String> _loadErrors = {};
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
    _tabController = TabController(length: 11, vsync: this);
    _loadVersion();
    final settings = ref.read(settingsProvider);
    // 桌面窗口偏好：WindowService 为单例，桌面入口已注册实现；移动端读 stub 默认值。
    _closeAction = ref.read(windowServiceProvider).closeAction;
    _maxMessagesController = TextEditingController(
      text: settings.memory.maxMessages?.toString() ?? '20',
    );
    _maxTokensController = TextEditingController(
      text: settings.memory.maxTokens?.toString() ?? '',
    );
    // 从 Rust 侧读取各区块数据（AI provider 多配置 / token 用量 / 推文服务 / 规则库）
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _loadProviders();
      _loadTokenUsage();
      _loadAnalysisJobStats();
      _loadDigestStats();
      _loadTweetService();
      _loadRules();
      _loadDbStats();
      _loadUserSkills();
      _loadMcpServers();
      _loadAppLockState();
    });
  }
  Future<void> _loadVersion() async {
    // pubspec 版本从没动过（恒 1.0.0）没区分度；报障要认的是构建 hash。
    // 两者都取：版本号兜底标识，hash 精确到这次编译。
    var build = '';
    try {
      build = await api.buildHash();
    } catch (_) {}
    try {
      final info = await PackageInfo.fromPlatform();
      final label = build.isEmpty
          ? 'elsewhen v${info.version}'
          : 'elsewhen v${info.version} · $build';
      if (mounted) setState(() => _version = label);
    } catch (_) {
      // 版本读取失败不影响设置页；留空即可。
      if (mounted && build.isNotEmpty) {
        setState(() => _version = 'elsewhen · $build');
      }
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
    // 移动端：标题栏补状态栏安全区，返回键放左侧（app 惯例）；
    // 桌面端：返回键在右侧 actions（窗口 chrome 风格）。
    final isMobile = !kIsWeb && (Platform.isAndroid || Platform.isIOS);
    final topInset = isMobile ? MediaQuery.viewPaddingOf(context).top : 0.0;
    return Scaffold(
      body: Column(
        children: [
          CustomTitleBar(
            title: '设置',
            topInset: topInset,
            leading: isMobile
                ? IconButton(
                    icon: const Icon(Icons.arrow_back, size: 20),
                    tooltip: '返回主界面',
                    color: AppTheme.textSecondary,
                    onPressed: () => Navigator.of(context).pop(),
                  )
                : null,
            actions: isMobile
                ? null
                : [
                    IconButton(
                      icon: const Icon(Icons.arrow_back, size: 18),
                      tooltip: '返回主界面',
                      color: const Color(0xFF9BA1AB),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                    // 原 titlebar 的全局「保存」已删：整页除「记忆策略」两个数字字段外
                    // 都是即时保存，全局按钮名不副实。那两个字段的保存挪进记忆卡片。
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
                // top: false —— 状态栏安全区已由 CustomTitleBar 的 topInset 处理，
                // 这里只保留底部手势条/刘海的安全区。
                top: false,
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
                          _buildIntegrationTab(),
                          _buildNetworkTab(),
                          _buildSkillsTab(),
                          _buildMcpTab(),
                          _buildKnowledgeTab(),
                          _buildHabitsTab(),
                          _buildDataTab(),
                          _buildRulesTab(),
                          _buildSecurityTab(),
                        ],
                      );
                      if (compact) {
                        // 版本放页面最底部：顶部 tab 区挤进小字像噪点，底部
                        // 是报障时顺手下滑就能看到的常规位置。
                        return Column(
                          children: [
                            _buildCompactTabBar(),
                            Expanded(child: content),
                            if (_version.isNotEmpty)
                              Padding(
                                padding: const EdgeInsets.only(bottom: 2),
                                child: Row(
                                  children: [
                                    Expanded(
                                      child: Text(
                                        _version,
                                        style: TextStyle(
                                          fontSize: 10,
                                          color: AppTheme.textTertiary,
                                        ),
                                      ),
                                    ),
                                    GestureDetector(
                                      onTap: _checkingUpdate
                                          ? null
                                          : () => AppUpdate.runFromSettings(
                                              context,
                                            ),
                                      child: Text(
                                        _checkingUpdate ? '检查中…' : '检查更新',
                                        style: TextStyle(
                                          fontSize: 10,
                                          color: AppTheme.accentPrimary,
                                        ),
                                      ),
                                    ),
                                    const SizedBox(width: 16),
                                  ],
                                ),
                              ),
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
          content: Text('记忆策略已保存'),
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
