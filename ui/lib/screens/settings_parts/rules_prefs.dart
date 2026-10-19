part of '../settings_screen.dart';
// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 规则偏好、命中反馈与规则内容区。
extension _RulesPrefs on _SettingsScreenState {
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
  /// 对话影子校验审查面（迁移 54）：展示聊天里预筛命中的规则与后台判定
  /// 状态。只读展示——影子校验绝不改写对话，这里也不提供会反向影响对话
  /// 的操作；excluded（判定不适用）默认收起不打扰。
  Widget _buildChatRuleHits() {
    String label(String status) => switch (status) {
          'pending' => '判定中',
          'applicable' => '已在对话遵守',
          _ => '不适用',
        };
    IconData icon(String status) => status == 'applicable'
        ? Icons.verified_outlined
        : Icons.hourglass_top_outlined;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '对话中的规则场景（后台静默判定，不打扰对话）',
          style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
        ),
        const SizedBox(height: AppTheme.space2),
        for (final hit in _recentChatRuleHits.where(
          (h) => h.status != 'excluded',
        ))
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 2),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  icon(hit.status),
                  size: 14,
                  color: hit.status == 'applicable'
                      ? AppTheme.accentPrimary
                      : AppTheme.textTertiary,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    '[${label(hit.status)}] ${hit.ruleContent}',
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 12,
                      height: 1.4,
                    ),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }

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
                if (_recentChatRuleHits.any(
                  (h) => h.status != 'excluded',
                )) ...[
                  const SizedBox(height: AppTheme.space4),
                  _buildChatRuleHits(),
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
}
