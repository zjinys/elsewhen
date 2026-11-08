part of '../settings_screen.dart';

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 集成/网络/技能 tab 与 MCP 服务器管理。
extension _Integrations on _SettingsScreenState {
  Future<void> _insertSecretReference(TextEditingController controller) async {
    try {
      final secrets = await api.listNamedSecrets();
      if (!mounted) return;
      final name = await showDialog<String>(
        context: context,
        builder: (context) => SimpleDialog(
          title: Text(context.l10n.l6ChooseASecretReference6a79c),
          children: secrets.isEmpty
              ? [
                  Padding(
                    padding: EdgeInsets.all(24),
                    child: Text(context.l10n.l6AddASecretInSecuritySecretb6b03),
                  ),
                ]
              : secrets
                    .map(
                      (s) => SimpleDialogOption(
                        onPressed: () => Navigator.pop(context, s.name),
                        child: Text(s.name),
                      ),
                    )
                    .toList(),
        ),
      );
      if (name == null) return;
      final selection = controller.selection;
      final start = selection.isValid
          ? selection.start
          : controller.text.length;
      final end = selection.isValid ? selection.end : start;
      final reference = '{{secret:$name}}';
      controller.value = TextEditingValue(
        text: controller.text.replaceRange(start, end, reference),
        selection: TextSelection.collapsed(offset: start + reference.length),
      );
    } catch (e) {
      if (mounted) {
        showAppError(context, context.l10n.load, e);
      }
    }
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
            title: context.l10n.l6PostFetching22fc8,
            icon: Icons.link_outlined,
            child: _buildTweetFetchSettings(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.l6WebSearch678ed,
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
            title: context.l10n.l6OutboundProxyabb5e,
            icon: Icons.vpn_key_outlined,
            child: const ProxySettings(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.l6EncryptedDnsDohc262a,
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
            title: context.l10n.l6UserSkills748e7,
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
            title: context.l10n.l6McpServersad57a,
            icon: Icons.usb_outlined,
            child: _buildMcpContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildMcpContent() {
    final servers = _mcpServers;
    final enabledCount = servers?.where((s) => s.enabled).length ?? 0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.l6ConnectModelContextProtocolServersTo6e5c7,
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
          context.l10n.l6EachToolCallRequiresApprovalBycbb3e,
          style: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 12,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        // 列表头：启用统计 + 批量删除/添加入口同行，服务器多时一眼看清规模
        Row(
          children: [
            Text(
              _mcpBatchMode && servers != null
                  ? context.l10n.l6SelectedArg0Arg1fd75c(
                      (_mcpSelected.length).toString(),
                      (servers.length).toString(),
                    )
                  : servers == null
                  ? context.l10n.loading
                  : context.l10n.l6EnabledArg0Arg1d02a1(
                      (enabledCount).toString(),
                      (servers.length).toString(),
                    ),
              style: TextStyle(
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
                letterSpacing: 0.4,
                color: AppTheme.textTertiary,
              ),
            ),
            const Spacer(),
            if (servers != null && servers.isNotEmpty) ...[
              TextButton.icon(
                onPressed: () => setState(() {
                  _mcpBatchMode = !_mcpBatchMode;
                  _mcpSelected.clear();
                }),
                icon: Icon(
                  _mcpBatchMode
                      ? Icons.close_outlined
                      : Icons.checklist_outlined,
                  size: 15,
                ),
                label: Text(
                  _mcpBatchMode
                      ? context.l10n.l6ExitSelectione23ed
                      : context.l10n.l6DeleteMultiple362ae,
                  style: const TextStyle(fontSize: 12.5),
                ),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                ),
              ),
              if (_mcpBatchMode) ...[
                const SizedBox(width: 4),
                TextButton(
                  onPressed: () => setState(() {
                    if (_mcpSelected.length == servers.length) {
                      _mcpSelected.clear();
                    } else {
                      _mcpSelected
                        ..clear()
                        ..addAll(servers.map((s) => s.id));
                    }
                  }),
                  style: TextButton.styleFrom(
                    visualDensity: VisualDensity.compact,
                  ),
                  child: Text(
                    _mcpSelected.length == servers.length
                        ? context.l10n.l6DeselectAllba20e
                        : context.l10n.l6SelectAll3e44b,
                    style: const TextStyle(fontSize: 12.5),
                  ),
                ),
              ],
            ],
            OutlinedButton.icon(
              onPressed: () => _showMcpServerDialog(),
              icon: const Icon(Icons.add_outlined, size: 16),
              label: Text(
                context.l10n.l6AddServer35085,
                style: TextStyle(fontSize: 12.5),
              ),
              style: OutlinedButton.styleFrom(
                visualDensity: VisualDensity.compact,
              ),
            ),
          ],
        ),
        // 批量操作条：确认删除需二次点击（弹确认框），取消则退出选择模式。
        if (_mcpBatchMode && servers != null && servers.isNotEmpty) ...[
          const SizedBox(height: AppTheme.space2),
          Row(
            children: [
              FilledButton.icon(
                onPressed: _mcpSelected.isEmpty
                    ? null
                    : () => _batchDeleteMcpServers(),
                icon: const Icon(Icons.delete_outline, size: 16),
                label: Text(
                  context.l10n.l6DeleteSelectedArg0def95(
                    (_mcpSelected.length).toString(),
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
                  _mcpBatchMode = false;
                  _mcpSelected.clear();
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
        if (servers == null)
          _McpStatusLine(
            icon: Icons.hourglass_empty_outlined,
            text: context.l10n.l6LoadingServers3023e,
            error: _mcpError,
          )
        else if (servers.isEmpty)
          _McpStatusLine(
            icon: Icons.usb_outlined,
            text: context.l10n.l6NoMcpServersAddedChooseAddac3f0,
          )
        else
          ...servers.map(
            (server) => _McpServerCard(
              server: server,
              selecting: _mcpBatchMode,
              selected: _mcpSelected.contains(server.id),
              onToggleSelected: () => setState(() {
                if (!_mcpSelected.remove(server.id)) {
                  _mcpSelected.add(server.id);
                }
              }),
              onToggleEnabled: (v) => _toggleMcpServer(server.id, v),
              onToggleTrusted: () =>
                  _toggleMcpTrusted(server.id, !server.trusted),
              onToggleReadonly: () => _toggleMcpAllowReadonlyAuto(
                server.id,
                !server.allowReadonlyAuto,
              ),
              onEdit: () => _showMcpServerDialog(existing: server),
              onDelete: () => _deleteMcpServer(server.id),
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
      if (mounted) {
        setState(() {
          _mcpServers = servers;
          _mcpError = null;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(
          () => _mcpError = _settingsFailure(
            context.l10n.l6LoadMcpServers1900c,
            e,
          ),
        );
      }
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
          SnackBar(
            content: Text(context.l10n.l6MarkedAsTrustedThisServerS74676),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: AppErrorMessage(
              action: context.l10n.load,
              error: _settingsFailure(
                context.l10n.l6ChangeTrustSetting7ce93,
                e,
              ),
            ),
          ),
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
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: Text(context.l10n.l6ReadOnlyExemptionEnabledForTools0d43a),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: AppErrorMessage(
              action: context.l10n.load,
              error: _settingsFailure(
                context.l10n.l6ChangeReadOnlyApprovalExemptione8168,
                e,
              ),
            ),
          ),
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
          SnackBar(
            content: AppErrorMessage(
              action: context.l10n.load,
              error: _settingsFailure(context.l10n.l6ToggleMcpServer14dd0, e),
            ),
          ),
        );
      }
    }
  }

  /// 批量删除：先确认，再逐项调删除；单项失败不中断其余项，
  /// 最后汇总报告成功/失败数并重载列表（与技能批量卸载同款交互）。
  Future<void> _batchDeleteMcpServers() async {
    final ids = _mcpSelected.toList();
    if (ids.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.l6DeleteMultipleServers13274),
        content: Text(
          context.l10n.l6ServersToRemoveArg0SyncedCopiesccae5(
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
            child: Text(context.l10n.delete),
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
        await repository.deleteMcpServer(id: id);
        ok++;
      } catch (e) {
        failures.add(id);
        if (mounted) {
          ScaffoldMessenger.maybeOf(context)?.showSnackBar(
            SnackBar(
              content: AppErrorMessage(
                action: context.l10n.load,
                error: _settingsFailure(
                  context.l10n.l6DeleteMcpServerArg0f17d5((id).toString()),
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
      _mcpSelected.clear();
      if (failures.isEmpty) _mcpBatchMode = false;
    });
    await _loadMcpServers();
    if (!mounted) return;
    if (failures.isEmpty) {
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6ServersDeletedArg05fc2a((ok).toString()),
          ),
        ),
      );
    } else {
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6SucceededArg0FailedArg1Arg2Arg312d85(
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

  Future<void> _deleteMcpServer(String id) async {
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.deleteMcpServer(id: id);
      await _loadMcpServers();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: AppErrorMessage(
              action: context.l10n.load,
              error: _settingsFailure(context.l10n.l6DeleteMcpServer34624, e),
            ),
          ),
        );
      }
    }
  }

  Future<void> _showMcpServerDialog({api.McpServerDto? existing}) async {
    bool trusted = existing?.trusted ?? false;
    final nameController = TextEditingController(text: existing?.name ?? '');
    final keywordsController = TextEditingController(
      text: existing?.keywords ?? '',
    );
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
          title: Text(
            existing == null
                ? context.l10n.l6AddMcpServerc9d22
                : context.l10n.l6EditMcpServer4c8df,
          ),
          content: SizedBox(
            width: 480,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  TextField(
                    controller: nameController,
                    decoration: InputDecoration(
                      labelText: context
                          .l10n
                          .l6NameLettersDigitsHyphensOrUnderscores0dc18,
                      hintText: 'filesystem',
                    ),
                  ),
                  const SizedBox(height: AppTheme.space3),
                  TextField(
                    key: const ValueKey('mcp-intent-keywords'),
                    controller: keywordsController,
                    decoration: InputDecoration(
                      labelText:
                          context.l10n.l6RelatedIntentKeywordsOptionalb6b1f,
                      hintText: context
                          .l10n
                          .l6ForExampleProductReviewsRecommendationsSales606d3,
                      helperText:
                          context.l10n.l6SeparateKeywordsWithCommasToHelp6c10b,
                      helperMaxLines: 2,
                    ),
                  ),
                  const SizedBox(height: AppTheme.space3),
                  SegmentedButton<String>(
                    segments: [
                      ButtonSegment(
                        value: 'stdio',
                        label: Text(context.l10n.l6LocalCommandaa556),
                        icon: Icon(Icons.terminal, size: 16),
                      ),
                      ButtonSegment(
                        value: 'streamable-http',
                        label: Text(context.l10n.l6RemoteServer1b481),
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
                    segments: [
                      ButtonSegment(
                        value: 'all',
                        label: Text(context.l10n.l6All778fc),
                      ),
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
                      decoration: InputDecoration(
                        labelText: context.l10n.l6Commandb114b,
                        hintText: 'npx',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: argsController,
                      decoration: InputDecoration(
                        labelText: context.l10n.l6ArgumentsJsonArray5ffa9,
                        hintText: '["-y","@modelcontextprotocol/server-filesystem","/tmp"]',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: envController,
                      decoration: InputDecoration(
                        suffixIcon: IconButton(
                          tooltip: context.l10n.l6InsertASecretReference6edc7,
                          onPressed: () =>
                              _insertSecretReference(envController),
                          icon: const Icon(Icons.key),
                        ),
                        labelText:
                            context.l10n.l6EnvironmentVariablesJsonObject364cf,
                        hintText: '{"NODE_ENV":"production"}',
                      ),
                    ),
                  ] else ...[
                    TextField(
                      controller: urlController,
                      decoration: InputDecoration(
                        labelText: context.l10n.l6EndpointUrl28548,
                        hintText: 'https://mcp.example.com/mcp',
                      ),
                    ),
                    const SizedBox(height: AppTheme.space3),
                    TextField(
                      controller: headersController,
                      decoration: InputDecoration(
                        suffixIcon: IconButton(
                          tooltip: context.l10n.l6InsertASecretReference6edc7,
                          onPressed: () =>
                              _insertSecretReference(headersController),
                          icon: const Icon(Icons.key),
                        ),
                        labelText: context
                            .l10n
                            .l6CustomHeadersJsonObjectAuthenticationEtcd9233,
                        hintText: '{"Authorization":"Bearer …"}',
                      ),
                    ),
                  ],
                  const SizedBox(height: AppTheme.space3),
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(
                      context.l10n.l6TrustedCallsSkipApproval6254b,
                      style: TextStyle(fontSize: 13.5),
                    ),
                    subtitle: Text(
                      context.l10n.l6ToolsFromThisServerExecuteWithoutf61a4,
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
                keywordsController.text,
                platform,
                transport,
                commandController.text,
                argsController.text,
                envController.text,
                urlController.text,
                headersController.text,
              ),
              child: Text(context.l10n.l6TestConnection10b7d),
            ),
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: () {
                Navigator.of(dialogContext).pop(
                  _McpServerFormResult(
                    name: nameController.text,
                    keywords: keywordsController.text,
                    platform: platform,
                    transport: transport,
                    command: commandController.text,
                    argsJson: argsController.text,
                    envJson: envController.text,
                    url: urlController.text,
                    headersJson: headersController.text,
                    trusted: trusted,
                  ),
                );
              },
              child: Text(context.l10n.save),
            ),
          ],
        ),
      ),
    );
    if (result == null) return;
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repository.saveMcpServer(
        api.McpServerDto(
          id: existing?.id ?? '',
          name: result.name,
          keywords: result.keywords,
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
        ),
      );
      await _loadMcpServers();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: AppErrorMessage(
              action: context.l10n.load,
              error: _settingsFailure(context.l10n.l6SaveMcpServer25c8a, e),
            ),
          ),
        );
      }
    }
  }

  Future<void> _testMcpConnection(
    BuildContext dialogContext,
    String name,
    String keywords,
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
        SnackBar(content: Text(context.l10n.l6EnterACommandBeforeTesting41aae)),
      );
      return;
    }
    if (transport == 'streamable-http' && !url.trim().startsWith('http')) {
      ScaffoldMessenger.maybeOf(dialogContext)?.showSnackBar(
        SnackBar(content: Text(context.l10n.l6EnterAnHttpSEndpointBeforec28cd)),
      );
      return;
    }
    final messenger = ScaffoldMessenger.maybeOf(dialogContext);
    final repository =
        ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      final tools = await repository.testMcpServer(
        api.McpServerDto(
          id: 'test-probe',
          allowReadonlyAuto: false,
          name: name.isEmpty ? 'probe' : name,
          keywords: keywords,
          platform: platform,
          transport: transport,
          command: transport == 'stdio' ? command : '',
          argsJson: argsJson,
          envJson: envJson,
          url: transport == 'streamable-http' ? url : '',
          headersJson: headersJson,
          enabled: true,
          trusted: false,
        ),
      );
      if (!mounted) return;
      messenger?.showSnackBar(
        SnackBar(
          content: Text(
            tools.isEmpty
                ? context.l10n.l6ConnectedButTheServerProvidedNod0d33
                : context.l10n.l6ConnectedArg0Arg1c4ea4(
                    (tools.map((t) => t.rawName).take(6).join('、')).toString(),
                    (tools.length > 6
                            ? context.l10n.l6AndArg0MoreTools52a1a(
                                (tools.length).toString(),
                              )
                            : '')
                        .toString(),
                  ),
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      messenger?.showSnackBar(
        SnackBar(
          content: AppErrorMessage(
            action: context.l10n.load,
            error: _settingsFailure(context.l10n.l6TestMcpConnection1eed1, e),
          ),
        ),
      );
    }
  }
}

//! ── MCP 列表视觉组件（只负责呈现与回传动作，逻辑全在上方 extension）──

/// 列表加载中 / 空态：图标 + 说明文字的浅提示容器。
class _McpStatusLine extends StatelessWidget {
  final IconData icon;
  final String text;
  final Object? error;

  const _McpStatusLine({required this.icon, required this.text, this.error});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(vertical: 20, horizontal: 16),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3.withValues(alpha: 0.6)),
      ),
      child: Column(
        children: [
          Icon(icon, size: 26, color: AppTheme.textTertiary),
          const SizedBox(height: 8),
          if (error != null)
            AppErrorMessage(error: error!, action: context.l10n.load)
          else
            Text(
              text,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 12.5,
                height: 1.6,
                color: AppTheme.textTertiary,
              ),
            ),
        ],
      ),
    );
  }
}

