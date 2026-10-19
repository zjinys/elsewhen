part of '../settings_screen.dart';
// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 加载器与用户模型/规则情绪区（extension 平移，行为零变化）。
extension _DataRulesLoads on _SettingsScreenState {
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
      // 对话影子命中独立读：失败只损失审查面展示，不影响规则列表。
      final chatHits = await repo.listRecentChatRuleHits(limit: 20);
      if (!mounted) return;
      setState(() {
        _ruleTriggerMode = mode;
        _recentRuleHits = hits;
        _recentChatRuleHits = chatHits;
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
}
