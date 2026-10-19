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
}