/// 单台 MCP 服务器卡片：
/// - 左侧传输类型图标（终端=本机命令 / 公网=远程），禁用时整体降饱和；
/// - 标题行：名称 + 传输徽章 + 平台徽章 + 信任/只读状态胶囊；
/// - 副标题：命令+参数 或 端点 URL，等宽省略展示；
/// - 尾部：启用开关 + 溢出菜单（编辑/信任/只读免审批/删除），不再平铺 5 个图标。
class _McpServerCard extends StatelessWidget {
  final api.McpServerDto server;
  final ValueChanged<bool> onToggleEnabled;
  final VoidCallback onToggleTrusted;
  final VoidCallback onToggleReadonly;
  final VoidCallback onEdit;
  final VoidCallback onDelete;

  /// 批量选择模式：为 true 时尾部操作控件换成勾选框，整卡点击切换选中。
  final bool selecting;
  final bool selected;
  final VoidCallback? onToggleSelected;

  const _McpServerCard({
    required this.server,
    required this.onToggleEnabled,
    required this.onToggleTrusted,
    required this.onToggleReadonly,
    required this.onEdit,
    required this.onDelete,
    this.selecting = false,
    this.selected = false,
    this.onToggleSelected,
  });

  bool get _isHttp => server.transport == 'streamable-http';

