import 'dart:io' show Platform, Process;

import 'package:file_picker/file_picker.dart';
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
import '../widgets/provider_endpoint_notice.dart';
import '../widgets/knowledge_digest_dialog.dart';
import '../widgets/web_search_settings.dart';
import '../widgets/doh_settings.dart';
import '../widgets/proxy_settings.dart';
import '../utils/error_report.dart';
import '../utils/system_fonts.dart';
import '../utils/window_service.dart';
import '../theme/app_theme.dart';
import '../app_update.dart';

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

  // 个人经验规则库
  List<Rule> _rules = [];
  bool _rulesLoading = true;
  // 规则触发提醒（决策辅助 D1)：偏好模式 + 最近提醒列表
  String _ruleTriggerMode = 'instant';
  List<RuleHit> _recentRuleHits = [];

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
    _tabController = TabController(length: 9, vsync: this);
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
      final daily = await repo.getDailyTokenUsage(30);
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

  Future<void> _loadRules() => _loadRulesInternal();

  /// 用户模型（D2）：模型面板只读列全部关系边（最近被强化者在前）。
  /// 来源事件跳转待事件详情视图存在后接入（如实遗留，见 roadmap D2）。
  Future<void> _loadUserModel() async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final relations = await repo.listRelations();
      // 情绪标注读取失败不拖垮关系边展示（D5：陪伴面拉取独立降级）。
      List<api.EventEmotionDto> emotions;
      try {
        emotions = await repo.listEventEmotions();
      } catch (e) {
        debugPrint('loadEventEmotions failed: $e');
        emotions = const [];
      }
      if (!mounted) return;
      setState(() {
        _userModelRelations = relations;
        _userModelEmotions = emotions;
        _userModelLoading = false;
      });
    } catch (e) {
      debugPrint('loadUserModel failed: $e');
      if (!mounted) return;
      setState(() => _userModelLoading = false);
    }
  }

  Future<void> _deleteEmotion(api.EventEmotionDto item) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteEventEmotion(
        id: item.id,
        target: item.target,
        feeling: item.feeling,
      );
      if (!mounted) return;
      setState(() => _userModelEmotions.remove(item));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('删除情绪标注', e))));
    }
  }

  Future<void> _deleteRelation(Relation rel) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteRelation(rel.id);
      if (!mounted) return;
      setState(() => _userModelRelations.removeWhere((r) => r.id == rel.id));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('删除关系', e))));
    }
  }

  /// 置信条（1..5）：点串联，视觉锚定「来源多样才强化」的语义。
  Widget _buildConfidenceDots(int confidence) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 1; i <= 5; i++)
          Container(
            width: 6,
            height: 6,
            margin: const EdgeInsets.only(left: 2),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: i <= confidence
                  ? AppTheme.accentPrimary
                  : AppTheme.surface3,
            ),
          ),
      ],
    );
  }

  Widget _buildUserModelContent() {
    if (_userModelLoading) {
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
    if (_userModelRelations.isEmpty && _userModelEmotions.isEmpty) {
      return Text(
        '还没有积累。对话或快录里提到的人物与项目，经你确认（或 @/# 标注的权威组合）后会在这里长成一份可查可删的关系账本；有客观情绪反应的记录会给出简短文字标注（无数值、不是情绪分析报告）。删掉任何一条都不会动原始记录。',
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
        for (var i = 0; i < _userModelRelations.length; i++) ...[
          if (i > 0) Divider(height: 1, color: AppTheme.surface3),
          Row(
            children: [
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 10),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '${_userModelRelations[i].fromSlug}  ——  ${_userModelRelations[i].relation}  ——  ${_userModelRelations[i].toSlug}',
                        style: TextStyle(
                          color: AppTheme.textPrimary,
                          fontSize: 13,
                          height: 1.4,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Row(
                        children: [
                          _buildConfidenceDots(
                            _userModelRelations[i].confidence,
                          ),
                          const SizedBox(width: 8),
                          Text(
                            '最近 ${DateFormat('MM-dd HH:mm').format(_userModelRelations[i].lastSeenAt)}',
                            style: TextStyle(
                              color: AppTheme.textTertiary,
                              fontSize: 11,
                            ),
                          ),
                        ],
                      ),
                      if (_userModelRelations[i].note?.isNotEmpty ?? false)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            _userModelRelations[i].note!,
                            style: TextStyle(
                              color: AppTheme.textSecondary,
                              fontSize: 12,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              IconButton(
                icon: const Icon(Icons.delete_outline, size: 18),
                color: AppTheme.textTertiary,
                tooltip: '删除这条关系（原始记录不动）',
                onPressed: () => _deleteRelation(_userModelRelations[i]),
              ),
            ],
          ),
        ],
        // ── 情绪标注（D5）：文本无数值，跨维度独立，用户可删 ──
        if (_userModelEmotions.isNotEmpty) ...[
          const SizedBox(height: 12),
          Divider(height: 1, color: AppTheme.surface3),
          const SizedBox(height: 8),
          Text(
            '情绪标注',
            style: TextStyle(
              color: AppTheme.textTertiary,
              fontSize: 12,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: 4),
          for (final item in _userModelEmotions)
            Row(
              children: [
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          item.target == null || item.target!.isEmpty
                              ? '对这件事'
                              : '对 ${item.target}',
                          style: TextStyle(
                            color: AppTheme.textPrimary,
                            fontSize: 13,
                            height: 1.4,
                          ),
                        ),
                        Text(
                          '${item.feeling} · ${DateFormat('MM-dd HH:mm').format(DateTime.tryParse(item.recordedAt)?.toLocal() ?? DateTime.now())}',
                          style: TextStyle(
                            color: AppTheme.textSecondary,
                            fontSize: 12,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.delete_outline, size: 18),
                  color: AppTheme.textTertiary,
                  tooltip: '删除这条情绪标注（原始记录不动）',
                  onPressed: () => _deleteEmotion(item),
                ),
              ],
            ),
        ],
        // ── 维度全集收口（D5）：其余维度按 D0 映射以知识库页承载 ──
        const SizedBox(height: 12),
        Text(
          '其余维度（承诺、复盘、节奏、偏好红线、健康、财务、地点、身份、创作线）以知识库页面承载，在知识库中查看与编辑。',
          style: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 11,
            height: 1.5,
          ),
        ),
      ],
    );
  }

  Future<void> _loadRulesInternal() async {
    // 提醒偏好与最近提醒独立于规则列表：读失败不拖垮规则库展示。
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final mode = await repo.getRuleTriggerMode();
      final hits = await repo.listRecentRuleHits(limit: 8);
      if (!mounted) return;
      setState(() {
        _ruleTriggerMode = mode;
        _recentRuleHits = hits;
      });
    } catch (e) {
      debugPrint('loadRuleTrigger failed: $e');
    }
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final prefs = await repo.getCompanionPrefs();
      if (!mounted) return;
      setState(() {
        _companionRevisitEnabled = prefs.revisitEnabled;
        _companionIntervalDays = prefs.revisitIntervalDays;
        _companionPrefsLoading = false;
      });
    } catch (e) {
      debugPrint('loadCompanionPrefs failed: $e');
      if (!mounted) return;
      setState(() => _companionPrefsLoading = false);
    }
    await _loadUserModel();
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

  /// 提醒偏好三态。默认 instant：预筛命中的规则经 LLM 判定后，高置信即时
  /// 提醒（事件旁可见）、其余进每日回顾聚合。review_only 全部聚合；off 彻底
  /// 关闭——预筛根本不跑。设计红线：分级防打扰，任何「默认全开」以失败论，
  /// 因此即时提醒只给高置信命中，且用户随时可整体关闭。
  Widget _buildRuleTriggerPrefs() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '事件提醒',
          style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
        ),
        const SizedBox(height: AppTheme.space2),
        SegmentedButton<String>(
          segments: const [
            ButtonSegment(value: 'instant', label: Text('即时+回顾')),
            ButtonSegment(value: 'review_only', label: Text('仅回顾')),
            ButtonSegment(value: 'off', label: Text('关闭')),
          ],
          selected: {_ruleTriggerMode},
          onSelectionChanged: (selection) async {
            final mode = selection.first;
            try {
              final repo =
                  ref.read(storageRepositoryProvider) as RustBridgeRepository;
              await repo.setRuleTriggerMode(mode);
              if (!mounted) return;
              setState(() => _ruleTriggerMode = mode);
            } catch (e) {
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text(reportUiError('保存提醒偏好', e))),
              );
            }
          },
        ),
      ],
    );
  }

  /// 情感回访（D6）：低打扰回引「被记住的事」，只在打开应用时可能出现，
  /// 绝不做系统级推送。频率上限可配；关闭后检查路径零查询（零侵入）。
  Widget _buildCompanionRevisitPrefs() {
    return Row(
      children: [
        Text(
          '情感回访',
          style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
        ),
        const Spacer(),
        if (!_companionPrefsLoading)
          DropdownButton<int>(
            value: _companionIntervalDays,
            underline: const SizedBox.shrink(),
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
            items: const [
              DropdownMenuItem(value: 3, child: Text('3天')),
              DropdownMenuItem(value: 7, child: Text('7天')),
              DropdownMenuItem(value: 14, child: Text('14天')),
              DropdownMenuItem(value: 30, child: Text('30天')),
            ],
            onChanged: (days) async {
              if (days == null) return;
              await _saveCompanionPrefs(_companionRevisitEnabled, days);
            },
          ),
        Switch(
          value: _companionRevisitEnabled,
          onChanged: _companionPrefsLoading
              ? null
              : (enabled) =>
                    _saveCompanionPrefs(enabled, _companionIntervalDays),
        ),
      ],
    );
  }

  Future<void> _saveCompanionPrefs(bool enabled, int intervalDays) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setCompanionPrefs(
        enabled: enabled,
        intervalDays: intervalDays,
      );
      if (!mounted) return;
      setState(() {
        _companionRevisitEnabled = enabled;
        _companionIntervalDays = intervalDays;
      });
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('保存回访偏好', e))));
    }
  }

  Future<void> _setHitFeedback(RuleHit hit, String feedback) async {
    final next = hit.feedback == feedback ? null : feedback;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setRuleHitFeedback(hit.id, next);
      // 回读真实状态（不让乐观更新说谎）
      final hits = await repo.listRecentRuleHits(limit: 8);
      if (!mounted) return;
      setState(() => _recentRuleHits = hits);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('记录提醒反馈', e))));
    }
  }

  /// 最近提醒（D6）：每条可标「有用 / 不相关」，再点一次即清除。
  /// 反馈只记录在本地命中行上，没有任何自动行为消费它。
  Widget _buildRecentRuleHits() {
    final helpfulCount = _recentRuleHits
        .where((h) => h.feedback == 'helpful')
        .length;
    final notRelevantCount = _recentRuleHits
        .where((h) => h.feedback == 'not_relevant')
        .length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              '最近提醒',
              style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
            ),
            const Spacer(),
            if (helpfulCount > 0 || notRelevantCount > 0)
              Text(
                '有用 $helpfulCount · 不相关 $notRelevantCount',
                style: TextStyle(color: AppTheme.textTertiary, fontSize: 11),
              ),
          ],
        ),
        const SizedBox(height: AppTheme.space2),
        for (final hit in _recentRuleHits)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 2),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  hit.isNotified
                      ? Icons.notifications_active_outlined
                      : Icons.inbox_outlined,
                  size: 14,
                  color: hit.isNotified
                      ? AppTheme.accentPrimary
                      : AppTheme.textTertiary,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    '${hit.ruleContent} · ${DateFormat('MM-dd HH:mm').format(hit.createdAt.toLocal())}',
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 12,
                      height: 1.4,
                    ),
                  ),
                ),
                SizedBox(
                  width: 30,
                  height: 26,
                  child: IconButton(
                    padding: EdgeInsets.zero,
                    iconSize: 14,
                    tooltip: '有用（再点一次清除）',
                    icon: Icon(
                      hit.feedback == 'helpful'
                          ? Icons.thumb_up
                          : Icons.thumb_up_outlined,
                    ),
                    color: hit.feedback == 'helpful'
                        ? AppTheme.accentPrimary
                        : AppTheme.textTertiary,
                    onPressed: () => _setHitFeedback(hit, 'helpful'),
                  ),
                ),
                const SizedBox(width: 2),
                SizedBox(
                  width: 30,
                  height: 26,
                  child: IconButton(
                    padding: EdgeInsets.zero,
                    iconSize: 14,
                    tooltip: '不相关（再点一次清除）',
                    icon: Icon(
                      hit.feedback == 'not_relevant'
                          ? Icons.thumb_down
                          : Icons.thumb_down_outlined,
                    ),
                    color: hit.feedback == 'not_relevant'
                        ? AppTheme.accentPrimary
                        : AppTheme.textTertiary,
                    onPressed: () => _setHitFeedback(hit, 'not_relevant'),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }

  Future<void> _editRuleTrigger(Rule rule) async {
    final keywordsController = TextEditingController(
      text: rule.triggerKeywords.join(', '),
    );
    var scene = rule.triggerScene;
    final saved = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: const Text('触发条件'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                rule.content,
                style: TextStyle(
                  color: AppTheme.textSecondary,
                  fontSize: 12,
                  height: 1.4,
                ),
              ),
              const SizedBox(height: AppTheme.space4),
              TextField(
                controller: keywordsController,
                decoration: const InputDecoration(
                  labelText: '触发关键词（逗号分隔，留空 = 不预筛）',
                  hintText: '如：X项目, deadline',
                ),
              ),
              const SizedBox(height: AppTheme.space4),
              DropdownButtonFormField<String>(
                initialValue: scene.isEmpty ? '' : scene,
                decoration: const InputDecoration(labelText: '触发场景'),
                items: const [
                  DropdownMenuItem(value: '', child: Text('全场景')),
                  DropdownMenuItem(value: 'capture', child: Text('快速捕获')),
                  DropdownMenuItem(value: 'conversation', child: Text('对话')),
                  DropdownMenuItem(value: 'daily_review', child: Text('每日回顾')),
                ],
                onChanged: (value) => setDialogState(() => scene = value ?? ''),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.of(dialogContext).pop(true),
              child: const Text('保存'),
            ),
          ],
        ),
      ),
    );
    if (saved != true) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final keywords = keywordsController.text
          .split(RegExp(r'[,，\n]'))
          .map((kw) => kw.trim())
          .where((kw) => kw.isNotEmpty)
          .toList();
      await repo.setRuleTrigger(
        rule.id,
        triggerScene: scene,
        triggerKeywords: keywords,
      );
      if (!mounted) return;
      await _loadRules();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('保存触发条件', e))));
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
          .showSnackBar(SnackBar(content: Text(reportUiError('删除规则', e))));
    }
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
                          _buildDataTab(),
                          _buildRulesTab(),
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
        builder: (context, _) => Column(
          children: [
            Expanded(
              child: ListView(
                children: [
                  _buildSettingsTabItem(0, '模型', Icons.smart_toy_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(1, '外观', Icons.palette_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(2, '集成', Icons.hub_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(3, '网络', Icons.dns_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(4, '技能', Icons.extension_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(5, 'MCP', Icons.usb_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(6, '知识', Icons.auto_stories_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(7, '数据', Icons.dataset_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(8, '规则', Icons.rule_outlined),
                ],
              ),
            ),
            if (_version.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(
                  top: AppTheme.space2,
                  left: AppTheme.space3,
                  right: AppTheme.space3,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      _version,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                    GestureDetector(
                      onTap: _checkingUpdate
                          ? null
                          : () => AppUpdate.runFromSettings(context),
                      child: Text(
                        _checkingUpdate ? '检查中…' : '检查更新',
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.accentPrimary,
                        ),
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

  Widget _buildCompactTabBar() {
    return Material(
      color: AppTheme.surface1,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TabBar(
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
              Tab(text: '集成', icon: Icon(Icons.hub_outlined, size: 17)),
              Tab(text: '网络', icon: Icon(Icons.dns_outlined, size: 17)),
              Tab(text: '技能', icon: Icon(Icons.extension_outlined, size: 17)),
              Tab(text: 'MCP', icon: Icon(Icons.usb_outlined, size: 17)),
              Tab(
                text: '知识',
                icon: Icon(Icons.auto_stories_outlined, size: 17),
              ),
              Tab(text: '数据', icon: Icon(Icons.dataset_outlined, size: 17)),
              Tab(text: '规则', icon: Icon(Icons.rule_outlined, size: 17)),
            ],
          ),
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
            trailing: FilledButton.icon(
              onPressed: () => _openProviderEditor(null),
              icon: const Icon(Icons.add, size: 18),
              label: const Text('添加'),
              style: FilledButton.styleFrom(
                backgroundColor: AppTheme.accentPrimary,
                foregroundColor: Colors.white,
              ),
            ),
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

  Widget _buildWikiExportContent() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '把知识页导出为带元数据的 Markdown 文件（Obsidian 等通用工具可直接打开）；'
          '派生产物与归档/拒绝页不导出，同名覆盖可重复执行。',
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 13,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        OutlinedButton.icon(
          onPressed: _exportingWiki ? null : _exportWikiMarkdown,
          icon: _exportingWiki
              ? const SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.ios_share_outlined, size: 18),
          label: Text(_exportingWiki ? '导出中…' : '选择目录并导出'),
        ),
      ],
    );
  }

  Future<void> _exportWikiMarkdown() async {
    final directory = await FilePicker.getDirectoryPath(dialogTitle: '选择导出目录');
    if (!mounted || directory == null || directory.trim().isEmpty) return;
    setState(() => _exportingWiki = true);
    try {
      final repository = ref.read(storageRepositoryProvider);
      if (repository is! RustBridgeRepository) {
        throw StateError('当前存储后端不支持导出');
      }
      final result = await repository.exportWikiMarkdown(outDir: directory);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('已导出 ${result.exported} 个知识页到 ${result.outDir}'),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('导出知识库', e))));
    } finally {
      if (mounted) setState(() => _exportingWiki = false);
    }
  }

  Widget _buildDbMaintenanceContent() {
    final stats = _dbStats;
    String mb(int bytes) => (bytes / 1024 / 1024).toStringAsFixed(1);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (stats == null)
          Text(
            _dbStatsError ?? '读取中…',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else ...[
          Text(
            '库文件 ${mb(stats.fileBytes)} MB，其中可回收 ${mb(stats.freeBytes)} MB'
            '（历史删除残留）${stats.freeBytes > 8 * 1024 * 1024 ? '。建议整理。' : ''}',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 13,
              height: 1.6,
            ),
          ),
          const SizedBox(height: AppTheme.space3),
          OutlinedButton.icon(
            onPressed: _vacuuming ? null : _vacuumDatabase,
            icon: _vacuuming
                ? const SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.cleaning_services_outlined, size: 18),
            label: Text(_vacuuming ? '整理中…' : '立即整理'),
          ),
        ],
      ],
    );
  }

  Future<void> _loadDbStats() async {
    final repository = ref.read(storageRepositoryProvider);
    if (repository is! RustBridgeRepository) return;
    try {
      final stats = await repository.databaseStats();
      if (mounted)
        setState(() {
          _dbStats = stats;
          _dbStatsError = null;
        });
    } catch (e) {
      if (mounted) setState(() => _dbStatsError = reportUiError('读取库体量', e));
    }
  }

  Future<void> _vacuumDatabase() async {
    setState(() => _vacuuming = true);
    try {
      final repository =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final result = await repository.vacuumDatabase();
      await _loadDbStats();
      if (!mounted) return;
      final saved = (result.beforeBytes - result.afterBytes) / 1024 / 1024;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            saved >= 0.1
                ? '整理完成，回收了 ${saved.toStringAsFixed(1)} MB'
                : '整理完成，库已经很紧凑',
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('整理数据库', e))));
    } finally {
      if (mounted) setState(() => _vacuuming = false);
    }
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
          if (_isDesktopPlatform) ...[
            const SizedBox(height: AppTheme.space6),
            _buildSection(
              title: '桌面窗口',
              icon: Icons.desktop_windows_outlined,
              child: _buildDesktopWindowSettings(),
            ),
          ],
        ],
      ),
    );
  }

  /// 是否桌面平台（窗口 chrome / 托盘 / 关闭语义只在这些平台存在）。
  static bool get _isDesktopPlatform =>
      Platform.isLinux || Platform.isMacOS || Platform.isWindows;

  /// 「关闭窗口时」设置：托盘可用才给选项，否则只说明「关闭即退出」。
  /// 二选一用 M3 `SegmentedButton`（而非下拉），选中态走强调色。
  Widget _buildDesktopWindowSettings() {
    final windowService = ref.read(windowServiceProvider);
    final trayAvailable = windowService.trayAvailable;
    if (!trayAvailable) {
      return Text(
        '当前环境没有可用的系统托盘，关闭窗口将直接退出应用。',
        style: TextStyle(
          fontSize: 12,
          color: AppTheme.textTertiary,
          height: 1.5,
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '关闭窗口时',
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 10),
        // 选项卡片而非 SegmentedButton：与卡片区块语言一致，
        // 且能各带一行描述（行为后果写在选项上，不再依赖区块尾部说明）。
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _buildCloseActionCard(
                action: CloseAction.minimizeToTray,
                selected: _closeAction == CloseAction.minimizeToTray,
                icon: Icons.arrow_downward_rounded,
                description: '应用留在后台托盘，可随时从托盘菜单唤回',
                onSelected: () => _applyCloseAction(CloseAction.minimizeToTray),
              ),
              const SizedBox(width: 10),
              _buildCloseActionCard(
                action: CloseAction.quit,
                selected: _closeAction == CloseAction.quit,
                icon: Icons.power_settings_new_rounded,
                description: '彻底退出应用，托盘图标一并移除',
                onSelected: () => _applyCloseAction(CloseAction.quit),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void _applyCloseAction(CloseAction action) {
    if (action == _closeAction) return;
    setState(() => _closeAction = action);
    ref.read(windowServiceProvider).setCloseAction(action);
  }

  Widget _buildCloseActionCard({
    required CloseAction action,
    required bool selected,
    required IconData icon,
    required String description,
    required VoidCallback onSelected,
  }) {
    return Expanded(
      child: Material(
        color: selected
            ? AppTheme.accentPrimary.withValues(alpha: 0.10)
            : AppTheme.surface2.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(12),
        child: InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: onSelected,
          child: Container(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              border: Border.all(
                color: selected
                    ? AppTheme.accentPrimary.withValues(alpha: 0.55)
                    : AppTheme.surface4,
              ),
            ),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(
                      icon,
                      size: 16,
                      color: selected
                          ? AppTheme.accentPrimary
                          : AppTheme.textTertiary,
                    ),
                    const SizedBox(width: 6),
                    Text(
                      action.displayName,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: selected
                            ? AppTheme.accentPrimary
                            : AppTheme.textSecondary,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                Text(
                  description,
                  style: TextStyle(
                    fontSize: 11.5,
                    color: AppTheme.textTertiary,
                    height: 1.45,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  /// 「服务」一拆三：集成（外部数据源）、网络（链路层）、技能（可编程扩展）。
  /// 原单 tab 四块，技能作为主打功能沉在底部不可见。
  Widget _buildIntegrationTab() {
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
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '外网搜索',
            icon: Icons.travel_explore_outlined,
            child: const WebSearchSettings(),
          ),
        ],
      ),
    );
  }

  Widget _buildNetworkTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '出站代理',
            icon: Icons.vpn_key_outlined,
            child: const ProxySettings(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: 'DoH 加密 DNS',
            icon: Icons.shield_outlined,
            child: const DohSettings(),
          ),
        ],
      ),
    );
  }

  Widget _buildSkillsTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '用户技能',
            icon: Icons.extension_outlined,
            child: _buildUserSkillsContent(),
          ),
        ],
      ),
    );
  }

  /// MCP 服务器：用户自定义的外部工具源（stdio 子进程，迁移 44）。
  Widget _buildMcpTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: 'MCP 服务器',
            icon: Icons.usb_outlined,
            child: _buildMcpContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildMcpContent() {
    final servers = _mcpServers;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '连接 Model Context Protocol 服务器，把外部工具带入对话。'
          '工具以 mcp__<服务器>__<工具名> 出现，模型按需调用；'
          'stdio 服务器以指定命令作为子进程启动（首期只支持 stdio 传输）。',
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 13,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space2),
        // 信任开关的风险说明：MCP server 是第三方代码，开启免审批后模型
        // 发起的每次工具调用都不再经人工确认，用户需自担风险。
        Text(
          '默认每次工具调用都会弹出审批卡逐次确认。把某个服务器标为「信任」后，'
          '它的调用将直接执行、不再审批——MCP 服务器是第三方代码，模型可能被'
          '诱导发起破坏性调用，开启即视为你了解并自担风险。'
          '若只想豁免确定只读的查询，可开每台服务器的「只读免审批」'
          '（仅对上游标了 readOnlyHint 的工具生效，其余照旧审批）。',
          style: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 12,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        OutlinedButton.icon(
          onPressed: () => _showMcpServerDialog(),
          icon: const Icon(Icons.add_outlined, size: 18),
          label: const Text('添加服务器'),
        ),
        const SizedBox(height: AppTheme.space3),
        if (servers == null)
          Text(
            _mcpError ?? '读取中…',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else if (servers.isEmpty)
          Text(
            '尚未添加任何 MCP 服务器',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else
          ...servers.map(
            (server) => Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Text(server.name, style: const TextStyle(fontSize: 14)),
                          const SizedBox(width: 6),
                          // 平台徽章：同步分发的适用范围（迁移 48）。
                          Text(
                            server.platform == 'all' ? '全部平台' : '仅 ${server.platform}',
                            style: TextStyle(
                              color: AppTheme.textTertiary,
                              fontSize: 11,
                            ),
                          ),
                        ],
                      ),
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text(
                          server.transport == 'streamable-http'
                              ? server.url
                              : '${server.command} ${server.argsJson}',
                          style: TextStyle(
                            color: AppTheme.textSecondary,
                            fontSize: 12,
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),
                IconButton(
                  tooltip: server.trusted
                      ? '信任（调用免审批，点击改为逐次审批）'
                      : '逐次审批（点击标为信任，调用免审批）',
                  icon: Icon(
                    server.trusted
                        ? Icons.verified_user_outlined
                        : Icons.shield_outlined,
                    size: 18,
                    color: server.trusted
                        ? AppTheme.accentPrimary
                        : AppTheme.textTertiary,
                  ),
                  onPressed: () => _toggleMcpTrusted(server.id, !server.trusted),
                ),
                // 只读免审批白名单（迁移 53）：比信任更窄的自审面——只豁免
                // 上游标了 readOnlyHint 的工具；不盲信上游标注，开关默认关。
                IconButton(
                  tooltip: server.allowReadonlyAuto
                      ? '只读免审批已开（仅豁免标了 readOnlyHint 的工具，点击关闭）'
                      : '只读免审批（点击后仅标了 readOnlyHint 的工具免逐次审批）',
                  icon: Icon(
                    server.allowReadonlyAuto
                        ? Icons.lock_open_outlined
                        : Icons.lock_outlined,
                    size: 18,
                    color: server.allowReadonlyAuto
                        ? AppTheme.accentPrimary
                        : AppTheme.textTertiary,
                  ),
                  onPressed: () =>
                      _toggleMcpAllowReadonlyAuto(server.id, !server.allowReadonlyAuto),
                ),
                IconButton(
                  tooltip: '编辑',
                  icon: const Icon(Icons.edit_outlined, size: 18),
                  onPressed: () => _showMcpServerDialog(existing: server),
                ),
                IconButton(
                  tooltip: '删除',
                  icon: const Icon(Icons.delete_outline, size: 18),
                  onPressed: () => _deleteMcpServer(server.id),
                ),
                Switch(
                  value: server.enabled,
                  onChanged: (value) => _toggleMcpServer(server.id, value),
                ),
              ],
            ),
          ),
      ],
    );
  }

  Future<void> _loadMcpServers() async {
    final repository = ref.read(storageRepositoryProvider);
    if (repository is! RustBridgeRepository) return;
    try {
      final servers = await repository.listMcpServers();
      if (mounted)
        setState(() {
          _mcpServers = servers;
          _mcpError = null;
        });
    } catch (e) {
      if (mounted) setState(() => _mcpError = reportUiError('读取 MCP 服务器', e));
    }
  }

  Future<void> _toggleMcpTrusted(String id, bool trusted) async {
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.setMcpServerTrusted(id: id, trusted: trusted);
      await _loadMcpServers();
      if (mounted && trusted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          const SnackBar(content: Text('已标为信任：该服务器的工具调用不再弹审批卡')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(reportUiError('切换信任状态', e))),
        );
      }
    }
  }

  Future<void> _toggleMcpAllowReadonlyAuto(String id, bool allow) async {
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.setMcpServerAllowReadonlyAuto(id: id, allow: allow);
      await _loadMcpServers();
      if (mounted && allow) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(const SnackBar(
          content: Text('已开只读免审批：仅标了 readOnlyHint 的工具不再弹审批卡；'
              '仍以上游标注为准，写类工具照旧逐次审批'),
        ));
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(reportUiError('切换只读免审批', e))),
        );
      }
    }
  }

  Future<void> _toggleMcpServer(String id, bool enabled) async {
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.setMcpServerEnabled(id: id, enabled: enabled);
      await _loadMcpServers();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(reportUiError('切换 MCP 服务器', e))),
        );
      }
    }
  }

  Future<void> _deleteMcpServer(String id) async {
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.deleteMcpServer(id: id);
      await _loadMcpServers();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(reportUiError('删除 MCP 服务器', e))),
        );
      }
    }
  }

  Future<void> _showMcpServerDialog({api.McpServerDto? existing}) async {
    bool trusted = existing?.trusted ?? false;
    final nameController = TextEditingController(text: existing?.name ?? '');
    final commandController = TextEditingController(
      text: existing?.command ?? '',
    );
    final argsController = TextEditingController(
      text: existing?.argsJson ?? '[]',
    );
    final envController = TextEditingController(
      text: existing?.envJson ?? '{}',
    );
    final urlController = TextEditingController(text: existing?.url ?? '');
    final headersController = TextEditingController(
      text: existing?.headersJson ?? '{}',
    );
    String transport = existing?.transport ?? 'stdio';
    String platform = existing?.platform ?? 'all';
    final result = await showDialog<_McpServerFormResult>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: Text(existing == null ? '添加 MCP 服务器' : '编辑 MCP 服务器'),
          content: SizedBox(
            width: 480,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  TextField(
                    controller: nameController,
                    decoration: const InputDecoration(
                      labelText: '名称（字母/数字/短横线/下划线）',
                      hintText: 'filesystem',
                    ),
                  ),
                  const SizedBox(height: AppTheme.space3),
                  SegmentedButton<String>(
                    segments: const [
                      ButtonSegment(
                        value: 'stdio',
                        label: Text('本机命令'),
                        icon: Icon(Icons.terminal, size: 16),
                      ),
                      ButtonSegment(
                        value: 'streamable-http',
                        label: Text('远程服务器'),
                        icon: Icon(Icons.public, size: 16),
                      ),
                    ],
                    selected: {transport},
                    onSelectionChanged: (selection) =>
                        setDialogState(() => transport = selection.first),
                  ),
                  const SizedBox(height: AppTheme.space3),
                  // 平台标签：定义随同步走，激活看平台。stdio 命令跨平台不
                  // 通（npx / npx.cmd），可为同一服务器建各平台变体。
                  SegmentedButton<String>(
                    segments: const [
                      ButtonSegment(value: 'all', label: Text('全部')),
                      ButtonSegment(value: 'linux', label: Text('Linux')),
                      ButtonSegment(value: 'macos', label: Text('macOS')),
                      ButtonSegment(value: 'windows', label: Text('Windows')),
                    ],
                    selected: {platform},
                    onSelectionChanged: (selection) =>
                        setDialogState(() => platform = selection.first),
                  ),
                  const SizedBox(height: AppTheme.space3),
                  if (transport == 'stdio') ...[
                    TextField(
                      controller: commandController,
                      decoration: const InputDecoration(
                        labelText: '命令',
                        hintText: 'npx',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: argsController,
                      decoration: const InputDecoration(
                        labelText: '参数（JSON 数组）',
                        hintText:
                            '["-y","@modelcontextprotocol/server-filesystem","/tmp"]',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: envController,
                      decoration: const InputDecoration(
                        labelText: '环境变量（JSON 对象）',
                        hintText: '{"NODE_ENV":"production"}',
                      ),
                    ),
                  ] else ...[
                    TextField(
                      controller: urlController,
                      decoration: const InputDecoration(
                        labelText: '端点 URL',
                        hintText: 'https://mcp.example.com/mcp',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: headersController,
                      decoration: const InputDecoration(
                        labelText: '自定义 Header（JSON 对象，鉴权等）',
                        hintText: '{"Authorization":"Bearer …"}',
                      ),
                    ),
                  ],
                  const SizedBox(height: AppTheme.space3),
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: const Text('信任（调用免审批）', style: TextStyle(fontSize: 13.5)),
                    subtitle: const Text(
                      '开启后该服务器的工具调用直接执行，不再逐次弹审批卡。MCP 服务器是第三方代码，请只对你信任的服务器开启。',
                      style: TextStyle(fontSize: 11.5),
                    ),
                    value: trusted,
                    onChanged: (v) => setDialogState(() => trusted = v),
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => _testMcpConnection(
                dialogContext,
                nameController.text,
                platform,
                transport,
                commandController.text,
                argsController.text,
                envController.text,
                urlController.text,
                headersController.text,
              ),
              child: const Text('测试连接'),
            ),
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () {
                Navigator.of(dialogContext).pop(_McpServerFormResult(
                  name: nameController.text,
                  platform: platform,
                  transport: transport,
                  command: commandController.text,
                  argsJson: argsController.text,
                  envJson: envController.text,
                  url: urlController.text,
                  headersJson: headersController.text,
                  trusted: trusted,
                ));
              },
              child: const Text('保存'),
            ),
          ],
        ),
      ),
    );
    if (result == null) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.saveMcpServer(api.McpServerDto(
        id: existing?.id ?? '',
        name: result.name,
        platform: result.platform,
        transport: result.transport,
        command: result.transport == 'stdio' ? result.command : '',
        argsJson: result.argsJson,
        envJson: result.envJson,
        url: result.transport == 'streamable-http' ? result.url : '',
        headersJson: result.headersJson,
        enabled: existing?.enabled ?? true,
        trusted: result.trusted,
        // 白名单不经保存对话框改写，独立开关维护；回填避免覆盖。
        allowReadonlyAuto: existing?.allowReadonlyAuto ?? false,
      ));
      await _loadMcpServers();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(reportUiError('保存 MCP 服务器', e))),
        );
      }
    }
  }

  Future<void> _testMcpConnection(
    BuildContext dialogContext,
    String name,
    String platform,
    String transport,
    String command,
    String argsJson,
    String envJson,
    String url,
    String headersJson,
  ) async {
    if (transport == 'stdio' && command.trim().isEmpty) {
      ScaffoldMessenger.maybeOf(dialogContext)?.showSnackBar(
        const SnackBar(content: Text('先填命令再测试')),
      );
      return;
    }
    if (transport == 'streamable-http' &&
        !url.trim().startsWith('http')) {
      ScaffoldMessenger.maybeOf(dialogContext)?.showSnackBar(
        const SnackBar(content: Text('先填 http(s) 端点 URL 再测试')),
      );
      return;
    }
    final messenger = ScaffoldMessenger.maybeOf(dialogContext);
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      final tools = await repository.testMcpServer(api.McpServerDto(
        id: 'test-probe',
        allowReadonlyAuto: false,
        name: name.isEmpty ? 'probe' : name,
        platform: platform,
        transport: transport,
        command: transport == 'stdio' ? command : '',
        argsJson: argsJson,
        envJson: envJson,
        url: transport == 'streamable-http' ? url : '',
        headersJson: headersJson,
        enabled: true,
        trusted: false,
      ));
      messenger?.showSnackBar(
        SnackBar(
          content: Text(
            tools.isEmpty
                ? '连接成功，但该服务器未提供工具'
                : '连接成功：${tools.map((t) => t.rawName).take(6).join('、')}'
                    '${tools.length > 6 ? ' 等 ${tools.length} 个工具' : ''}',
          ),
        ),
      );
    } catch (e) {
      messenger?.showSnackBar(
        SnackBar(content: Text(reportUiError('MCP 测试连接', e))),
      );
    }
  }

  /// 用户技能：skills 目录里每个 SKILL.md 一行，开关控制是否参与对话。
  Widget _buildUserSkillsContent() {
    final skills = _userSkills;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '把技能目录放进 <数据目录>/skills（每个技能一个子目录，含 SKILL.md），'
          '对话中由模型按需加载；脚本执行前会单独向你确认。'
          '把某个技能标为「信任」后，它的脚本调用直接执行、不再审批——脚本可在'
          '本机执行任意操作，开启即视为你了解并自担风险。',
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 13,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        Wrap(
          spacing: AppTheme.space2,
          runSpacing: AppTheme.space2,
          children: [
            OutlinedButton.icon(
              onPressed: _installingSkill
                  ? null
                  : () => _pickInstallSkill(false),
              icon: const Icon(Icons.folder_open_outlined, size: 18),
              label: const Text('从文件夹安装'),
            ),
            OutlinedButton.icon(
              onPressed: _installingSkill
                  ? null
                  : () => _pickInstallSkill(true),
              icon: const Icon(Icons.archive_outlined, size: 18),
              label: const Text('从 zip 安装'),
            ),
            if (!Platform.isAndroid && !Platform.isIOS)
              OutlinedButton.icon(
                onPressed: _installingSkill ? null : _promptGitInstall,
                icon: const Icon(Icons.commit_outlined, size: 18),
                label: const Text('从 Git 安装'),
              ),
            if (!Platform.isAndroid && !Platform.isIOS)
              TextButton.icon(
                onPressed: _openSkillsDirectory,
                icon: const Icon(Icons.output_outlined, size: 18),
                label: const Text('打开技能目录'),
              ),
          ],
        ),
        const SizedBox(height: AppTheme.space3),
        if (skills == null)
          Text(
            _userSkillsError ?? '读取中…',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else if (skills.isEmpty)
          Text(
            '尚未安装任何用户技能',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else
          ...skills.map(
            (skill) => Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(skill.title, style: const TextStyle(fontSize: 14)),
                      if (skill.brief.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            skill.brief,
                            style: TextStyle(
                              color: AppTheme.textSecondary,
                              fontSize: 12,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
                IconButton(
                  tooltip: skill.trusted ? '信任（脚本免审批，点击改为逐次审批）' : '逐次审批（点击标为信任，脚本免审批）',
                  icon: Icon(
                    skill.trusted
                        ? Icons.verified_user_outlined
                        : Icons.shield_outlined,
                    size: 18,
                    color: skill.trusted
                        ? AppTheme.accentPrimary
                        : AppTheme.textTertiary,
                  ),
                  onPressed: () => _toggleUserSkillTrusted(skill.id, !skill.trusted),
                ),
                IconButton(
                  tooltip: '卸载（全设备同步删除）',
                  icon: const Icon(Icons.delete_outline, size: 18),
                  onPressed: () => _uninstallUserSkill(skill.id),
                ),
                Switch(
                  value: skill.enabled,
                  onChanged: (value) => _toggleUserSkill(skill.id, value),
                ),
              ],
            ),
          ),
      ],
    );
  }

  Future<void> _loadUserSkills() async {
    final repository = ref.read(storageRepositoryProvider);
    if (repository is! RustBridgeRepository) return;
    try {
      final skills = await repository.listUserSkills();
      if (mounted)
        setState(() {
          _userSkills = skills;
          _userSkillsError = null;
        });
    } catch (e) {
      if (mounted)
        setState(() => _userSkillsError = reportUiError('读取用户技能', e));
    }
  }

  Future<void> _pickInstallSkill(bool asZip) async {
    final path = asZip
        ? (await FilePicker.pickFiles(
            dialogTitle: '选择技能 zip 包',
            type: FileType.custom,
            allowedExtensions: ['zip'],
          )).first.path
        : await FilePicker.getDirectoryPath(dialogTitle: '选择包含 SKILL.md 的技能目录');
    if (path == null) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await _installSkill(
      asZip
          ? () => repository.installUserSkillFromZip(path)
          : () => repository.installUserSkillFromDirectory(path),
      what: asZip ? 'zip 安装' : '目录安装',
    );
  }

  Future<void> _promptGitInstall() async {
    final controller = TextEditingController();
    final url = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('从 Git 仓库安装技能'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(
            hintText: 'https://github.com/user/skill-repo.git',
            labelText: '仓库地址（https 或本地路径）',
          ),
          onSubmitted: (v) => Navigator.of(context).pop(v.trim()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text.trim()),
            child: const Text('安装'),
          ),
        ],
      ),
    );
    if (url == null || url.isEmpty) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await _installSkill(
      () => repository.installUserSkillFromGit(url),
      what: 'Git 安装',
    );
  }

  Future<void> _installSkill(
    Future<api.UserSkillDto> Function() install, {
    required String what,
  }) async {
    setState(() => _installingSkill = true);
    try {
      final dto = await install();
      await _loadUserSkills();
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('已安装 ${dto.title}，默认启用')));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError(what, e))));
    } finally {
      if (mounted) setState(() => _installingSkill = false);
    }
  }

  Future<void> _openSkillsDirectory() async {
    final repository = ref.read(storageRepositoryProvider);
    if (repository is! RustBridgeRepository) return;
    try {
      final dir = await repository.userSkillsDirectoryPath();
      final binary = Platform.isMacOS
          ? 'open'
          : Platform.isWindows
          ? 'explorer'
          : 'xdg-open';
      await Process.run(binary, [dir]);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('打开技能目录', e))));
    }
  }

  Future<void> _uninstallUserSkill(String id) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('卸载技能'),
        content: Text('$id 将从本机删除；若已同步，其他设备也会一并删除。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('卸载'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      final repository =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repository.uninstallUserSkill(id);
      await _loadUserSkills();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(reportUiError('卸载技能', e))));
    }
  }

  Future<void> _toggleUserSkill(String id, bool enabled) async {
    setState(() {
      _userSkills = _userSkills
          ?.map((s) => s.id == id ? _copySkill(s, enabled: enabled) : s)
          .toList();
    });
    try {
      final repository =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repository.setUserSkillEnabled(id: id, enabled: enabled);
    } catch (e) {
      if (mounted) {
        setState(() => _userSkillsError = reportUiError('切换用户技能', e));
      }
      await _loadUserSkills();
    }
  }

  Future<void> _toggleUserSkillTrusted(String id, bool trusted) async {
    setState(() {
      _userSkills = _userSkills
          ?.map((s) => s.id == id ? _copySkill(s, trusted: trusted) : s)
          .toList();
    });
    try {
      final repository =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repository.setUserSkillTrusted(id: id, trusted: trusted);
      if (mounted && trusted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          const SnackBar(content: Text('已标为信任：该技能的脚本调用不再弹审批卡')),
        );
      }
    } catch (e) {
      if (mounted) {
        setState(() => _userSkillsError = reportUiError('切换信任状态', e));
      }
      await _loadUserSkills();
    }
  }

  api.UserSkillDto _copySkill(
    api.UserSkillDto s, {
    bool? enabled,
    bool? trusted,
  }) =>
      api.UserSkillDto(
        id: s.id,
        title: s.title,
        brief: s.brief,
        enabled: enabled ?? s.enabled,
        trusted: trusted ?? s.trusted,
        hasSkillMd: s.hasSkillMd,
      );

  /// 设备同步独立 tab：本地密钥/移交/名册/冲突态势都聚在区块内。
  /// 知识管道仪表（主循环吞吐与产物）：分析队列 → 消化 → 导出。
  /// 与「数据」tab（存储与用量）分开——原单 tab 五组内容太挤。
  Widget _buildKnowledgeTab() {
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
            title: '知识导出',
            icon: Icons.ios_share_outlined,
            child: _buildWikiExportContent(),
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
            title: '数据库维护',
            icon: Icons.cleaning_services_outlined,
            child: _buildDbMaintenanceContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '每日 Token 使用',
            icon: Icons.analytics_outlined,
            child: _buildTokenUsageContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildRulesTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '个人规则库',
            icon: Icons.rule_outlined,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _buildRuleTriggerPrefs(),
                const SizedBox(height: AppTheme.space4),
                _buildCompanionRevisitPrefs(),
                const SizedBox(height: AppTheme.space4),
                _buildRulesContent(),
                if (_recentRuleHits.isNotEmpty) ...[
                  const SizedBox(height: AppTheme.space4),
                  _buildRecentRuleHits(),
                ],
                ChatProposalReviewButton(
                  conversationId: null,
                  label: '审阅历史未归属规则',
                  onChanged: _loadRules,
                ),
              ],
            ),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: '用户模型',
            icon: Icons.account_tree_outlined,
            child: _buildUserModelContent(),
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
        '还没有规则。在对话里分享踩坑或心得时，AI 会建议把其中的经验沉淀成规则，你在审阅卡接受具体规则后就会出现在这里，以后遇到类似情况 AI 会主动引用并提醒你。',
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
                icon: Icon(
                  _rules[i].hasTrigger
                      ? Icons.notifications_active_outlined
                      : Icons.notifications_none,
                  size: 18,
                ),
                color: _rules[i].hasTrigger
                    ? AppTheme.accentPrimary
                    : AppTheme.textTertiary,
                tooltip: _rules[i].hasTrigger ? '触发条件（已配置）' : '配置触发条件',
                onPressed: () => _editRuleTrigger(_rules[i]),
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
    Widget? trailing,
  }) {
    return Container(
      padding: const EdgeInsets.all(22),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.75),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: AppTheme.surface3.withValues(alpha: 0.6)),
        boxShadow: [
          BoxShadow(
            color: Colors.black.withValues(alpha: 0.12),
            blurRadius: 10,
            offset: const Offset(0, 3),
          ),
        ],
      ),
      // 透明 Material 垫底：带背景色的 DecoratedBox 不能挡在 ListTile 与其
      // Material 祖先之间，否则 CheckboxListTile（如主人端配对同意勾选）的
      // 背景/水波纹不可见，debug 模式直接断言崩。
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, color: AppTheme.accentPrimary, size: 20),
                ),
                const SizedBox(width: 12),
                Text(
                  title,
                  style: TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                    letterSpacing: -0.2,
                  ),
                ),
                if (trailing != null) ...[const Spacer(), trailing],
              ],
            ),
            const SizedBox(height: 18),
            child,
          ],
        ),
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
          _buildProviderCardGrid(),
      ],
    );
  }

  /// Provider 以卡片网格展示：宽度够时并排多列，窄窗（侧边栏 / 小窗口）自动收成一列。
  ///
  /// 用 `Wrap` 而不是 `GridView` —— 外层是 `SingleChildScrollView`，`GridView`
  /// 需要显式 `shrinkWrap` + `NeverScrollableScrollPhysics`，而 `Wrap` 天然
  /// 按可用高度伸展、少一个滚动容器嵌套的坑。
  Widget _buildProviderCardGrid() {
    const minCardWidth = 260.0;
    const gap = AppTheme.space3;

    return LayoutBuilder(
      builder: (context, constraints) {
        // constraints.maxWidth 可能无限（理论上不会，但别让除法炸掉）。
        final available = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : minCardWidth * 3;
        final columns = ((available + gap) / (minCardWidth + gap))
            .floor()
            .clamp(1, 4);
        final cardWidth = (available - gap * (columns - 1)) / columns;

        return Wrap(
          spacing: gap,
          runSpacing: gap,
          children: [
            for (final p in _providers)
              SizedBox(width: cardWidth, child: _buildProviderCard(p)),
          ],
        );
      },
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
            '尚未配置任何 AI Provider，点击右上角「添加」开始。',
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

  /// 单个 provider 卡片。
  ///
  /// 结构：标题行（名称 + 激活徽标）→ 类型徽标 + 模型名 → endpoint → 操作行。
  /// 温度/上下文/输出上限/密钥来源等参数不进卡片（编辑对话框里可见），
  /// 卡片只留识别与切换所需的最小信息。
  Widget _buildProviderCard(api.AiProviderConfigDto p) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
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
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
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
                const SizedBox(width: AppTheme.space2),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space2,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(AppTheme.radiusFull),
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
          const SizedBox(height: AppTheme.space2),
          // 类型徽标与模型名同行：卡片纵向压缩，模型名可能较长， Flexible 会省略。
          Row(
            children: [
              _buildProviderBadge(p.providerType),
              const SizedBox(width: AppTheme.space2),
              Flexible(
                child: Text(
                  p.model,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: AppTheme.textPrimary,
                    fontSize: 13,
                    fontWeight: FontWeight.w500,
                    height: 1.3,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppTheme.space3),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.link_rounded, size: 13, color: AppTheme.textTertiary),
              const SizedBox(width: AppTheme.space1),
              Expanded(
                child: Text(
                  p.baseUrl,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: AppTheme.textTertiary,
                    fontSize: 11,
                    height: 1.35,
                  ),
                ),
              ),
            ],
          ),
          ProviderEndpointNotice(baseUrl: p.baseUrl),
          const SizedBox(height: AppTheme.space3),
          // 不能加 const：AppTheme.surface3 是运行时主题色，深浅模式下不同。
          Divider(height: 1, color: AppTheme.surface3),
          const SizedBox(height: AppTheme.space2),
          Row(
            children: [
              Expanded(
                child: p.isActive
                    // 激活项不再显示「设为激活」：按钮点了没意义，占位还让卡片左右不对称。
                    ? Align(
                        alignment: Alignment.centerLeft,
                        child: Text(
                          '对话使用此配置',
                          style: TextStyle(
                            color: AppTheme.textTertiary,
                            fontSize: 11,
                          ),
                        ),
                      )
                    : Align(
                        alignment: Alignment.centerLeft,
                        child: TextButton(
                          onPressed: () => _activateProvider(p),
                          style: TextButton.styleFrom(
                            padding: const EdgeInsets.symmetric(horizontal: 8),
                            minimumSize: const Size(0, 28),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          child: const Text('设为激活'),
                        ),
                      ),
              ),
              IconButton(
                tooltip: '编辑',
                icon: const Icon(Icons.edit_outlined, size: 18),
                color: AppTheme.textSecondary,
                visualDensity: VisualDensity.compact,
                onPressed: () => _openProviderEditor(p),
              ),
              IconButton(
                tooltip: '复制配置（API Key 需重新输入）',
                icon: const Icon(Icons.copy_rounded, size: 18),
                color: AppTheme.textSecondary,
                visualDensity: VisualDensity.compact,
                onPressed: () => _openProviderEditor(p, duplicate: true),
              ),
              IconButton(
                tooltip: '删除',
                icon: const Icon(Icons.delete_outline, size: 18),
                color: AppTheme.error,
                visualDensity: VisualDensity.compact,
                onPressed: () => _deleteProvider(p),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildProviderBadge(String providerType) {
    return Align(
      alignment: Alignment.centerLeft,
      child: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: AppTheme.space2,
          vertical: 3,
        ),
        decoration: BoxDecoration(
          color: AppTheme.surface3,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        ),
        child: Text(
          _providerTypeLabel(providerType),
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 11,
            fontWeight: FontWeight.w500,
          ),
        ),
      ),
    );
  }

  Future<void> _activateProvider(api.AiProviderConfigDto p) async {
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.setActiveAiProviderConfig(
        p.id,
        revisionToken: p.revisionToken,
      );
      await _loadProviders();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(reportUiError('切换 Provider', e))));
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
      await repo.deleteAiProviderConfig(p.id, revisionToken: p.revisionToken);
      await _loadProviders();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(reportUiError('删除 Provider', e))));
    }
  }

  Future<void> _openProviderEditor(
    api.AiProviderConfigDto? existing, {
    bool duplicate = false,
  }) async {
    final isNew = existing == null || duplicate;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    // 保存动作在对话框内部执行：失败时错误留在对话框里，用户输入不丢。
    // （此前保存发生在 pop 之后，校验失败时对话框已关闭，填了一半的表单全没。）
    Future<void> onSave(_ProviderDraft draft) async {
      final target = duplicate ? null : existing;
      await repo.saveAiProviderConfig(
        api.AiProviderConfigDto(
          id: target?.id ?? '',
          name: draft.name,
          providerType: draft.providerType,
          baseUrl: draft.baseUrl,
          model: draft.model,
          apiKeySource: target?.apiKeySource ?? '',
          apiKey: draft.apiKey,
          // 复制出的配置不继承激活态：避免出现两个同时激活。
          isActive: duplicate ? false : target?.isActive ?? false,
          temperature: draft.temperature,
          maxTokens: draft.maxTokens,
          contextWindow: draft.contextWindow,
          // revisionToken 仅编辑原配置时携带；duplicate 是新建，不带锁令牌。
          revisionToken: (target != null && !duplicate)
              ? target.revisionToken
              : null,
        ),
      );
    }

    final saved = await showDialog<_ProviderDraft>(
      context: context,
      builder: (_) => _ProviderEditorDialog(
        isNew: isNew,
        isDuplicate: duplicate,
        onSave: onSave,
        initialName: duplicate ? '${existing!.name} 副本' : existing?.name ?? '',
        initialBaseUrl: existing?.baseUrl ?? '',
        initialModel: existing?.model ?? '',
        initialTemperature: existing?.temperature ?? 0.7,
        initialContextWindow: existing?.contextWindow?.toString() ?? '',
        initialMaxTokens: existing?.maxTokens?.toString() ?? '',
        initialProviderType: existing?.providerType ?? 'openai-compatible',
      ),
    );
    // 保存成功时由对话框 pop；失败时对话框保持打开，此处只处理取消。
    if (saved == null || !mounted) return;
    await _loadProviders();
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
            label: '对话记忆预算（可选）',
            controller: _maxTokensController,
            hint: '留空按 Provider 上下文窗口推算',
            keyboardType: TextInputType.number,
          ),
        const SizedBox(height: 16),
        // 保存按钮只负责本卡片的两个数字字段（maxMessages / maxTokens）；
        // 策略类型下拉是 onChanged 即时存。原 titlebar 的全局「保存」已删，
        // 按钮收进本卡片，语义与作用范围一致。
        Align(
          alignment: Alignment.centerRight,
          child: FilledButton.icon(
            icon: const Icon(Icons.check, size: 18),
            label: const Text('保存'),
            onPressed: _saveSettings,
            style: FilledButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
              foregroundColor: Colors.white,
            ),
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
                label: '近 30 天 Total',
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
        _TokenUsageBarChart(
          // 图表按时间正序（左旧右新）；_dailyUsage 是倒序（最新在前）。
          days: _dailyUsage.take(30).toList().reversed.toList(),
          today: today,
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
                  content: Text(reportUiError('保存推文抓取服务', e)),
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

  // static：只依赖 AppTheme 全局量，不碰任何 State 字段，
  // 因此 Provider 编辑对话框（独立 State）也能复用。
  static Widget _buildTextField({
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

  static Widget _buildDropdownField<T>({
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
          initialValue: value,
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
              _SettingsScreenState._buildTextField(
                label: '配置名称 *',
                hint: '如：主用 GPT',
                controller: _name,
              ),
              const SizedBox(height: 12),
              _SettingsScreenState._buildDropdownField<String>(
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
              _SettingsScreenState._buildTextField(
                label: 'Base URL *',
                hint: 'https://api.openai.com/v1',
                controller: _baseUrl,
              ),
              const SizedBox(height: 12),
              _SettingsScreenState._buildTextField(
                label: '模型 *',
                hint: 'gpt-4o',
                controller: _model,
              ),
              const SizedBox(height: 12),
              _SettingsScreenState._buildTextField(
                label: widget.isNew ? 'API Key *' : 'API Key',
                hint: widget.isNew ? '输入你的 API Key' : '留空表示保持不变',
                obscure: true,
                controller: _apiKey,
              ),
              const SizedBox(height: 12),
              _SettingsScreenState._buildTextField(
                label: '上下文窗口（token）',
                hint: '留空使用保守默认值；中转可填写实测上限',
                keyboardType: TextInputType.number,
                controller: _contextWindow,
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: _SettingsScreenState._buildTextField(
                      label: 'Temperature',
                      hint: '0.7',
                      keyboardType: TextInputType.number,
                      controller: _temperature,
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: _SettingsScreenState._buildTextField(
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
