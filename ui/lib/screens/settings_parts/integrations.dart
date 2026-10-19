part of '../settings_screen.dart';
// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 集成/网络/技能 tab 与 MCP 服务器管理。
extension _Integrations on _SettingsScreenState {
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
}