  String get _endpoint =>
      _isHttp ? server.url : '${server.command} ${server.argsJson}'.trim();

  @override
  Widget build(BuildContext context) {
    final enabled = server.enabled;
    // 主色随状态：启用=强调色，停用=灰；信任中的服务器再用图标点亮；
    // 批量选择模式下选中卡边框高亮。
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
          // 传输类型图标；批量选择模式下换成勾选框。
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
              child: Icon(
                _isHttp ? Icons.public_outlined : Icons.terminal_outlined,
                size: 18,
                color: leadColor,
              ),
            ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 标题行：名称 + 徽章
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        server.name,
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
                    const SizedBox(width: 6),
                    _McpBadge(
                      label: _isHttp
                          ? context.l10n.l6Remote80194
                          : context.l10n.l6LocalCommandaa556,
                      color: AppTheme.textSecondary,
                    ),
                    const SizedBox(width: 4),
                    // 平台徽章：同步分发的适用范围（迁移 48）。
                    _McpBadge(
                      label: server.platform == 'all'
                          ? context.l10n.l6AllPlatforms52e2a
                          : context.l10n.l6OnlyArg064396(
                              (server.platform).toString(),
                            ),
                      color: AppTheme.textTertiary,
                    ),
                    if (server.trusted) ...[
                      const SizedBox(width: 4),
                      _McpBadge(
                        label: context.l10n.l6TrustedNoApproval69302,
                        color: AppTheme.accentPrimary,
                        icon: Icons.verified_user_outlined,
                      ),
                    ] else if (server.allowReadonlyAuto) ...[
                      const SizedBox(width: 4),
                      _McpBadge(
                        label: context.l10n.l6ReadOnlyCallsSkipApproval63678,
                        color: AppTheme.success,
                        icon: Icons.lock_open_outlined,
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 3),
                // 端点：命令+参数 或 URL，单行省略
                Text(
                  _endpoint,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11.5,
                    color: enabled
                        ? AppTheme.textSecondary
                        : AppTheme.textTertiary,
                  ),
                ),
                if (server.keywords.isNotEmpty) ...[
                  const SizedBox(height: 3),
                  Text(
                    context.l10n.l6RelatedIntentsArg0dff5f(
                      (server.keywords).toString(),
                    ),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(width: 8),
          // 溢出菜单：低频操作（编辑/审批策略/删除）收进来，列表不再横排 5 个图标；
          // 批量选择模式下隐藏操作控件，避免误触。
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
                'edit' => onEdit(),
                'trusted' => onToggleTrusted(),
                'readonly' => onToggleReadonly(),
                'delete' => onDelete(),
                _ => null,
              },
              itemBuilder: (_) => [
                PopupMenuItem(
                  value: 'edit',
                  child: Row(
                    children: [
                      Icon(Icons.edit_outlined, size: 16),
                      SizedBox(width: 8),
                      Text(context.l10n.edit),
                    ],
                  ),
                ),
                PopupMenuItem(
                  value: 'trusted',
                  child: Row(
                    children: [
                      Icon(
                        server.trusted
                            ? Icons.shield_outlined
                            : Icons.verified_user_outlined,
                        size: 16,
                        color: server.trusted
                            ? AppTheme.textSecondary
                            : AppTheme.accentPrimary,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        server.trusted
                            ? context.l10n.l6RequireApprovalEachTime673dd
                            : context.l10n.l6TrustNoApproval12269,
                      ),
                    ],
                  ),
                ),
                PopupMenuItem(
                  value: 'readonly',
                  child: Row(
                    children: [
                      Icon(
                        server.allowReadonlyAuto
                            ? Icons.lock_outlined
                            : Icons.lock_open_outlined,
                        size: 16,
                        color: server.allowReadonlyAuto
                            ? AppTheme.textSecondary
                            : AppTheme.success,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        server.allowReadonlyAuto
                            ? context
                                  .l10n
                                  .l6DisableReadOnlyApprovalExemptioned6aa
                            : context
                                  .l10n
                                  .l6EnableReadOnlyApprovalExemption95d1c,
                      ),
                    ],
                  ),
                ),
                const PopupMenuDivider(),
                PopupMenuItem(
                  value: 'delete',
                  child: Row(
                    children: [
                      Icon(Icons.delete_outline, size: 16),
                      SizedBox(width: 8),
                      Text(context.l10n.delete),
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

/// 小号语义色徽章（可带图标）：名称右侧的传输/平台/审批策略标签。
class _McpBadge extends StatelessWidget {
  final String label;
  final Color color;
  final IconData? icon;

  const _McpBadge({required this.label, required this.color, this.icon});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1.5),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 10, color: color),
            const SizedBox(width: 2),
          ],
          Text(
            label,
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.w600,
              color: color,
            ),
          ),
        ],
      ),
    );
  }
}
