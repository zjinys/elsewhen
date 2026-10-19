part of '../settings_screen.dart';
// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! AI 供应商卡片、徽标与增删激活入口。
extension _ProvidersView on _SettingsScreenState {
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
}
