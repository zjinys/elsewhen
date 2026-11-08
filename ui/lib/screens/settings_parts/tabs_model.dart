part of '../settings_screen.dart';

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! Tab 栏骨架与模型/数据维护/外观 tab 装配。
extension _TabsModel on _SettingsScreenState {
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
                  _buildSettingsTabItem(
                    0,
                    context.l10n.models,
                    Icons.smart_toy_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    1,
                    context.l10n.appearance,
                    Icons.palette_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    2,
                    context.l10n.integrations,
                    Icons.hub_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    3,
                    context.l10n.network,
                    Icons.dns_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    4,
                    context.l10n.skills,
                    Icons.extension_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(5, 'MCP', Icons.usb_outlined),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    6,
                    context.l10n.knowledge,
                    Icons.auto_stories_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    7,
                    context.l10n.habits,
                    Icons.notifications_active_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    8,
                    context.l10n.data,
                    Icons.dataset_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    9,
                    context.l10n.rules,
                    Icons.rule_outlined,
                  ),
                  const SizedBox(height: AppTheme.space1),
                  _buildSettingsTabItem(
                    10,
                    context.l10n.security,
                    Icons.lock_outline,
                  ),
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
                child: Text(
                  _version,
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
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
            tabs: [
              Tab(
                text: context.l10n.models,
                icon: Icon(Icons.smart_toy_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.appearance,
                icon: Icon(Icons.palette_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.integrations,
                icon: Icon(Icons.hub_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.network,
                icon: Icon(Icons.dns_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.skills,
                icon: Icon(Icons.extension_outlined, size: 17),
              ),
              Tab(text: 'MCP', icon: Icon(Icons.usb_outlined, size: 17)),
              Tab(
                text: context.l10n.knowledge,
                icon: Icon(Icons.auto_stories_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.habits,
                icon: Icon(Icons.notifications_active_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.data,
                icon: Icon(Icons.dataset_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.rules,
                icon: Icon(Icons.rule_outlined, size: 17),
              ),
              Tab(
                text: context.l10n.security,
                icon: Icon(Icons.lock_outline, size: 17),
              ),
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
              Expanded(
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                    color: selected
                        ? AppTheme.textPrimary
                        : AppTheme.textTertiary,
                  ),
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
            title: context.l10n.aiProviders,
            icon: Icons.psychology_outlined,
            trailing: FilledButton.icon(
              onPressed: () => _openProviderEditor(null),
              icon: const Icon(Icons.add, size: 18),
              label: Text(context.l10n.add),
              style: FilledButton.styleFrom(
                backgroundColor: AppTheme.accentPrimary,
                foregroundColor: Colors.white,
              ),
            ),
            child: _buildAiProviderSettings(settings),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.memoryStrategy,
            icon: Icons.memory_outlined,
            child: _buildMemorySettings(settings),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.prompts,
            icon: Icons.edit_note_outlined,
            onTitleTap: _tapPromptTitle,
            child: PromptSettings(showDiagnostics: _promptTitleTaps >= 10),
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
        context.l10n.analysisStatusFailed,
        style: TextStyle(color: AppTheme.textSecondary),
      );
    }

    final waiting = stats.pending + stats.running + stats.retry;
    return Wrap(
      spacing: AppTheme.space4,
      runSpacing: AppTheme.space3,
      children: [
        _buildUsageMetric(label: context.l10n.pending, value: '$waiting'),
        _buildUsageMetric(
          label: context.l10n.processing,
          value: '${stats.running}',
        ),
        _buildUsageMetric(
          label: context.l10n.waitingRetry,
          value: '${stats.retry}',
        ),
        _buildUsageMetric(
          label: context.l10n.completed,
          value: '${stats.succeeded}',
        ),
        _buildUsageMetric(label: context.l10n.failed, value: '${stats.failed}'),
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
        context.l10n.digestStatusFailed,
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
          context.l10n.digestDescription,
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
            _buildUsageMetric(label: context.l10n.pending, value: '$waiting'),
            _buildUsageMetric(
              label: context.l10n.completed,
              value: '${stats.succeeded}',
            ),
            _buildUsageMetric(
              label: context.l10n.failed,
              value: '${stats.failed}',
            ),
            _buildUsageMetric(
              label: context.l10n.skipped,
              value: '${stats.skipped}',
            ),
            _buildUsageMetric(
              label: context.l10n.lastSuccess,
              value: when(stats.lastSuccessAt),
            ),
          ],
        ),
        if (stats.lastError != null && stats.lastError!.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space3),
          AppErrorMessage(
            action: context.l10n.knowledgeProcessing,
            error: stats.lastError!,
            message: context.l10n.lastFailure(
              when(stats.lastErrorAt),
              context.errorMessage(
                context.l10n.knowledgeProcessing,
                stats.lastError!,
              ),
            ),
          ),
        ],
        const SizedBox(height: AppTheme.space3),
        OutlinedButton.icon(
          icon: const Icon(Icons.list_alt_outlined, size: 16),
          label: Text(context.l10n.viewQueueLogs),
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
          context.l10n.wikiExportDescription,
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
          label: Text(
            _exportingWiki
                ? context.l10n.exporting
                : context.l10n.chooseExportFolder,
          ),
        ),
      ],
    );
  }

  Future<void> _exportWikiMarkdown() async {
    final directory = await FilePicker.getDirectoryPath(
      dialogTitle: context.l10n.exportFolder,
    );
    if (!mounted || directory == null || directory.trim().isEmpty) return;
    setState(() => _exportingWiki = true);
    try {
      final repository = ref.read(storageRepositoryProvider);
      if (repository is! RustBridgeRepository) {
        throw StateError(context.l10n.storageExportUnsupported);
      }
      final result = await repository.exportWikiMarkdown(outDir: directory);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.exportSuccess(result.exported, result.outDir),
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(context.l10n.l6ExportKnowledge863fb, e),
          ),
        ),
      );
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
            _dbStatsError == null
                ? context.l10n.loading
                : context.notice(_dbStatsError),
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 13),
          )
        else ...[
          Text(
            context.l10n.databaseSize(
              mb(stats.fileBytes),
              mb(stats.freeBytes),
              stats.freeBytes > 8 * 1024 * 1024
                  ? context.l10n.compactionRecommended
                  : '',
            ),
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
            label: Text(
              _vacuuming ? context.l10n.compacting : context.l10n.compactNow,
            ),
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
      if (mounted) {
        setState(() {
          _dbStats = stats;
          _dbStatsError = null;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(
          () => _dbStatsError = _settingsFailure(
            context.l10n.l6LoadDatabaseSize69850,
            e,
          ),
        );
      }
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
                ? context.l10n.databaseSpaceReclaimed(saved.toStringAsFixed(1))
                : context.l10n.databaseAlreadyCompact,
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(context.l10n.l6OptimizeDatabase12519, e),
          ),
        ),
      );
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
            title: context.l10n.appearance,
            icon: Icons.palette_outlined,
            child: _buildAppearanceSettings(settings),
          ),
          if (_isDesktopPlatform) ...[
            const SizedBox(height: AppTheme.space6),
            _buildSection(
              title: context.l10n.desktopWindow,
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
        context.l10n.noTrayAvailable,
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
          context.l10n.onWindowClose,
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
                description: context.l10n.stayInTrayDescription,
                onSelected: () => _applyCloseAction(CloseAction.minimizeToTray),
              ),
              const SizedBox(width: 10),
              _buildCloseActionCard(
                action: CloseAction.quit,
                selected: _closeAction == CloseAction.quit,
                icon: Icons.power_settings_new_rounded,
                description: context.l10n.quitDescription,
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
                      action == CloseAction.minimizeToTray
                          ? context.l10n.minimizeToTray
                          : context.l10n.quitApp,
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
}
