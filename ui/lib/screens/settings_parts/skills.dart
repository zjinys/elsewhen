part of '../settings_screen.dart';

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 用户技能安装/卸载/信任/开关操作与内容面板。
extension _Skills on _SettingsScreenState {
  /// 用户技能：skills 目录里每个 SKILL.md 一行，开关控制是否参与对话。
  Widget _buildUserSkillsContent() {
    final skills = _userSkills;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.l6PlaceEachSkillFolderWithIts94f37,
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
              label: Text(context.l10n.l6InstallFromFolder22a87),
            ),
            OutlinedButton.icon(
              onPressed: _installingSkill
                  ? null
                  : () => _pickInstallSkill(true),
              icon: const Icon(Icons.archive_outlined, size: 18),
              label: Text(context.l10n.l6InstallFromZip906f1),
            ),
            if (!Platform.isAndroid && !Platform.isIOS)
              OutlinedButton.icon(
                onPressed: _installingSkill ? null : _promptGitInstall,
                icon: const Icon(Icons.commit_outlined, size: 18),
                label: Text(context.l10n.l6InstallFromGit0882e),
              ),
            if (!Platform.isAndroid && !Platform.isIOS)
              TextButton.icon(
                onPressed: _openSkillsDirectory,
                icon: const Icon(Icons.output_outlined, size: 18),
                label: Text(context.l10n.l6OpenSkillsFoldera4437),
              ),
          ],
        ),
        const SizedBox(height: AppTheme.space3),
        // 列表头：数量统计 + 「批量删除」/「全选」入口同行（与 MCP 列表同款）。
        if (skills != null && skills.isNotEmpty)
          Row(
            children: [
              Text(
                _skillBatchMode
                    ? context.l10n.l6SelectedArg0Arg1fd75c(
                        (_skillSelected.length).toString(),
                        (skills.length).toString(),
                      )
                    : context.l10n.l6TotalArg01e668((skills.length).toString()),
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  letterSpacing: 0.4,
                  color: AppTheme.textTertiary,
                ),
              ),
              const Spacer(),
              TextButton.icon(
                onPressed: () => setState(() {
                  _skillBatchMode = !_skillBatchMode;
                  _skillSelected.clear();
                }),
                icon: Icon(
                  _skillBatchMode
                      ? Icons.close_outlined
                      : Icons.checklist_outlined,
                  size: 15,
                ),
                label: Text(
                  _skillBatchMode
                      ? context.l10n.l6ExitSelectione23ed
                      : context.l10n.l6DeleteMultiple362ae,
                  style: const TextStyle(fontSize: 12.5),
                ),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                ),
              ),
              if (_skillBatchMode) ...[
                const SizedBox(width: 4),
                TextButton(
                  onPressed: () => setState(() {
                    if (_skillSelected.length == skills.length) {
                      _skillSelected.clear();
                    } else {
                      _skillSelected
                        ..clear()
                        ..addAll(skills.map((s) => s.id));
                    }
                  }),
                  style: TextButton.styleFrom(
                    visualDensity: VisualDensity.compact,
                  ),
                  child: Text(
                    _skillSelected.length == skills.length
                        ? context.l10n.l6DeselectAllba20e
                        : context.l10n.l6SelectAll3e44b,
                    style: const TextStyle(fontSize: 12.5),
                  ),
                ),
              ],
            ],
          ),
        if (_skillBatchMode && skills != null && skills.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space2),
          // 批量操作条：确认删除需二次点击（弹确认框），取消则退出选择模式。
          Row(
            children: [
              FilledButton.icon(
                onPressed: _skillSelected.isEmpty
                    ? null
                    : () => _batchUninstallSkills(),
                icon: const Icon(Icons.delete_outline, size: 16),
                label: Text(
                  context.l10n.l6DeleteSelectedArg0def95(
                    (_skillSelected.length).toString(),
                  ),
                ),
                style: FilledButton.styleFrom(
                  backgroundColor: AppTheme.error,
                  visualDensity: VisualDensity.compact,
                ),
              ),
              const SizedBox(width: 8),
              OutlinedButton(
                onPressed: () => setState(() {
                  _skillBatchMode = false;
                  _skillSelected.clear();
                }),
                style: OutlinedButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                ),
                child: Text(context.l10n.cancel),
              ),
            ],
          ),
          const SizedBox(height: AppTheme.space2),
        ],
        const SizedBox(height: AppTheme.space3),
        if (skills == null)
          _McpStatusLine(
            icon: Icons.hourglass_empty_outlined,
            text: context.l10n.l6LoadingSkills8d716,
            error: _userSkillsError,
          )
        else if (skills.isEmpty)
          _McpStatusLine(
            icon: Icons.extension_outlined,
            text: context.l10n.l6NoUserSkillsInstalledInstallAe8aea,
          )
        else
          ...skills.map(
            (skill) => _UserSkillCard(
              skill: skill,
              selecting: _skillBatchMode,
              selected: _skillSelected.contains(skill.id),
              onToggleSelected: () => setState(() {
                if (!_skillSelected.remove(skill.id)) {
                  _skillSelected.add(skill.id);
                }
              }),
              onToggleEnabled: (v) => _toggleUserSkill(skill.id, v),
              onToggleTrusted: () =>
                  _toggleUserSkillTrusted(skill.id, !skill.trusted),
              onUninstall: () => _uninstallUserSkill(skill.id),
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
      if (mounted) {
        setState(() {
          _userSkills = skills;
          _userSkillsError = null;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(
          () => _userSkillsError = _settingsFailure(
            context.l10n.l6LoadUserSkillsfb79f,
            e,
          ),
        );
      }
    }
  }

  Future<void> _pickInstallSkill(bool asZip) async {
    final path = asZip
        ? (await FilePicker.pickFiles(
            dialogTitle: context.l10n.l6ChooseASkillZipFile0e485,
            type: FileType.custom,
            allowedExtensions: ['zip'],
          )).first.path
        : await FilePicker.getDirectoryPath(
            dialogTitle: context.l10n.l6ChooseASkillFolderContainingSkill8789f,
          );
    if (path == null || !mounted) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await _installSkill(
      asZip
          ? () => repository.installUserSkillFromZip(path)
          : () => repository.installUserSkillFromDirectory(path),
      what: asZip
          ? context.l10n.l6InstallFromZip37d9a
          : context.l10n.l6InstallFromFolder71cd7,
    );
  }

  Future<void> _promptGitInstall() async {
    final controller = TextEditingController();
    final url = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.l6InstallASkillFromGitfa921),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: InputDecoration(
            hintText: 'https://github.com/user/skill-repo.git',
            labelText: context.l10n.l6RepositoryUrlHttpsOrLocalPath3ae69,
          ),
          onSubmitted: (v) => Navigator.of(context).pop(v.trim()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text.trim()),
            child: Text(context.l10n.l6Install087db),
          ),
        ],
      ),
    );
    if (url == null || url.isEmpty || !mounted) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await _installSkill(
      () => repository.installUserSkillFromGit(url),
      what: context.l10n.l6InstallFromGit6d266,
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6InstalledArg0EnabledByDefaultdd2c2(
              (dto.title).toString(),
            ),
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(what, e),
          ),
        ),
      );
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(context.l10n.l6OpenSkillsFoldera4437, e),
          ),
        ),
      );
    }
  }

  Future<void> _uninstallUserSkill(String id) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.l6UninstallSkill053a6),
        content: Text(
          context.l10n.l6Arg0WillBeRemovedFromThis1a7c6((id).toString()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.l6Uninstall200cf),
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
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(context.l10n.l6UninstallSkill053a6, e),
          ),
        ),
      );
    }
  }

  /// 批量卸载：先确认，再逐项调卸载；单项失败不中断其余项，
  /// 最后汇总报告成功/失败数并重载列表。
  Future<void> _batchUninstallSkills() async {
    final ids = _skillSelected.toList();
    if (ids.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.l6UninstallMultipleSkills13309),
        content: Text(
          context.l10n.l6SkillsToRemoveArg0SyncedCopiesaa41a(
            (ids.length).toString(),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.l6Uninstall200cf),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    var ok = 0;
    final failures = <String>[];
    for (final id in ids) {
      try {
        await repository.uninstallUserSkill(id);
        ok++;
      } catch (e) {
        failures.add(id);
        if (mounted) {
          ScaffoldMessenger.maybeOf(context)?.showSnackBar(
            SnackBar(
              content: AppErrorMessage(
                action: context.l10n.load,
                error: _settingsFailure(
                  context.l10n.l6UninstallSkillArg0a3819((id).toString()),
                  e,
                ),
              ),
            ),
          );
        }
      }
    }
    if (!mounted) return;
    setState(() {
      _skillSelected.clear();
      if (failures.isEmpty) _skillBatchMode = false;
    });
    await _loadUserSkills();
    if (!mounted) return;
    if (failures.isEmpty) {
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6SkillsUninstalledArg08db74((ok).toString()),
          ),
        ),
      );
    } else {
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6SucceededArg0FailedArg1Arg2Arg3a2cde(
              (ok).toString(),
              (failures.length).toString(),
              (failures.take(3).join('、')).toString(),
              (failures.length > 3 ? context.l10n.l6AndMore93ac7 : '')
                  .toString(),
            ),
          ),
        ),
      );
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
        setState(
          () => _userSkillsError = _settingsFailure(
            context.l10n.l6ToggleUserSkill2de88,
            e,
          ),
        );
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
          SnackBar(
            content: Text(context.l10n.l6MarkedAsTrustedThisSkillS1cf36),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        setState(
          () => _userSkillsError = _settingsFailure(
            context.l10n.l6ChangeTrustSetting7ce93,
            e,
          ),
        );
      }
      await _loadUserSkills();
    }
  }

  api.UserSkillDto _copySkill(
    api.UserSkillDto s, {
    bool? enabled,
    bool? trusted,
  }) => api.UserSkillDto(
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
            title: context.l10n.analysisQueue,
            icon: Icons.sync_outlined,
            child: _buildAnalysisJobStatsContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.knowledgeProcessing,
            icon: Icons.auto_stories_outlined,
            child: _buildDigestStatsContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.l6KnowledgeExport26466,
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
            title: context.l10n.l6DatabaseMaintenance60286,
            icon: Icons.cleaning_services_outlined,
            child: _buildDbMaintenanceContent(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.l6DailyTokenUsage5a651,
            icon: Icons.analytics_outlined,
            child: _buildTokenUsageContent(),
          ),
        ],
      ),
    );
  }
}

/// 单个用户技能卡片（与 _McpServerCard 同款视觉，2026-11-04 统一）：
/// - 左侧扩展图标，停用时整体降饱和；批量选择模式下图标位换成勾选框；
/// - 标题行：名称 + 信任徽章（复用 _McpBadge）；
/// - 副标题：一句话简介单行省略，无 description 时退回技能 id；
/// - 尾部：溢出菜单（信任/卸载）+ 启用开关，不再横排图标。
class _UserSkillCard extends StatelessWidget {
  final api.UserSkillDto skill;
  final ValueChanged<bool> onToggleEnabled;
  final VoidCallback onToggleTrusted;
  final VoidCallback onUninstall;

  /// 批量选择模式：为 true 时尾部操作控件换成勾选框，整卡点击切换选中。
  final bool selecting;
  final bool selected;
  final VoidCallback? onToggleSelected;

  const _UserSkillCard({
    required this.skill,
    required this.onToggleEnabled,
    required this.onToggleTrusted,
    required this.onUninstall,
    this.selecting = false,
    this.selected = false,
    this.onToggleSelected,
  });

  @override
  Widget build(BuildContext context) {
    final enabled = skill.enabled;
    // 主色随状态：启用=强调色，停用=灰（同 MCP 卡片）；选中时边框高亮。
    final leadColor = enabled ? AppTheme.accentPrimary : AppTheme.textTertiary;
    final card = Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(12, 10, 10, 10),
      decoration: BoxDecoration(
        color: enabled ? AppTheme.surface2 : AppTheme.surface1,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(
          color: selected
              ? AppTheme.accentPrimary
              : AppTheme.surface3.withValues(alpha: enabled ? 0.7 : 0.4),
        ),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          // 批量选择模式：图标位换成勾选框；否则展示扩展图标。
          if (selecting)
            Icon(
              selected ? Icons.check_circle : Icons.radio_button_unchecked,
              size: 22,
              color: selected ? AppTheme.accentPrimary : AppTheme.textTertiary,
            )
          else
            Container(
              width: 34,
              height: 34,
              decoration: BoxDecoration(
                color: leadColor.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
              ),
              child: Icon(Icons.extension_outlined, size: 18, color: leadColor),
            ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        skill.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 13.5,
                          fontWeight: FontWeight.w600,
                          color: enabled
                              ? AppTheme.textPrimary
                              : AppTheme.textTertiary,
                        ),
                      ),
                    ),
                    if (skill.trusted) ...[
                      const SizedBox(width: 6),
                      _McpBadge(
                        label: context.l10n.l6TrustedNoApproval69302,
                        color: AppTheme.accentPrimary,
                        icon: Icons.verified_user_outlined,
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 3),
                Text(
                  skill.brief.isNotEmpty ? skill.brief : 'id: ${skill.id}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11.5,
                    color: enabled
                        ? AppTheme.textSecondary
                        : AppTheme.textTertiary,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          // 批量选择模式下隐藏操作控件，避免误触；整卡点击切换选中。
          if (!selecting) ...[
            PopupMenuButton<String>(
              tooltip: context.l10n.moreActions,
              icon: Icon(
                Icons.more_horiz,
                size: 18,
                color: AppTheme.textTertiary,
              ),
              color: AppTheme.surface3,
              onSelected: (value) => switch (value) {
                'trusted' => onToggleTrusted(),
                'uninstall' => onUninstall(),
                _ => null,
              },
              itemBuilder: (_) => [
                PopupMenuItem(
                  value: 'trusted',
                  child: Row(
                    children: [
                      Icon(
                        skill.trusted
                            ? Icons.shield_outlined
                            : Icons.verified_user_outlined,
                        size: 16,
                        color: skill.trusted
                            ? AppTheme.textSecondary
                            : AppTheme.accentPrimary,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        skill.trusted
                            ? context.l10n.l6RequireApprovalEachTime673dd
                            : context.l10n.l6TrustNoApproval12269,
                      ),
                    ],
                  ),
                ),
                const PopupMenuDivider(),
                PopupMenuItem(
                  value: 'uninstall',
                  child: Row(
                    children: [
                      Icon(Icons.delete_outline, size: 16),
                      SizedBox(width: 8),
                      Text(context.l10n.l6Uninstall200cf),
                    ],
                  ),
                ),
              ],
            ),
            Switch(value: enabled, onChanged: onToggleEnabled),
          ],
        ],
      ),
    );
    // 选择模式下整卡可点，切换选中状态。
    if (!selecting) return card;
    return InkWell(
      borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      onTap: onToggleSelected,
      child: card,
    );
  }
}
