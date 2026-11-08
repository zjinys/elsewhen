// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appTitle => 'Elsewhen';

  @override
  String get initializationFailed => '初始化失败';

  @override
  String initializationError(String error) {
    return '初始化失败：$error';
  }

  @override
  String get startingApp => '正在启动…';

  @override
  String get language => '语言';

  @override
  String get followSystem => '跟随系统';

  @override
  String get chinese => '简体中文';

  @override
  String get english => 'English';

  @override
  String get save => '保存';

  @override
  String get saving => '保存中…';

  @override
  String get cancel => '取消';

  @override
  String get delete => '删除';

  @override
  String get edit => '编辑';

  @override
  String get add => '添加';

  @override
  String get close => '关闭';

  @override
  String get confirm => '确认';

  @override
  String get retry => '重试';

  @override
  String get settings => '设置';

  @override
  String get backToMain => '返回主界面';

  @override
  String get conversation => '对话';

  @override
  String get knowledge => '知识';

  @override
  String get goals => '目标';

  @override
  String get review => '回顾';

  @override
  String get sync => '同步';

  @override
  String get today => '今天';

  @override
  String get recentlyUsed => '最近使用';

  @override
  String get noRecentPages => '还没有最近使用的页面';

  @override
  String get conversationLoadFailed => '对话加载失败';

  @override
  String get lastSevenDays => '近 7 天';

  @override
  String get rejectedSources => '已否定（负知识）';

  @override
  String get noRejectedSources => '还没有否定的来源。';

  @override
  String get models => '模型';

  @override
  String get appearance => '外观';

  @override
  String get integrations => '集成';

  @override
  String get network => '网络';

  @override
  String get skills => '技能';

  @override
  String get habits => '习惯';

  @override
  String get data => '数据';

  @override
  String get rules => '规则';

  @override
  String get security => '安全';

  @override
  String get aiProviders => 'AI 提供商';

  @override
  String get memoryStrategy => '记忆策略';

  @override
  String get prompts => '提示词';

  @override
  String get memorySaved => '记忆策略已保存';

  @override
  String get strategyType => '策略类型';

  @override
  String get simpleMemory => '简单记忆 (最近 N 条)';

  @override
  String get slidingWindowMemory => '滑动窗口 (Token 限制)';

  @override
  String get maximumMessages => '最大消息数';

  @override
  String get memoryBudget => '对话记忆预算（可选）';

  @override
  String get memoryBudgetHint => '留空按 Provider 上下文窗口推算';

  @override
  String get themeMode => '主题模式';

  @override
  String get themeColors => '主题配色';

  @override
  String get lightTheme => '浅色';

  @override
  String get darkTheme => '深色';

  @override
  String get amberTheme => '琥珀';

  @override
  String get indigoTheme => '靛蓝';

  @override
  String get aquaTheme => '青碧';

  @override
  String get violetTheme => '郁金';

  @override
  String get bodyFontSize => '正文字号';

  @override
  String get contentFont => '内容字体';

  @override
  String get contentFontDescription => '作用于知识库正文、对话消息等内容区；界面文字使用固定字体';

  @override
  String get systemDefault => '系统默认';

  @override
  String get desktopWindow => '桌面窗口';

  @override
  String get noTrayAvailable => '当前环境没有可用的系统托盘，关闭窗口将直接退出应用。';

  @override
  String get onWindowClose => '关闭窗口时';

  @override
  String get minimizeToTray => '最小化到托盘';

  @override
  String get quitApp => '退出应用';

  @override
  String get stayInTrayDescription => '应用留在后台托盘，可随时从托盘菜单唤回';

  @override
  String get quitDescription => '彻底退出应用，托盘图标一并移除';

  @override
  String get pending => '待处理';

  @override
  String get processing => '处理中';

  @override
  String get waitingRetry => '等待重试';

  @override
  String get completed => '已完成';

  @override
  String get failed => '失败';

  @override
  String get skipped => '跳过';

  @override
  String get lastSuccess => '最近成功';

  @override
  String get analysisStatusFailed => '分析队列状态读取失败';

  @override
  String get digestStatusFailed => '知识消化状态读取失败';

  @override
  String get digestDescription => '记录的事件会在后台自动整理进知识库；失败项冷却后会自动重试。';

  @override
  String get viewQueueLogs => '查看队列与运行日志';

  @override
  String get exporting => '导出中…';

  @override
  String get chooseExportFolder => '选择目录并导出';

  @override
  String get exportFolder => '选择导出目录';

  @override
  String get storageExportUnsupported => '当前存储后端不支持导出';

  @override
  String get loading => '读取中…';

  @override
  String get compacting => '整理中…';

  @override
  String get compactNow => '立即整理';

  @override
  String get databaseAlreadyCompact => '整理完成，库已经很紧凑';

  @override
  String get selectSecret => '选择密钥';

  @override
  String get noSecretsHint => '请先在设置 → 安全 → 密钥管理中添加密钥';

  @override
  String get secretLoadFailed => '密钥库读取失败，请重试';

  @override
  String get duplicateProvider => '复制 AI Provider';

  @override
  String get addProvider => '添加 AI Provider';

  @override
  String get editProvider => '编辑 Provider';

  @override
  String get configurationNameRequired => '配置名称 *';

  @override
  String get configurationNameHint => '如：主用 GPT';

  @override
  String get providerType => '提供商类型';

  @override
  String get openAiCompatible => 'OpenAI 兼容';

  @override
  String get modelRequired => '模型 *';

  @override
  String get secretVault => '密钥库';

  @override
  String get manualInput => '手动输入';

  @override
  String get apiKeyHintNew => '输入 API Key，或点击钥匙选择';

  @override
  String get apiKeyHintExisting => '留空保持当前密钥不变';

  @override
  String get chooseFromVault => '从密钥库选择';

  @override
  String get contextWindow => '上下文窗口（token）';

  @override
  String get contextWindowHint => '留空使用保守默认值；中转可填写实测上限';

  @override
  String get maximumOutput => '输出上限（token）';

  @override
  String get optional => '可选';

  @override
  String get selectSecretFirst => '请先选择一个密钥';

  @override
  String get saveProvider => '保存 Provider';

  @override
  String get noProviders => '尚未配置任何 AI Provider，点击右上角「添加」开始。';

  @override
  String get active => '激活中';

  @override
  String get deleteConfiguration => '删除配置';

  @override
  String get addSecret => '添加密钥';

  @override
  String get editSecret => '编辑密钥';

  @override
  String get secretCodeRequired => '唯一代号 *';

  @override
  String get secretCodeCreateHint => '字母、数字、下划线或短横线，创建后固定';

  @override
  String get secretCodeEditHint => '引用标识不可修改，名称可随时调整';

  @override
  String get nameRequired => '名称 *';

  @override
  String get secretNameHint => '例如：个人 GitHub Token';

  @override
  String get secretValueRequired => '密钥值 *';

  @override
  String get newSecretValueOptional => '新密钥值（可选）';

  @override
  String get keepSecretValueHint => '留空保留原密钥，仅更新名称';

  @override
  String get replaceMatchingCredentials => '将相同的现有凭据替换为引用';

  @override
  String get matchingCredentialsHint => '精确匹配 MCP 环境变量、Header 和搜索 Key';

  @override
  String get notReferenced => '尚未引用';

  @override
  String get nameRequiredError => '请填写名称';

  @override
  String get secretFieldsRequired => '请填写唯一代号和密钥值';

  @override
  String get deleteSecretTitle => '删除密钥？';

  @override
  String get secretVaultDescription =>
      '密钥仅保存在本机。在 MCP 环境变量、Header 或搜索 API Key 中填写 {{secret:代号}} 即可引用。';

  @override
  String get promptBody => '提示词正文';

  @override
  String get promptEditHint => '保存只产生草稿，启用影响下一次运行。协议与执行权限由程序约束。';

  @override
  String get promptReadOnlyHint => '输出协议与执行纪律只读。';

  @override
  String get promptDraftSaved => '草稿已保存，可点击启用。';

  @override
  String get viewDefault => '查看默认';

  @override
  String get settingsEditNote => '设置页编辑';

  @override
  String get saveDraft => '保存草稿';

  @override
  String get activateDraft => '启用草稿';

  @override
  String get builtin => '内置';

  @override
  String get custom => '自定义';

  @override
  String get currentlyActive => '当前生效';

  @override
  String get activateRevision => '启用此版本';

  @override
  String get recentRunVersions => '最近20次运行版本';

  @override
  String get promptLocalDescription =>
      '提示词保存在本机。角色和处理方法支持修订与回滚；固定协议只读。当前使用中文资源。';

  @override
  String get loadPrompts => '加载提示词';

  @override
  String get viewRunVersions => '查看运行版本';

  @override
  String get editable => '可编辑';

  @override
  String get readOnlyContract => '只读协议';

  @override
  String get revisionHistory => '版本历史与回滚';

  @override
  String get restoreDefault => '恢复默认';

  @override
  String get syncPlatformUnavailable => '同步在此平台暂不可用';

  @override
  String get todos => '待办';

  @override
  String get drafts => '草稿';

  @override
  String get search => '搜索';

  @override
  String get lockedTitle => 'Elsewhen 已锁定';

  @override
  String get incorrectPassword => '密码不正确';

  @override
  String get secureStorageUnavailable => '安全存储暂不可用，请稍后重试';

  @override
  String get passwordInputHint => '输入打开密码，输完按回车';

  @override
  String get verifying => '验证中…';

  @override
  String get biometricUnlock => '使用指纹/面容解锁';

  @override
  String get usePinEntry => '改用 4 格输入';

  @override
  String get legacyPasswordEntry => '旧密码不是 4 位？换输入方式';

  @override
  String sectionLoadFailed(String section, String error) {
    return '$section读取失败：$error';
  }

  @override
  String secretUsedBy(String references) {
    return '使用位置：$references';
  }

  @override
  String deleteSecretConfirmation(String name) {
    return '删除 $name？';
  }

  @override
  String secretStillUsedBy(String references) {
    return '仍被 $references 引用，删除后相关服务将无法连接。';
  }

  @override
  String deleteProviderConfirmation(String name, String warning) {
    return '确定删除 Provider「$name」吗？$warning';
  }

  @override
  String get activeProviderDeletionWarning => '该配置正处于激活状态，删除后剩余第一个配置将自动激活。';

  @override
  String providerCopyName(String name) {
    return '$name 副本';
  }

  @override
  String get switchProvider => '切换 Provider';

  @override
  String get deleteProvider => '删除 Provider';

  @override
  String promptHistoryTitle(String title) {
    return '$title · 最近30个版本';
  }

  @override
  String approximateTokens(int tokens) {
    return '约 $tokens token';
  }

  @override
  String get promptPersonalPolicy => '对话角色与风格';

  @override
  String get promptChatContract => '对话协议与执行纪律';

  @override
  String get promptSummaryContract => '历史摘要协议';

  @override
  String get promptReadingContract => '全文阅读协议';

  @override
  String get promptDigestContract => '知识消化协议';

  @override
  String get promptSummaryPolicy => '历史摘要重点';

  @override
  String get promptReadingPolicy => '全文阅读重点';

  @override
  String get promptDigestPolicy => '知识消化方法';

  @override
  String get promptAnalysisPolicy => '事件分析方法';

  @override
  String lastFailure(String time, String error) {
    return '最近失败（$time）：$error';
  }

  @override
  String exportSuccess(int count, String folder) {
    return '已导出 $count 个知识页到 $folder';
  }

  @override
  String get wikiExportDescription =>
      '把知识页导出为带元数据的 Markdown 文件（Obsidian 等通用工具可直接打开）；派生产物与归档/拒绝页不导出，同名覆盖可重复执行。';

  @override
  String databaseSize(String file, String free, String advice) {
    return '库文件 $file MB，其中可回收 $free MB（历史删除残留）$advice';
  }

  @override
  String get compactionRecommended => '。建议整理。';

  @override
  String databaseSpaceReclaimed(String size) {
    return '整理完成，回收了 $size MB';
  }

  @override
  String get noAiUsage => '暂无 AI 调用记录。发起对话并生成 AI 回复后，这里会按天统计 token 用量。';

  @override
  String get last30DaysTokens => '近 30 天 Total';

  @override
  String get totalCalls => '累计调用';

  @override
  String callCount(int count) {
    return '$count 次';
  }

  @override
  String get listSeparator => '、';

  @override
  String get providerConfiguration => 'AI Provider 配置';

  @override
  String get dailyTokenUsage => '每日 Token 用量';

  @override
  String get analysisQueue => '事件分析队列';

  @override
  String get knowledgeProcessing => '知识消化';

  @override
  String get ruleLibrary => '规则库';

  @override
  String get tweetFetchService => '推文抓取服务';

  @override
  String get providerInUse => '对话使用此配置';

  @override
  String get activateProvider => '设为激活';

  @override
  String get duplicateProviderTooltip => '复制配置（API Key 需重新输入）';

  @override
  String get keyNotConfigured => '未配置 Key';

  @override
  String get notConfigured => '未配置';

  @override
  String get providerNameMissing => '配置名称不能为空';

  @override
  String get providerFieldsMissing => 'base_url 和 model 均不能为空';

  @override
  String get apiKeyRequiredForNew => '新建配置时必须填写 API Key';

  @override
  String get chooseFont => '选择字体';

  @override
  String get searchFonts => '搜索字体…';

  @override
  String get clear => '清除';

  @override
  String get followGlobal => '跟随全局';

  @override
  String get useGlobalFont => '使用全局字体设置';

  @override
  String get useSystemFont => '跟随操作系统，不指定字体';

  @override
  String get noMatchingFonts => '没有匹配的字体';

  @override
  String get fontPreview => '中文测试 AaBbGg 1234 · ';

  @override
  String get endpointMalformed => 'Provider 地址格式需要检查；不会自动修正地址或发送连接测试。';

  @override
  String get endpointLoopback =>
      '这是本机环回地址：在另一台设备上会指向那台设备本身，而非原设备。同步保留原地址，不会自动探测或改用其他 Provider。';

  @override
  String get endpointUnspecified =>
      '这是未指定地址，通常用于服务监听；请确认本机及其他设备实际可访问的 Provider 地址。不会自动替换或探测。';

  @override
  String get endpointPrivate =>
      '这是内网或链路本地地址，其他设备需要相应的网络连接。同步保留原配置，尚未验证可达性，也不会自动发送模型请求。';

  @override
  String operationFailed(String action) {
    return '$action失败，请重试一次';
  }

  @override
  String localFontCount(int count) {
    return '本地字体 ($count)';
  }

  @override
  String get secretManagement => '密钥管理';

  @override
  String get appPassword => '打开密码';

  @override
  String get openNavigation => '打开导航';

  @override
  String get createTopic => '创建主题';

  @override
  String get importUrl => '网址导入';

  @override
  String get hideSearch => '隐藏搜索';

  @override
  String get searchAndDate => '搜索与日期定位';

  @override
  String get aiReplying => '回复中';

  @override
  String get aiReplyingHint => 'AI 正在生成回复';

  @override
  String get aiAnalyzing => '分析中';

  @override
  String get aiAnalyzingHint => 'AI 正在分析记录';

  @override
  String get aiDigesting => '消化中';

  @override
  String get aiDigestingHint => 'AI 正在消化新记录';

  @override
  String get aiOrganizing => '整理中';

  @override
  String get aiOrganizingHint => 'AI 正在整理知识库';

  @override
  String get aiChecking => '检测中';

  @override
  String get aiCheckingHint => 'AI 正在检测周期报告';

  @override
  String goalsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '目标（$count）',
      zero: '目标',
    );
    return '$_temp0';
  }

  @override
  String todoCount(int count) {
    return '待办（$count）';
  }

  @override
  String get locateByDate => '按日期定位';

  @override
  String get searchMessages => '搜索你说过的话';

  @override
  String get clearLocation => '清除定位';

  @override
  String get backToNow => '回到现在';

  @override
  String get jumpToDate => '跳到日期';

  @override
  String get jump => '跳转';

  @override
  String get showEarlierMessages => '显示更早的消息';

  @override
  String get startConversation => '选择一个对话开始聊天';

  @override
  String get noMessages => '还没有消息';

  @override
  String get startTyping => '在下方输入框开始对话';

  @override
  String conversationCount(int count) {
    return '对话$count次';
  }

  @override
  String todoCountItems(int count) {
    return '待办$count项';
  }

  @override
  String get copyAll => '复制全部对话';

  @override
  String get flipPast => '翻篇记录';

  @override
  String get topics => '主题';

  @override
  String get systemNotice => '系统提示';

  @override
  String get copyMessage => '复制';

  @override
  String get saveAsTopic => '存为主题';

  @override
  String get saveAsTopicDescription => '提取标题与正文，确认后存入主题库';

  @override
  String get regenerate => '重新生成';

  @override
  String get recordCategory => '记录分类';

  @override
  String get record => '记录';

  @override
  String get discussion => '讨论';

  @override
  String get includeInRecords => '纳入记录';

  @override
  String get asDiscussion => '作为讨论';

  @override
  String get reanalyze => '重新分析';

  @override
  String get savedAsRecord => '已纳入记录';

  @override
  String get savedAsDiscussion => '已作为讨论，不再进入日流和长期事实';

  @override
  String get reanalyzeQueued => '已加入重新分析队列';

  @override
  String get saveMessageAsTopic => '把这条内容存为主题';

  @override
  String get thinking => 'AI 正在思考…';

  @override
  String get startGenerating => '正在启动生成…';

  @override
  String get readingGeneration => '正在读取生成状态…';

  @override
  String get generationFailedSaved => '生成失败，输入已保存';

  @override
  String get generationStopped => '已停止生成';

  @override
  String get generationInterrupted => '生成已中断';

  @override
  String get stopping => '正在停止…';

  @override
  String get stopGeneration => '停止生成';

  @override
  String get regenerateThis => '重新生成本条';

  @override
  String get reconnect => '重新连接';

  @override
  String actualToolSteps(int count) {
    return '实际工具步骤（最近 $count 项）';
  }

  @override
  String get chooseDirectory => '选择要导入的目录';

  @override
  String get selectConversation => '请先选择一个会话';

  @override
  String get noSupplementTask => '当前没有可补充的任务，请选择保存或排队。';

  @override
  String sendFailed(String error) {
    return '发送失败：$error';
  }

  @override
  String get savedButNotEligible => '输入已保存，但生成资格未确认。请从该消息继续生成，不要重新发送。';

  @override
  String enteredTopic(String title) {
    return '已进入主题：$title';
  }

  @override
  String get thinkingNext => 'AI 正在思考，您可以先输入下一条消息…';

  @override
  String get inputMessage => '输入消息...';

  @override
  String get closeInputFormat => '关闭输入格式';

  @override
  String get addContent => '添加内容';

  @override
  String get selectDirectory => '选择目录';

  @override
  String get viewInputFormat => '查看输入格式';

  @override
  String get queueNext => '保存并排队下一问';

  @override
  String get send => '发送';

  @override
  String get recordOnlyHelp => '只保存为记录，不请求 AI';

  @override
  String get formatDirectory => '导入项目或文件';

  @override
  String get formatDirectoryHelp => '看看 `/path/to/project` 下的项目，导入到知识库';

  @override
  String get formatConfirm => '确认 / 取消';

  @override
  String get formatConfirmHelp => '处理 AI 草案';

  @override
  String get mentionPerson => '@人名';

  @override
  String get tagTopic => '#主题名';

  @override
  String get link => '链接';

  @override
  String get directory => '目录';

  @override
  String get unverifiedDraft => '未核验草稿 · 不代表操作成功或引用有效';

  @override
  String get toolRunning => '进行中';

  @override
  String get toolUncertainNoRetry => '结果待核实；不会自动重做';

  @override
  String get toolReadCompleted => '查询已完成';

  @override
  String get toolProposalReady => '草稿已生成，等待审阅；尚未执行';

  @override
  String get toolExecuted => '操作已执行（业务回执）';

  @override
  String get toolFailed => '执行失败';

  @override
  String get toolUncertain => '结果待核实';

  @override
  String get toolListRules => '查询规则';

  @override
  String get toolListPages => '浏览知识页';

  @override
  String get toolReadPage => '读取知识页';

  @override
  String get toolUsage => '查询用量';

  @override
  String get toolSearchKnowledge => '检索知识库';

  @override
  String get toolReadLink => '读取链接';

  @override
  String get toolReadWeb => '读取网页';

  @override
  String get toolRecordEvent => '记录事件';

  @override
  String get toolDraftPage => '草拟知识页';

  @override
  String get toolSkillScript => '执行技能脚本';

  @override
  String get toolMcp => 'MCP 工具调用';

  @override
  String get toolListTodos => '查询待办';

  @override
  String get toolDraftTodo => '草拟待办';

  @override
  String get toolDraftRelations => '草拟联系人关联';

  @override
  String get toolExtractRelations => '整理联系人关联';

  @override
  String get toolEventsByDate => '按日期查询事件';

  @override
  String get toolArchiveChats => '准备归档对话';

  @override
  String get toolRenamePage => '准备知识页重命名';

  @override
  String get toolImportSource => '准备来源导入';

  @override
  String get toolInspectDirectory => '分析项目目录';

  @override
  String get toolImportFiles => '准备目录文件导入';

  @override
  String get toolRevisePage => '草拟知识页修订';

  @override
  String get toolStep => '处理工具步骤';

  @override
  String get gerundPreparing0 => '翻阅记忆中…';

  @override
  String get gerundPreparing1 => '整理思路中…';

  @override
  String get gerundPreparing2 => '回顾对话中…';

  @override
  String get gerundPreparing3 => '翻检知识库中…';

  @override
  String get gerundPreparing4 => '搜集线索中…';

  @override
  String get gerundPreparing5 => '梳理上下文中…';

  @override
  String get gerundAwaiting0 => '正在酝酿…';

  @override
  String get gerundAwaiting1 => '正在构思…';

  @override
  String get gerundAwaiting2 => '正在推敲…';

  @override
  String get gerundAwaiting3 => '正在斟酌…';

  @override
  String get gerundAwaiting4 => '灵感涌现中…';

  @override
  String get gerundAwaiting5 => '编织回复中…';

  @override
  String get gerundAwaiting6 => '打磨措辞中…';

  @override
  String get gerundAwaiting7 => '抽丝剥茧中…';

  @override
  String get copyThisMessage => '复制这条消息';

  @override
  String get message => '消息';

  @override
  String get me => '我';

  @override
  String get updateMessageStatus => '更新消息状态';

  @override
  String get eventAnalyzing => '事件正在分析中，请稍后再试';

  @override
  String get inputGuidePerson => '标注联系人';

  @override
  String get inputGuideTopic => '进入主题';

  @override
  String get inputGuideLink => '导入内容';

  @override
  String get aiProviderMissing => '尚未配置 AI Provider，请到设置页填写后重试';

  @override
  String copiedItem(String label) {
    return '已复制$label';
  }

  @override
  String importDirectoryPrompt(String path) {
    return '看看 `$path` 下的项目，导入到知识库';
  }

  @override
  String messageLoadFailed(String error) {
    return '加载消息失败\n$error';
  }

  @override
  String aiReplyFailed(String error) {
    return 'AI 回复失败：$error';
  }

  @override
  String get goalCoAuthorReady => '已带目标上下文到「目标」页的 AI 输入框，确认或修改后发送即可共同拆解';

  @override
  String get goalStudio => 'AI 立项工作室';

  @override
  String get newItem => '新建';

  @override
  String get newGoal => '新建目标';

  @override
  String get goalManageInPage => '管理与 AI 讨论在「目标」页';

  @override
  String get goalStudioHelp =>
      '把一句意图交给 AI（例：「3 年内月收入 10 万」）：它先给现实性评估与假设清单，再拆成一条长程计划与由近及远的里程碑步骤，每步都带可验证判据；落库前都要你确认。';

  @override
  String get goalTreeHelp => '计划确认后出现在左侧目标树。在左侧选中某个目标，这里会切换成那条目标的专属讨论。';

  @override
  String get noActiveGoals => '还没有活跃目标。在「目标」页可新建或与 AI 共同立项。';

  @override
  String get planWithAi => '和 AI 聊聊，共同立项';

  @override
  String get reviewGoalsWithAi => '和 AI 回顾 / 立项';

  @override
  String get noGoalsInStatus => '所选状态下暂无目标';

  @override
  String get noActiveGoalsCreate => '还没有活跃目标，点右上「新建」，或和 AI 共同立项';

  @override
  String get goalCapHint => '活跃已满，新目标将先放入「暂停搁置」';

  @override
  String get newGoalHint => '设立新目标愿景…（如：发布 Elsewhen 1.0）';

  @override
  String get collapseGoalContext => '收起目标背景';

  @override
  String get addGoalContext => '补充动力/行动/困难';

  @override
  String get goalWhyOptional => '💡 为什么做 / 核心动力（可选）';

  @override
  String get goalWhyHint => '如：验证个人认知系统的完整闭环';

  @override
  String get goalActionOptional => '📍 接下来一两周要做的事（可选）';

  @override
  String get goalActionHint => '如：攻克多端同步快照收敛';

  @override
  String get goalObstacleOptional => '🚧 当前困难 / 困难（可选）';

  @override
  String get goalObstacleHint => '如：真机证书配置较繁琐';

  @override
  String get goalDoneOptional => '🎯 验收标准（可选）';

  @override
  String get goalDoneHint => '如：多端测试全绿通过';

  @override
  String get collapsePhase => '收起阶段';

  @override
  String get expandPhase => '展开阶段';

  @override
  String get currentPhase => '当前阶段';

  @override
  String get moreActions => '更多操作';

  @override
  String get reactivateGoal => '重新激活（读档唤醒）';

  @override
  String get adjustGoalAction => '调整当前行动 / 困难';

  @override
  String get completeGoal => '达成目标（归档为成就）';

  @override
  String get pauseGoal => '暂时搁置（随时读档复活）';

  @override
  String get editGoalDetails => '编辑全貌';

  @override
  String get currentActionLabel => '当前行动：';

  @override
  String get currentObstacleLabel => '当前困难：';

  @override
  String get goalActionNotSet => '还没确定，点击写下接下来一两周要做的事';

  @override
  String get nextPhaseHint => '攻克上一阶段后，将注意力收敛到近 1~2 周最先启动的下一个阶段。';

  @override
  String get currentActionField => '📍 当前行动';

  @override
  String get nextActionHint => '如：完成首版同步快照算法并跑通测试';

  @override
  String get clearObstacleField => '🚧 当前困难（可留空清除）';

  @override
  String get nextObstacleHint => '如：等待对方确认网络穿透端口';

  @override
  String get updateAction => '更新行动';

  @override
  String get restartGoalHint => '重新启动此目标，请确认或设定接下来的第一具体行动：';

  @override
  String get restartActionField => '📍 重新具体行动';

  @override
  String get restartActionHint => '如：理清代码依赖并设计重构方案';

  @override
  String get restartObstacleField => '🚧 当前当前困难（可选）';

  @override
  String get restartObstacleHint => '如：缺少相关设备环境';

  @override
  String get confirmReactivate => '确认唤醒';

  @override
  String get editFullGoal => '编辑目标全貌';

  @override
  String get goalVision => '目标愿景内容';

  @override
  String get goalWhy => '💡 为什么做 / 核心动力';

  @override
  String get goalNextAction => '📍 接下来一两周要做的事';

  @override
  String get goalObstacle => '🚧 当前困难 / 困难';

  @override
  String get goalDone => '🎯 验收判据 / 完成标准';

  @override
  String get saveChanges => '保存修改';

  @override
  String get goalActionSaved => '已更新行动与困难';

  @override
  String get goalDetailsSaved => '已保存目标全貌';

  @override
  String get collapseDiscussion => '收起讨论';

  @override
  String get goalStudioInputHint => '说说你的长程意图…（草稿不会被自动发送）';

  @override
  String get goalChatInputHint => '和 AI 聊聊这个目标…（草稿不会被自动发送）';

  @override
  String get generateReply => '生成回复';

  @override
  String get goalStudioWelcome =>
      '🎯 说说你想实现的长程意图（比如「3 年内月收入 10 万」）。我会先给现实性评估与假设清单，再拆成一条长程计划与由近及远的里程碑步骤，落库前都要你确认。';

  @override
  String get goalChatWelcome =>
      '🎯 我们来聊聊这个目标。我可以帮你拆解行动、检视困难、回顾完成标准；需要暂停、标记达成或重启目标时，我会先给出提议，等你确认后才生效。';

  @override
  String goalPaused(String goal) {
    return '已将「$goal」暂时搁置';
  }

  @override
  String goalCompletedToast(String goal) {
    return '🎉 达成目标「$goal」！';
  }

  @override
  String goalReactivated(String goal) {
    return '已唤醒「$goal」并设为活跃目标';
  }

  @override
  String goalCapacity(int count) {
    return '当前已有 $count 个活跃目标（精力上限）。请先暂停或达成一个，再腾出精力唤醒此目标。';
  }

  @override
  String activeGoalCount(int count, int maximum) {
    return '活跃目标 $count / $maximum';
  }

  @override
  String goalLoadFailed(String error) {
    return '目标加载失败：$error';
  }

  @override
  String newGoalPaused(String goal) {
    return '活跃已满，新愿景「$goal」已放入「暂停搁置」';
  }

  @override
  String newGoalCreated(String goal) {
    return '已设立新愿景「$goal」';
  }

  @override
  String newGoalCapacityHint(int count) {
    return '活跃已满 $count 个，新目标将以「暂停搁置」状态创建，可随时读档唤醒。';
  }

  @override
  String whyGoal(String why) {
    return '为什么做：$why';
  }

  @override
  String doneGoal(String done) {
    return '怎样算完成：$done';
  }

  @override
  String adjustActionTitle(String goal) {
    return '调整当前行动 · $goal';
  }

  @override
  String reactivateGoalTitle(String goal) {
    return '读档唤醒 · $goal';
  }

  @override
  String motivationReview(String why) {
    return '💡 核心动力回顾：$why';
  }

  @override
  String goalDiscussionTitle(String goal) {
    return '目标讨论 · $goal';
  }

  @override
  String goalNewSeed(String brief) {
    return '我想立一个新的长程目标。请用几个设问帮我明确：为什么做、什么算完成、近 1~2 周的第一具体行动是什么、可能卡在哪。$brief';
  }

  @override
  String goalReviewSeed(String brief) {
    return '帮我看看这些目标的推进情况：哪个行动该滚动到下一个阶段了？或者帮我共同规划一个新的长程目标。$brief';
  }

  @override
  String goalBriefItem(String goal, String action) {
    return '「$goal」（行动：$action）';
  }

  @override
  String goalBrief(String goals) {
    return '当前活跃目标：$goals。';
  }

  @override
  String get noGoalsBrief => '目前还没有活跃目标。';

  @override
  String get notSet => '未设定';

  @override
  String get planNewGoal => '和 AI 拆解 / 立新目标';

  @override
  String get nearTerm => '近期';

  @override
  String get midTerm => '中期';

  @override
  String get longTerm => '长远';

  @override
  String get activeGoals => '活跃';

  @override
  String get pausedGoals => '暂停搁置';

  @override
  String get achievedGoals => '已达成';

  @override
  String get inProgressGoal => '进行中';

  @override
  String get pausedGoalState => '已暂停';

  @override
  String get pausedShort => '暂停';

  @override
  String get initializingConversation => '会话初始化';

  @override
  String get loadMessages => '消息加载';

  @override
  String get examplePerson => '@张伟 ';

  @override
  String get exampleTopic => '#付款流程';

  @override
  String goalSummaryCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '目标$count',
      zero: '目标',
    );
    return '$_temp0';
  }

  @override
  String get supplementUnsupported => '未补充：材料不能安全合并，已完整保留；请单独处理或撤回';

  @override
  String get supplementInputChanged => '未补充：输入版本已变化，仍在队列';

  @override
  String get supplementContextChanged => '未补充：材料版本或任务范围不同，已保留为下一问';

  @override
  String get supplementBudget => '未补充：当前请求预算或协议不足，仍在队列';

  @override
  String get supplementClosed => '目标任务已结束；继续队列前不会生成';

  @override
  String get supplementWaiting => '等待工具轮结束后补充；未加入时仍保留排队资格';

  @override
  String get queueUnavailable => '队列状态暂不可用；输入不会重新保存。';

  @override
  String get queueActionUnconfirmed => '队列操作未确认，请刷新核实。';

  @override
  String get queuePaused => '待生成队列已暂停';

  @override
  String get queueOrdered => '待生成队列 · 按保存顺序处理';

  @override
  String get resumeQueue => '继续队列';

  @override
  String get pauseQueue => '暂停队列';

  @override
  String get queueInputDescription => '待生成列表中的输入尚未发送给模型；撤回只取消生成资格。';

  @override
  String get withdrawGeneration => '撤回生成';

  @override
  String get moreQueuedInputs => '另有待生成输入；前面的输入处理后会依次显示。';

  @override
  String get injectedSupplements => '已加入任务的补充';

  @override
  String get injectedSupplementsHint => '仅表示已加入下一请求上下文，不保证网络送达或模型完成；不会再次排队。';

  @override
  String get wikiChat => 'AI对话';

  @override
  String get moveChatWindow => '拖动以移动对话窗口';

  @override
  String get shrinkChatWindow => '缩小对话窗口';

  @override
  String get backToMainChat => '返回主对话';

  @override
  String get aiTasks => 'AI 任务';

  @override
  String get summarizePage => '总结这一页';

  @override
  String get extractKeyPoints => '提取关键点';

  @override
  String get captureDecisions => '沉淀决定和依据';

  @override
  String get organizeNextSteps => '整理下一步';

  @override
  String get translateSelection => '翻译片段';

  @override
  String get writeCopy => '写文案';

  @override
  String get rescanProject => '重新扫描项目目录';

  @override
  String get reimportSource => '重新导入来源';

  @override
  String get taskResetDescription => '重新选择将使用当前页的新版本，并清除旧选区和对照材料。';

  @override
  String get reselectTaskMaterial => '明确重新选择任务与材料';

  @override
  String get askPageHint => '就这一页问问 AI…';

  @override
  String get pageChatWelcome =>
      '👋 我可以帮你总结、提取要点、补充或改写这一页；需要写回知识库时会先给你确认。\n如果这是从本地目录导入的项目，也可以直接说“重新扫描这个项目目录”，我会按最新文件重新导入。';

  @override
  String get summarizePagePrompt => '请总结这一页，提炼最重要的信息。';

  @override
  String get keyPointsPrompt => '请提取这一页的关键事实、判断和可复用信息。';

  @override
  String get decisionsPrompt => '请整理这一页中已经明确的决定及其依据。';

  @override
  String get nextStepsPrompt => '请整理这一页的下一步行动和仍未解决的问题。';

  @override
  String get translateSelectionPrompt =>
      '请把选定的原文片段翻译成 XX 语言，保留结构和专业含义；只处理已提供的片段，先说明范围，不声称翻译整页。';

  @override
  String get writeCopyPrompt => '请根据这一页内容，写一版适合发布到 XX 渠道的文案，目标受众是 XX，语气是 XX。';

  @override
  String get projectRescannedAssets => '项目目录已重新扫描，资产知识页已更新';

  @override
  String get replaceProjectPage => '替换项目知识页正文？';

  @override
  String get replaceProjectDescription => '当前 AI 回复将替换整篇正文。原内容会保留在页面修订记录中。';

  @override
  String get replace => '替换';

  @override
  String get projectPageReplaced => '已替换当前项目知识页正文';

  @override
  String get pageMissing => '页面不存在，可能已被删除';

  @override
  String get projectRescanned => '项目目录已重新扫描，知识页已更新';

  @override
  String get sourceReimported => '已重新导入，知识页内容已更新';

  @override
  String get thinkingShort => '思考中…';

  @override
  String get copiedMessage => '已复制消息';

  @override
  String get replaceBody => '替换正文';

  @override
  String get saveAsOutput => '保存为产出';

  @override
  String get aiReply => 'AI 回复';

  @override
  String get selectReceipt => '选择左侧一条记录查看回执';

  @override
  String get backToList => '返回列表';

  @override
  String get selectProposal => '选择左侧一条提议查看详情';

  @override
  String get pendingReview => '待审';

  @override
  String get decisionHistory => '处理记录';

  @override
  String get refresh => '刷新';

  @override
  String get noDecisions => '暂无处理记录';

  @override
  String get noProposals => '暂无待审提议';

  @override
  String get previousPage => '上一页';

  @override
  String get nextPage => '下一页';

  @override
  String get parametersSources => '参数与来源';

  @override
  String get midTermGoal => '中期目标';

  @override
  String get longTermGoal => '长远目标';

  @override
  String get nearTermGoal => '近期目标';

  @override
  String get motivationLabel => '核心动力：';

  @override
  String get completionCriteriaLabel => '完成判据：';

  @override
  String get feasibilityAssessment => '现实性评估';

  @override
  String get assumptions => '假设清单';

  @override
  String get assumption => '假设';

  @override
  String get milestoneSteps => '里程碑步骤（第一步确认后直接推进，其余排队）';

  @override
  String get proposalTitle => '标题';

  @override
  String get proposalBody => '正文';

  @override
  String get goalVisionShort => '目标愿景';

  @override
  String get ruleContent => '规则内容';

  @override
  String get recordContent => '记录内容';

  @override
  String get note => '备注';

  @override
  String get dueIsoOptional => '截止时间（ISO 8601，可留空）';

  @override
  String get priorityOptions => '优先级（low/normal/high）';

  @override
  String get changeNote => '修改说明';

  @override
  String get newTitle => '新标题';

  @override
  String get phaseOptions => '周期阶段（near/mid/long）';

  @override
  String get visionOptional => '愿景（可留空）';

  @override
  String get assessmentOptional => '现实性评估（可留空）';

  @override
  String get whyMotivation => '为什么做 / 核心动力';

  @override
  String get skillsExperience => '能力与经验';

  @override
  String get availableResources => '已有资源';

  @override
  String get timeConstraints => '时间与现实限制';

  @override
  String get preferencesBoundaries => '偏好与底线';

  @override
  String get actionWeeks => '当前 1~2 周具体行动';

  @override
  String get obstaclesExpected => '当前困难 / 困难预判';

  @override
  String get completionCriteria => '完成判据 / 验收标准';

  @override
  String get reviewGoalBasis => '审阅所依据的目标版本';

  @override
  String get editDraftHint => '保存修改只更新草稿；重新查看后，仍需点击接受才执行业务操作。';

  @override
  String get rejectThis => '拒绝这项';

  @override
  String get acceptThis => '接受这项';

  @override
  String get processingEllipsis => '处理中…';

  @override
  String get saveWithoutExecution => '保存修改，不执行';

  @override
  String get batchReviewDescription =>
      '仅处理下面列出的具体版本。逐项执行，不是整批原子提交；可能部分成功。每项都会显示结果。';

  @override
  String get targetVersionBasis => '目标版本依据';

  @override
  String get batchFinished => '本次逐项处理结束，请查看每项结果。';

  @override
  String get rejectThese => '拒绝这些项';

  @override
  String get acceptThese => '接受这些项';

  @override
  String get processingItems => '逐项处理中…';

  @override
  String get reviewProposals => '审阅提议';

  @override
  String get globalRuleReview => '历史全局规则审阅';

  @override
  String get conversationProposalReview => '对话提议审阅';

  @override
  String get actionActivateRule => '个人规则';

  @override
  String get actionCreateTodo => '创建待办';

  @override
  String get actionSavePage => '保存知识页';

  @override
  String get actionRevisePage => '知识页修订或派生';

  @override
  String get actionRenamePage => '知识页改名';

  @override
  String get actionRecordEvent => '保存记录';

  @override
  String get actionPeopleRelations => '联系人与关系';

  @override
  String get actionArchiveChats => '归档对话';

  @override
  String get actionImportSource => '导入来源';

  @override
  String get actionImportFiles => '导入目录文件';

  @override
  String get actionImportProject => '导入项目摘要';

  @override
  String get actionPlanningProfile => '关于我档案';

  @override
  String get actionCreateGoal => '设立认知目标';

  @override
  String get actionUpdateGoal => '更新目标切片';

  @override
  String get actionCreatePlan => '长程计划拆解';

  @override
  String get actionAddSteps => '计划追加步骤';

  @override
  String get receiptWaiting => '等待执行回执';

  @override
  String get receiptFailedRolledBack => '执行失败（已回滚）';

  @override
  String get declined => '已拒绝';

  @override
  String reviewSelectedCount(int count) {
    return '审阅选中（$count）';
  }

  @override
  String reviewSelectedTitle(int count) {
    return '审阅选中的 $count 项';
  }

  @override
  String pageNumber(int page) {
    return '第 $page 页';
  }

  @override
  String proposalVersion(String action, String version) {
    return '$action · 版本 $version';
  }

  @override
  String updateGoalById(String id) {
    return '更新目标阶段 (ID: $id)';
  }

  @override
  String invalidProposal(String args) {
    return '提议格式异常，不能接受；可以拒绝并重新草拟。\n$args';
  }

  @override
  String goalId(String id) {
    return '目标 ID: $id';
  }

  @override
  String planLabel(String plan) {
    return '计划：$plan';
  }

  @override
  String intentLabel(String intent) {
    return '意图：「$intent」';
  }

  @override
  String visionLabel(String vision) {
    return '愿景：$vision';
  }

  @override
  String criterionLabel(String done) {
    return '（判据：$done）';
  }

  @override
  String proposalReopen(String error) {
    return '$error\n请关闭后重新打开审阅，不会自动接受新的版本。';
  }

  @override
  String fullReceiptCount(int count) {
    return '展开完整回执（全文 $count 字）';
  }

  @override
  String batchUnconfirmed(String error) {
    return '本项未确认完成，请核对处理记录；不会自动重试：$error';
  }

  @override
  String taskRestoreFailed(String error) {
    return '原任务暂不能恢复：$error';
  }

  @override
  String reloadMaterialFailed(String error) {
    return '重新读取材料失败：$error';
  }

  @override
  String savedAsDerivative(String title) {
    return '已保存为派生产物：$title';
  }

  @override
  String get rescan => '重新扫描';

  @override
  String get reimport => '重新导入';

  @override
  String get readPage => '读取页面';

  @override
  String get aiSummary => 'AI 总结';

  @override
  String get justNow => '刚刚';

  @override
  String minutesAgo(int count) {
    return '$count 分钟前';
  }

  @override
  String hoursAgo(int count) {
    return '$count 小时前';
  }

  @override
  String daysAgo(int count) {
    return '$count 天前';
  }

  @override
  String get taskLocalQuestion => '局部问答';

  @override
  String get taskFullSummary => '全文总结';

  @override
  String get taskCompare => '跨来源对照';

  @override
  String get taskLocalEdit => '局部编辑';

  @override
  String get taskFullRevision => '全页修订';

  @override
  String get taskIndependent => '独立新问题';

  @override
  String get taskMaterialChanged => '原任务材料已变化，请明确重新选择任务与材料；不会自动沿用新版本。';

  @override
  String get chooseMaterial => '查看 / 选择材料';

  @override
  String get taskDisplayedRevision => '任务与显示版本';

  @override
  String get taskScopeHint => '后续问题沿用这里的任务和版本；新任务请重新选择。全页修订按连续分块处理，完成后仍需审阅。';

  @override
  String get refreshMaterialRevision => '明确刷新版本';

  @override
  String get comparisonSlug => '对照材料 slug';

  @override
  String get readAndAdd => '读取并加入';

  @override
  String get selectionAutomatic => '未选区；问答将自动选取相关片段';

  @override
  String get selectionTooLong => '选区超过6000字，请缩小';

  @override
  String get useDisplayedRevision => '使用显示的版本';

  @override
  String get compassDismissHint => '处理急事，7 天内不打扰';

  @override
  String get compassFocusObstacle => '聚焦攻克此困难';

  @override
  String get compassDiscussProgress => '聊聊推进破局';

  @override
  String get compassUpdateAction => '更新当前行动';

  @override
  String get compassSnooze => '暂不打扰（退避7天）';

  @override
  String get compassRealigned => '已更新目标行动，航向已校准';

  @override
  String get compassDismissed => '已暂不打扰，7 天内静默退避';

  @override
  String get compassTitle => '航向偏离提醒';

  @override
  String get compassSnoozeHint => '暂不打扰（7天）';

  @override
  String get compassRollAction => '滚动更新行动';

  @override
  String get habitDoneFallback => '完成了';

  @override
  String get habitMissedFallback => '还没';

  @override
  String get habitDoneTextFallback => '今天完成了';

  @override
  String get habitMissedTextFallback => '今天还没完成';

  @override
  String get habitCheckin => '习惯问候';

  @override
  String get changeTime => '改时间';

  @override
  String get pauseCheckins => '暂停问候';

  @override
  String get checkinsPaused => '已暂停，可在设置页恢复';

  @override
  String get askLater => '晚点再问';

  @override
  String get discuss => '聊聊';

  @override
  String get skipToday => '今天不问';

  @override
  String get discussReason => '聊聊原因';

  @override
  String get checkinTimeUpdated => '已按新的时间继续提醒';

  @override
  String get checkinsUpdated => '定期问候已更新';

  @override
  String get checkinDismissed => '今天先不问了';

  @override
  String get checkinSnoozed => '已推迟，稍后再问一次';

  @override
  String get checkinSuperseded => '已再次提醒';

  @override
  String get checkinExpired => '这条问候已过期';

  @override
  String get checkinPlanClosed => '这条问候的计划已关闭';

  @override
  String get checkinRecordedEvent => '已记录，并生成一条事件';

  @override
  String get askAfterHour => '好，1 小时后再问一次';

  @override
  String get cognitiveInsight => '认知洞察';

  @override
  String get adoptAsRule => '采纳为规则';

  @override
  String get enableCheckins => '开启习惯问候';

  @override
  String get makeTodo => '设为待办';

  @override
  String get acknowledge => '知道了';

  @override
  String get insightRuleApplied => '已采纳为个人规则 ✓';

  @override
  String get insightHabitEnabled => '已开启习惯定时问候 ✓';

  @override
  String get insightTodoAdded => '已加入待办清单 ✓';

  @override
  String get insightHandled => '已处理 ✓';

  @override
  String get insightDismissed => '已了解';

  @override
  String get insightExpired => '这条洞察已搁置';

  @override
  String get personalRuleAdopted => '已采纳为个人规则';

  @override
  String get addedToTodoList => '已加入待办清单';

  @override
  String get recorded => '已记录';

  @override
  String get pageNotFoundDeleted => '页面不存在或已删除';

  @override
  String get habitCheckinsEnabled => '已开启习惯定时问候';

  @override
  String get contactsSuggestionInvalid => '有一条联系人关联建议，但数据无法解析。';

  @override
  String get choose => '选择';

  @override
  String get contactArgsInvalid => '动作参数格式异常，无法选择联系人';

  @override
  String get contactArgsUnreadable => '动作参数无法解析，无法选择联系人';

  @override
  String get selectionCancelledUnsaved => '已取消选择，未保存本次修改';

  @override
  String get wikiEmptyHelp =>
      '知识库还没有页面\n\n去「导入」tab 粘贴网址或文本，\n或在对话里说「导入 xxx 到知识库」';

  @override
  String get sortRecentlyUpdated => '按最近更新';

  @override
  String get loadFailed => '加载失败';

  @override
  String get reminderTenMinutes => '+10分钟';

  @override
  String get done => '完成';

  @override
  String get dismissReminderHint => '关闭（不标记完成）';

  @override
  String get bodyRequired => '正文不能为空';

  @override
  String get topicTitleHint => '标题（可选，留空自动取正文首行前 60 字）';

  @override
  String get topicTagsHint => '标签（可选，空格或逗号分隔，例如：理财 副业）';

  @override
  String get markdownBodyHint => '正文（Markdown 支持）';

  @override
  String get saveToTopics => '保存到主题库';

  @override
  String materialScope(String task, String range) {
    return '材料范围：$task$range';
  }

  @override
  String materialRange(int start, int end) {
    return ' · $start–$end字';
  }

  @override
  String materialTitleCount(String title, int count) {
    return '$title · $count 字';
  }

  @override
  String selectionHint(String selection) {
    return '在下方原文选择要处理的文字（最多6000字）。当前：$selection';
  }

  @override
  String compassPlanTitle(String plan) {
    return '航向校准 · 计划「$plan」';
  }

  @override
  String compassGoalTitle(String goal) {
    return '航向校准 · 目标「$goal」';
  }

  @override
  String compassStalled(int days) {
    return '停滞 $days 天';
  }

  @override
  String compassActionStalled(String action, int days) {
    return '当前行动「$action」已停滞 $days 天';
  }

  @override
  String compassObstacle(String obstacle) {
    return '，困难：$obstacle';
  }

  @override
  String compassRecentTopics(String topics) {
    return '。近期注意力多在「$topics」。';
  }

  @override
  String compassObstacleLine(String obstacle) {
    return '当前困难：$obstacle';
  }

  @override
  String compassSeedObstacle(String goal, String obstacle, String action) {
    return '关于目标「$goal」卡在「$obstacle」，当前行动是「$action」，我们聊聊怎么破局？';
  }

  @override
  String compassSeedProgress(String goal, String action) {
    return '关于目标「$goal」，当前行动是「$action」，近几天有些停滞，我们聊聊怎么推进？';
  }

  @override
  String compassPlanStep(String plan) {
    return '计划「$plan」· 步骤';
  }

  @override
  String compassCardDetail(
    String prefix,
    String goal,
    String action,
    int days,
    String topics,
  ) {
    return '$prefix「$goal」当前行动「$action」已连续 $days 天未推进，近期精力分散在「$topics」。';
  }

  @override
  String recordedText(String text) {
    return '已记录：$text';
  }

  @override
  String checkinDiscussSeed(String text) {
    return '$text，因为';
  }

  @override
  String insightSuggestion(String action) {
    return '建议：$action';
  }

  @override
  String insightSource(String slug) {
    return '来源：$slug';
  }

  @override
  String contactsSuggestion(String people, String targets) {
    return '发现联系人：$people\n关联事项：$targets\n请打开“审阅提议”，接受或拒绝这项建议。';
  }

  @override
  String chooseContactName(String name) {
    return '选择联系人「$name」';
  }

  @override
  String chooseTargetName(String name) {
    return '选择事项「$name」';
  }

  @override
  String recentPageCount(int count) {
    return '最近 $count 条';
  }

  @override
  String incubatingCount(int count) {
    return '孵化中 $count 条 · 待定论';
  }

  @override
  String evidenceCount(int count) {
    return '证据 $count';
  }

  @override
  String reminderTitle(String title, String time) {
    return '提醒：$title（$time）';
  }

  @override
  String topicSavedTitle(String title) {
    return '已存为主题：《$title》';
  }

  @override
  String get copyPageTitle => '复制标题';

  @override
  String get pageTitleCopied => '页面标题已复制';

  @override
  String get readPageFailed => '读取知识页出错';

  @override
  String get pageBodyEmpty => '*(正文为空)*';

  @override
  String get knowledgePageMissing => '知识页未找到或尚未生成';

  @override
  String get knowledgePageMissingHelp =>
      '该条目可能已被删除、重命名，或者是由 AI 回复提及的建议标识符，尚未正式写入知识库。';

  @override
  String get openInKnowledgeBase => '在知识库中打开';

  @override
  String get checkinQuestionRequired => '问法不能为空';

  @override
  String get checkinTimeInvalid => '时间需在 07:00–22:00 之间，格式 HH:MM';

  @override
  String get checkinDayRequired => '至少选择一天';

  @override
  String get checkinResponsesRequired => '按钮和记录句都不能为空';

  @override
  String get closeCheckinsQuestion => '关闭定期问候？';

  @override
  String get closeCheckinsHelp => '已产生的历史问候和回答会保留，之后不再主动问你。';

  @override
  String get askRegularly => '定期问我';

  @override
  String get adjustCheckins => '调整定期问候';

  @override
  String get checkinHelp => '到点后我会在主对话里发一条消息。所有内容先由你确认，不会自动修改习惯页。';

  @override
  String get checkinSyncHelp => '多设备同步时只有主设备会在到点后写入问候；其他设备同步显示，不会重复发问。';

  @override
  String get inferenceBasis => '推断依据';

  @override
  String get checkinQuestion => '问法';

  @override
  String get checkinQuestionHint => '例如：今天午饭后散步了吗？';

  @override
  String get checkinTime => '每天什么时候问';

  @override
  String get repeat => '重复';

  @override
  String get everyDay => '每天';

  @override
  String get checkinDoneFields => '完成按钮与记录';

  @override
  String get checkinMissedFields => '未完成按钮与记录';

  @override
  String get walkDoneHint => '走了';

  @override
  String get walkDoneTextHint => '今天午饭后散步了';

  @override
  String get walkMissedHint => '没走';

  @override
  String get walkMissedTextHint => '今天还没走';

  @override
  String get checkinRecordHelp => '按钮文字尽量短；记录句会作为一条真实记录写入，「没完成」默认只留在对话里，不生成事件。';

  @override
  String get closeCheckins => '关闭定期问候';

  @override
  String get feedbackSuggestion => '反馈这条建议';

  @override
  String get feedbackHelp => '选择回复中的建议原文。接受表示认可建议，实际行动仍需另行确认。';

  @override
  String get feedbackOriginal => '建议原文（可缩小到其中一段）';

  @override
  String get feedbackRewrite => '我的改写（可选）';

  @override
  String get feedbackAccept => '接受';

  @override
  String get feedbackIgnore => '忽略';

  @override
  String get feedbackSaveRewrite => '保存改写';

  @override
  String get feedbackWithdraw => '撤回反馈';

  @override
  String get feedbackRewritten => '改写';

  @override
  String get feedbackWithdrawn => '撤回';

  @override
  String get pageKindProfile => '档案';

  @override
  String get pageKindPerson => '联系人';

  @override
  String get pageKindRecurringCost => '固定成本';

  @override
  String get pageKindCapability => '能力';

  @override
  String get pageKindAsset => '资产';

  @override
  String get pageKindProject => '项目';

  @override
  String get pageKindRelationship => '关系';

  @override
  String get pageKindDecision => '决策';

  @override
  String get pageKindHabit => '习惯';

  @override
  String get pageKindConstraint => '约束';

  @override
  String get pageKindInsight => '洞察';

  @override
  String get pageKindTopic => '主题';

  @override
  String get pageKindMethod => '方法';

  @override
  String get pageKindCase => '案例';

  @override
  String get pageKindPrinciple => '规律';

  @override
  String get pageKindSeries => '系列';

  @override
  String get pageKindSource => '来源';

  @override
  String checkinDaysPerWeek(int count) {
    return '每周 $count 天';
  }

  @override
  String weekdayShort(String day) {
    String _temp0 = intl.Intl.selectLogic(day, {
      '1': '一',
      '2': '二',
      '3': '三',
      '4': '四',
      '5': '五',
      '6': '六',
      'other': '日',
    });
    return '$_temp0';
  }

  @override
  String get load => '读取';

  @override
  String get promptTemplateLanguage => '模板语言';

  @override
  String get promptIncompatibleFallback => '当前修订不兼容，使用内置默认';

  @override
  String promptParentFallback(String requested, String resolved) {
    return '使用父语言资源：$requested → $resolved';
  }

  @override
  String promptMissingTranslation(String requested, String resolved) {
    return '缺少译文，使用内置默认：$requested → $resolved';
  }

  @override
  String get aiResponseLanguage => 'AI 回答语言';

  @override
  String get aiInstructionLanguage => '指令语言';

  @override
  String get languageAutomatic => '自动';

  @override
  String get instructionFollowContext => '使用默认策略';

  @override
  String get languagePolicyHelp =>
      '回答自动模式根据本轮与最近可靠会话语言决定。默认指令在对话中跟随界面，在后台跟随知识语言；设置变更只影响新运行。';

  @override
  String knowledgeLanguageReadonly(String locale) {
    return '知识产出语言：$locale（当前只读）';
  }

  @override
  String runLanguageUi(String locale) {
    return '界面语言：$locale';
  }

  @override
  String runLanguageInstructions(String locale) {
    return '指令语言：$locale';
  }

  @override
  String runLanguageResponse(String locale, String source) {
    return '回答语言：$locale · $source';
  }

  @override
  String runLanguageKnowledge(String locale) {
    return '知识产出语言：$locale';
  }

  @override
  String runLanguageFallback(String reason) {
    return '回退：$reason';
  }

  @override
  String get languageSourceExplicit => '本轮明确要求';

  @override
  String get languageSourcePreference => '固定偏好';

  @override
  String get languageSourceCurrent => '本轮交流语言';

  @override
  String get languageSourceConversation => '沿用会话语言';

  @override
  String get languageSourceFallback => '默认回退';

  @override
  String get languageSourceDataset => '数据集策略';

  @override
  String get languageSourceLegacy => '历史策略';

  @override
  String get promptContextContract => '对话场景与档案协议';

  @override
  String get promptToolDescriptionsContract => '工具说明协议';

  @override
  String get promptToolMetadataContract => '技能目录与工具启用协议';

  @override
  String get promptExecutionContract => '运行修复与页内处理协议';

  @override
  String get promptProcessingContract => '后台处理与分析协议';

  @override
  String get promptReplyContract => '对话答复与待审状态文案';

  @override
  String get promptKnowledgeProcessingContract => '知识整理与主题维护协议';

  @override
  String get promptLookupContract => '本机数据直查文案';

  @override
  String get promptSupplementContract => '任务补充材料与引用协议';

  @override
  String get knowledgeLanguage => '知识语言';

  @override
  String get knowledgeLanguageHelp =>
      '新知识使用此语言。已开始的任务沿用原语言，已有页面和原文保持不变；修改后不会重新处理整个知识库。';

  @override
  String get knowledgeLanguageUnsupported =>
      '当前版本不支持此数据集的语言策略，知识生成已暂停。请更新应用后继续。';

  @override
  String get knowledgeLanguageOwnerOnly => '只有数据集的主设备可以修改。';

  @override
  String get knowledgeLanguageCapabilityRequired => '修改知识语言前，请连接并更新所有已配对设备。';

  @override
  String get title => '标题';

  @override
  String get copy => '复制';

  @override
  String get todoInputHint => '记一条要跟进的事…（回车添加）';

  @override
  String get todoNoteHint => '补充说明（可选）';

  @override
  String get todoEmptyHelp => '还没有待办\n\n可以手动添加，或在对话里让 AI 分析出\n需要跟进的事（需确认后创建）';

  @override
  String todoOpenGroup(int count) {
    return '进行中 · $count';
  }

  @override
  String todoDoneGroup(int count) {
    return '已完成 · $count';
  }

  @override
  String get todoEdit => '编辑待办';

  @override
  String get todoTitleHint => '要跟进的事…';

  @override
  String get todoDueHint => '截止日期（可选，YYYY-MM-DD，留空清除）';

  @override
  String get todoReminderOption => '到期提醒（到时弹横幅/通知）';

  @override
  String get todoPriority => '优先级';

  @override
  String get priorityHigh => '高';

  @override
  String get priorityNormal => '中';

  @override
  String get priorityLow => '低';

  @override
  String todoDueDate(String date) {
    return '截止 $date';
  }

  @override
  String todoReminderDate(String date) {
    return '提醒 $date';
  }

  @override
  String get todoStatusOpen => '进行中';

  @override
  String get todoStatusArchived => '已归档';

  @override
  String errorFieldRequired(String field) {
    return '$field不能为空。';
  }

  @override
  String errorNotFound(String resource) {
    return '找不到$resource，请刷新后重试。';
  }

  @override
  String get errorSettingsChanged => '设置已更新，请刷新后重新修改。';

  @override
  String get errorSyncUnlockRequired => '请先解锁同步，再修改此设置。';

  @override
  String get errorLanguageUnsupported => '当前版本不支持所选语言或策略，请更新应用后重试。';

  @override
  String get errorReminderTimePast => '提醒时间已过，请先设置一个未来时间。';

  @override
  String get errorReminderTimeRequired => '请先设置截止时间，再开启提醒。';

  @override
  String get errorDetails => '查看详情';

  @override
  String get open => '打开';

  @override
  String get knowledgePage => '知识页';

  @override
  String get habitReminderSettings => '习惯提醒';

  @override
  String get habitReminderSettingsHelp =>
      '这里汇总所有未归档习惯页的定期问候。时间、问法和按钮改动会直接保存到对应习惯页；主设备到点后把问候写进主对话，其他设备只同步显示，不会重复发问。';

  @override
  String get habitReminderEmpty => '还没有未归档的习惯页。先在知识库创建「习惯」页，再回到这里设置定期问候。';

  @override
  String habitReminderCounts(int enabled, int paused, int unset) {
    return '运行中 $enabled · 已暂停 $paused · 未设置 $unset';
  }

  @override
  String get habitReminderActive => '运行中';

  @override
  String get habitReminderUnset => '尚未设置定期问候';

  @override
  String get habitReminderSetup => '设置';

  @override
  String get habitReminderAdjust => '调整';

  @override
  String get habitReminderResumed => '定期问候已恢复';

  @override
  String get habitReminderPaused => '定期问候已暂停';

  @override
  String habitWeekSchedule(String days) {
    return '每周 $days';
  }

  @override
  String get habitReminderNotSet => '未设置';

  @override
  String get trayShowWindow => '显示主窗口';

  @override
  String get trayQuit => '退出 Elsewhen';

  @override
  String get syncKeepAliveDefault => '正在与另一台设备同步；请保持应用运行。';

  @override
  String get syncKeepAliveSetup => '正在准备本机同步身份';

  @override
  String get syncKeepAlivePairOwner => '正在与加入设备配对';

  @override
  String get syncKeepAlivePairJoin => '正在与已有设备配对';

  @override
  String get syncKeepAliveEnrollment => '正在完成设备交接与初始同步';

  @override
  String get syncNotificationTitle => 'Elsewhen 正在同步';

  @override
  String get syncNotificationChannel => '设备同步';

  @override
  String get syncNotificationDescription => '配对与初始快照传输期间保持应用运行';

  @override
  String get syncObjectConflictsTitle => '待办与目标同步冲突';

  @override
  String get syncObjectConflictsHelp =>
      '来自其他设备的待办、目标、计划、规则或知识语言策略与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。';

  @override
  String get syncObjectConflictsEmpty => '没有待审阅的待办/目标/计划/规则或知识语言冲突。';

  @override
  String syncConflictFrom(String kind, String device) {
    return '$kind · 来自 $device 的改动';
  }

  @override
  String get syncKindTodo => '待办';

  @override
  String get syncKindGoal => '目标';

  @override
  String get syncKindPlan => '计划';

  @override
  String get syncKindRule => '规则';

  @override
  String get syncDeletedReason => '对端已删除本条，而之后仍有改动要送达';

  @override
  String get syncGoalDeletedReason => '对端已删除本条（目标无删除语义，删除后收到编辑必审）';

  @override
  String get syncBaseChangedReason => '两端都修改了同一条，各自基于旧版本';

  @override
  String get syncGoalLimitReason => '对端目标会使本端活跃目标超出上限';

  @override
  String get syncLanguageChangedReason => '知识语言策略的版本不一致，请核对主设备签名的版本。';

  @override
  String get syncLocalDeleted => '本端（已删除状态）';

  @override
  String get syncLocalCurrent => '本端当前';

  @override
  String get syncIncomingVersion => '对端送来的版本';

  @override
  String get syncKeepLocal => '保留本端';

  @override
  String get syncAcceptIncoming => '采纳对端';

  @override
  String get syncDeleted => '已删除';

  @override
  String get syncEmptyValue => '（空）';

  @override
  String syncStatusUpdated(String status, String updated) {
    return '状态 $status · 更新 $updated';
  }

  @override
  String get syncRefresh => '重新拉取';

  @override
  String get syncLoadingConflicts => '正在读取冲突…';

  @override
  String get syncNoActionNeeded => '无需处理';

  @override
  String syncReviewCount(int count) {
    return '$count 条待审阅';
  }

  @override
  String get errorConflictChanged => '本端记录已变化，请刷新后重新审阅。';

  @override
  String get errorConflictResolved => '该冲突已裁决，请刷新列表。';

  @override
  String get errorGoalLimit => '本端活跃目标已满，无法直接采纳对端；请先归档目标或选择保留本端。';

  @override
  String get knowledgeReadingTitle => '阅读与使用';

  @override
  String get knowledgeReadingHelp => '记录自己的阅读与使用进度；不会改变原文或观点认可状态。';

  @override
  String get readingUnread => '待读';

  @override
  String get readingRead => '已读';

  @override
  String get readingValuable => '值得使用';

  @override
  String get readingAdopted => '已采用';

  @override
  String get readingArchived => '归档';

  @override
  String libraryAllFilter(String filter) {
    return '全部$filter';
  }

  @override
  String get librarySearchHint => '搜索标题或全文';

  @override
  String get clearSearch => '清除搜索';

  @override
  String get libraryArea => '分区';

  @override
  String get libraryImported => '素材库';

  @override
  String get libraryInsight => '知识沉淀';

  @override
  String get libraryNetwork => '联系人与项目';

  @override
  String get libraryKind => '类型';

  @override
  String get libraryReadingState => '阅读状态';

  @override
  String get tags => '标签';

  @override
  String get librarySourceKind => '原料';

  @override
  String get pageKindNote => '笔记';

  @override
  String get loadMore => '加载更多';

  @override
  String get libraryNoMatch => '没有匹配的知识页';

  @override
  String get libraryAllShown => '已显示全部结果';

  @override
  String get knowledgeBatchTitle => '批量审阅修订';

  @override
  String get knowledgeBatchHelp => '先查看正文与适用条件，每批最多确认 20 项';

  @override
  String get knowledgeViewRevision => '查看本项修订';

  @override
  String knowledgeAcceptCount(int count) {
    return '确认采纳 $count 项';
  }

  @override
  String get knowledgeRejectSelected => '拒绝所选';

  @override
  String get knowledgeAccepted => '已采纳';

  @override
  String get knowledgeRejected => '已拒绝';

  @override
  String get priorRejectionTitle => '此前否定过这条来源';

  @override
  String get priorRejectionReason => '你曾把这条来源丢弃为负知识，当时的原因：';

  @override
  String get priorRejectionHelp => '重新导入会重新入册并从孵化中重新开始；要继续吗？';

  @override
  String get saveAnyway => '仍要保存';

  @override
  String get sourceUpdateTitle => '来源内容有更新';

  @override
  String get sourceUpdateHelp => '确认后保存为新版本，旧原文保留；引用它的知识页会提示复核。';

  @override
  String get sourceSavedVersion => '已保存版本';

  @override
  String get sourceIncomingVersion => '本次抓取';

  @override
  String get sourceKeepOld => '保留旧版本';

  @override
  String get sourceSaveNew => '确认保存新版本';

  @override
  String get wikiImportUrl => '网址导入';

  @override
  String get wikiImportUrlHelp => '抓取网页或推文，确认后保存';

  @override
  String get wikiCreateTopic => '创建主题';

  @override
  String get wikiCreateTopicHelp => '输入内容并创建知识页';

  @override
  String get knowledgeReview => '知识审阅';

  @override
  String get noPendingWork => '无待办';

  @override
  String get wikiCloseConfirm => '关闭前确认';

  @override
  String get wikiUnsavedChanges => '该页面还有未保存的编辑，关闭将丢失这些修改。';

  @override
  String get wikiDiscardClose => '放弃修改并关闭';

  @override
  String get wikiSaveClose => '保存并关闭';

  @override
  String get wikiCloseTab => '关闭标签页';

  @override
  String get wikiPageUnavailable => '页面不存在或已被删除';

  @override
  String get wikiEditTags => '编辑标签';

  @override
  String get wikiTagsHelp => '用空格或逗号分隔多个标签，留空即清空';

  @override
  String get wikiTagsExample => '例如：工作 Rust 投资';

  @override
  String get wikiEnterUrl => '请先粘贴一个链接';

  @override
  String get wikiEnterContent => '请先输入要保存的内容';

  @override
  String wikiSavedTitle(String title) {
    return '已保存到知识库：《$title》';
  }

  @override
  String get wikiSave => '保存到知识库';

  @override
  String get wikiUrlHelp => '粘贴任意网址：x.com/twitter.com 推文或其他网页链接，也支持文章型推文';

  @override
  String get wikiUrlHint => 'https://x.com/… 或 https://example.com/…';

  @override
  String get wikiFetching => '抓取中…';

  @override
  String get wikiFetch => '抓取';

  @override
  String get wikiPreviewHelp => '抓取后会新开预览 tab，预览内容后点「保存到知识库」才会入库';

  @override
  String get wikiTitleHint => '标题（可选，留空自动取前 60 字）';

  @override
  String get wikiTagsHint => '标签（可选，空格或逗号分隔，例如：工作 投资）';

  @override
  String get wikiSourceLoading => '正在读取来源…';

  @override
  String get wikiSourceRetry => '来源读取失败，点击重试';

  @override
  String get wikiSourceUnavailable => '来源页已不可用';

  @override
  String wikiBasedOn(String title) {
    return '基于：$title';
  }

  @override
  String get wikiSaved => '已保存到知识库';

  @override
  String get wikiContent => '内容';

  @override
  String get wikiOriginal => '原文';

  @override
  String get wikiOutputs => '产出';

  @override
  String get wikiRelations => '关联';

  @override
  String get wikiSources => '来源';

  @override
  String get wikiSourcesRevisions => '来源与修订';

  @override
  String get wikiPageChangedTitle => '页面已被后台更新';

  @override
  String get wikiPageChangedHelp =>
      '你编辑期间，该页面已被更新。\n「重新加载」放弃你的修改并查看最新内容；\n「强制覆盖」以你的修改覆盖更新（两个版本都留在修订历史里）。';

  @override
  String get wikiReload => '重新加载';

  @override
  String get wikiOverwrite => '强制覆盖';

  @override
  String wikiCollectedOn(String date) {
    return '采集 $date';
  }

  @override
  String get wikiCheckinHint => '到点在主对话里问我，问法和按钮先由你确认';

  @override
  String wikiCheckinPaused(String time) {
    return '已暂停 · $time';
  }

  @override
  String get wikiCheckins => '定期问候';

  @override
  String get wikiCopyTitle => '复制标题';

  @override
  String get wikiTitleCopied => '标题已复制';

  @override
  String get wikiNoEventOrigins => '源于 尚无事件溯源';

  @override
  String wikiEventOrigins(int count) {
    return '源于 $count 条事件';
  }

  @override
  String get wikiAddTags => '添加标签';

  @override
  String get wikiContactRelations => '联系人关系';

  @override
  String get wikiTagsUpdated => '标签已更新';

  @override
  String get wikiImmersive => '沉浸阅读';

  @override
  String get wikiImmersiveHelp => '沉浸阅读（Esc 退出）';

  @override
  String get wikiExitImmersive => '退出沉浸阅读（Esc）';

  @override
  String get wikiReadingSettings => '阅读设置';

  @override
  String get wikiReadingSettingsHelp => '正文阅读设置（字体 / 字号 / 行距）';

  @override
  String get wikiEditContent => '编辑正文';

  @override
  String get wikiDeletePage => '删除页面';

  @override
  String get wikiDeleteQuestion => '删除页面？';

  @override
  String wikiDeleteHelp(String title) {
    return '「$title」将被删除，修订历史随之清除，且不可恢复。其他设备同步后同样删除；它们与此页冲突的离线编辑会保留为冲突候选。';
  }

  @override
  String get errorWikiPageChanged => '页面已更新，请重新加载或确认覆盖。';

  @override
  String get errorWikiMaterialReadonly => '素材原文只读，无法编辑正文。';

  @override
  String get errorWikiContentRequired => '正文不能为空。';

  @override
  String get errorWikiContentTooLong => '正文不能超过 65,536 个字符。';

  @override
  String get wikiDiscussPage => '和 AI 讨论此页';

  @override
  String get wikiAskAi => '问 AI';

  @override
  String get wikiProjectPathCopied => '项目路径已复制';

  @override
  String get wikiEditProjectPath => '修改项目路径';

  @override
  String get wikiChooseProjectDirectory => '选择项目目录';

  @override
  String get wikiOldPath => '旧路径';

  @override
  String get wikiNewPath => '新路径';

  @override
  String get wikiProjectPathUpdated => '项目路径已更新';

  @override
  String get wikiSourceLinkCopied => '来源链接已复制';

  @override
  String get wikiRelatedPages => '相关页面';

  @override
  String get wikiAddRelatedPage => '添加相关页面';

  @override
  String get wikiNoRelatedCandidates => '暂时没有可选的页面';

  @override
  String get wikiTweet => '推文';

  @override
  String get wikiWebpage => '网页';

  @override
  String get wikiImportTagsHint => '标签（可选，空格分隔，例如：方法 投资）';

  @override
  String get wikiHome => '首页';

  @override
  String wikiPostTab(String id) {
    return '推文 $id';
  }

  @override
  String wikiRelationLabel(String relation) {
    String _temp0 = intl.Intl.selectLogic(relation, {
      'related': '相关',
      'mentions': '提及',
      'mentionedBy': '被提及',
      'part_of': '从属',
      'contains': '包含本页',
      'same_topic': '同主题',
      'supports': '佐证',
      'supportedBy': '被佐证',
      'contradicts': '相悖',
      'contradictedBy': '被相悖',
      'other': '$relation',
    });
    return '$_temp0';
  }

  @override
  String get tweetDiscuss => '与 AI 讨论这篇推文';

  @override
  String get tweetDiscussHelp => '保存前可先和 AI 过一遍，点「保存到知识库」才入库';

  @override
  String get tweetUnknownAuthor => '未知作者';

  @override
  String tweetByAuthor(String author) {
    return '$author 的推文';
  }

  @override
  String get tweetSaved => '已保存';

  @override
  String get tweetOriginal => '抓取内容（原文）';

  @override
  String get tweetLinkCopied => '推文链接已复制';

  @override
  String get tweetFormatting => 'AI 整理中…';

  @override
  String get tweetFormat => 'AI 整理为 Markdown';

  @override
  String get tweetFormatAgain => '重新整理';

  @override
  String get tweetDiscardFormat => '放弃整理';

  @override
  String get tweetSeparateDraftHelp => '保存原文，整理稿另存为产物';

  @override
  String get tweetFormatPreview => 'AI 整理结果（Markdown 预览）';

  @override
  String get tweetChatHint => '就这篇推文问问 AI…（回车发送，Shift+回车换行）';

  @override
  String get artifactWorks => '衍生作品';

  @override
  String get artifactRefresh => '刷新衍生作品';

  @override
  String get artifactEmpty => '还没有衍生作品。在本页对话里让 AI 生成摘要、观点、灵感或脚本，保存后会在这里列为版本。';

  @override
  String get artifactNotAdopted => '未采用';

  @override
  String artifactMetadata(String model, String strategy) {
    return '模型：$model · 流程版本：$strategy';
  }

  @override
  String get artifactUnknownModel => '未知（未记录）';

  @override
  String get artifactUnknownStrategy => '未知（旧产物）';

  @override
  String get artifactInstruction => '生成指令';

  @override
  String get artifactNoInstruction => '此版本未记录生成指令';

  @override
  String get artifactAdoptedRevision => '查看实际采用的修订';

  @override
  String get artifactNewerRevisionHelp => '当前正文已有新修订；采用记录仍指向之前确认的修订。';

  @override
  String get artifactOpenPage => '打开知识页';

  @override
  String get artifactCopyBody => '复制全文';

  @override
  String get artifactBodyCopied => '已复制完整正文';

  @override
  String get artifactCompareSelect => '加入比较';

  @override
  String get artifactUnadopt => '取消采用';

  @override
  String get artifactAdopt => '采用这个版本';

  @override
  String get artifactMoreVersions => '加载更多版本';

  @override
  String get artifactCompare => '版本比较';

  @override
  String artifactType(String type) {
    String _temp0 = intl.Intl.selectLogic(type, {
      'summary': '摘要',
      'opinion': '观点',
      'idea': '灵感',
      'script': '脚本',
      'ai_summary': 'AI 总结',
      'other': '$type',
    });
    return '$_temp0';
  }

  @override
  String get workItemUpdateStatus => '更新状态';

  @override
  String get workItemUpdatePriority => '更新优先级';

  @override
  String workItemPriority(String priority) {
    String _temp0 = intl.Intl.selectLogic(priority, {
      'high': '高优先级',
      'low': '低优先级',
      'other': '普通优先级',
    });
    return '$_temp0';
  }

  @override
  String get workItemChooseDue => '选择截止日期';

  @override
  String get workItemSetDue => '设置截止日期';

  @override
  String get entityMerged => '此实体已合并';

  @override
  String entityMergedInto(String target) {
    return '此实体已合并到 $target';
  }

  @override
  String get entityViewTarget => '查看目标';

  @override
  String get entityUndoMerge => '撤销合并';

  @override
  String get entityMerge => '合并实体';

  @override
  String get entityNoMergeCandidates => '没有可合并的同类型实体';

  @override
  String get entityChooseTarget => '选择目标实体';

  @override
  String get entityMergeInto => '合并到';

  @override
  String get entityContinue => '继续';

  @override
  String entityMergeQuestion(String title) {
    return '合并到“$title”？';
  }

  @override
  String get entityMergeHelp =>
      '事实、别名和关系会迁移到目标实体；重复内容会合并。原始事件不会改变。只要迁移后的内容没有被修改，就可以从旧实体页撤销。';

  @override
  String get entityConfirmMerge => '确认合并';

  @override
  String entityMergeSaved(String title) {
    return '已合并到“$title”，原始事件仍保留';
  }

  @override
  String get entityUndoMergeQuestion => '撤销这次合并？';

  @override
  String get entityUndoMergeHelp =>
      '只恢复本次合并迁移的事实、别名和关系。若这些内容合并后已被修改，系统会拒绝操作以保护新数据。原始事件不会改变。';

  @override
  String get entityMergeUndone => '合并已撤销，实体内容已恢复';

  @override
  String get entityRelatedTodos => '关联待办';

  @override
  String get entityOpenWorkItem => '打开工作项';

  @override
  String get entityFacts => '结构化事实';

  @override
  String get entityFactConflictHelp => '发现可能冲突的事实。历史来源均已保留，请查看来源后移除错误的派生事实。';

  @override
  String entityFactSource(String date, String confidence, String source) {
    return '$date · 置信度 $confidence/5 · 查看来源 $source';
  }

  @override
  String get entityRemoveFact => '纠正：移除此事实';

  @override
  String get entityRemoveFactQuestion => '移除这条事实？';

  @override
  String get entityRemoveFactHelp => '只会移除知识页中的派生事实，不会删除原始事件。';

  @override
  String get entityRemove => '移除';

  @override
  String get entityFactRemoved => '事实已移除，原始事件仍保留';

  @override
  String get entitySourceMissing => '来源事件不存在';

  @override
  String get entitySourceEvent => '来源事件';

  @override
  String get entityAiSummary => 'AI 摘要';

  @override
  String get entityEmotionAnnotations => '情绪标注（文本，无数值）';

  @override
  String entityEmotionAboutEvent(String feeling) {
    return '对这件事：$feeling';
  }

  @override
  String entityEmotionAboutTarget(String target, String feeling) {
    return '对 $target：$feeling';
  }

  @override
  String get entityAiGuidance => 'AI 指导（派生层）';

  @override
  String entityRelatedReference(String slug, String note) {
    return '相关：$slug — $note';
  }

  @override
  String entityAnalysisMetadata(String date, String status) {
    return '记录于 $date · 分析状态 $status';
  }

  @override
  String entityAnalysisStatus(String status) {
    String _temp0 = intl.Intl.selectLogic(status, {
      'pending': '待处理',
      'running': '处理中',
      'succeeded': '已完成',
      'failed': '失败',
      'skipped': '已跳过',
      'other': '$status',
    });
    return '$_temp0';
  }

  @override
  String get entityGuidance => '相关指导';

  @override
  String get entityViewGuidance => '查看指导';

  @override
  String entityGuidanceDetails(String date) {
    return '$date - 点击查看要点';
  }

  @override
  String get entityGuidanceHelpful => '有用（再点一次清除）';

  @override
  String get entityGuidanceNotRelevant => '不相关（再点一次清除）';

  @override
  String entityRecordedOn(String date) {
    return '记录于 $date';
  }

  @override
  String get entityReusableLessons => '可复用经验';

  @override
  String get entityRefinedTodos => '待办精炼';

  @override
  String get runLanguagePreserveSource => '产出策略：保留来源原文语言（仅排版整理）';

  @override
  String get l6ReviewHistoryIsNotAPersonaldee4d => '审阅历史未归属规则';

  @override
  String get l6All778fc => '全部';

  @override
  String get l6RunLoga8ce4 => '运行日志';

  @override
  String get l6NoRunsYet022d0 => '还没有运行记录';

  @override
  String get l6ProtectedOnlyEvidenceCanBeAddedba2bf => '受保护（仅累加证据）';

  @override
  String get l6Savinga032e => '保存中';

  @override
  String get l6ScanInvitationQrCode4882e => '扫描邀请二维码';

  @override
  String get l6CameraAccessIsUnavailableCopyAnd95be5 => '无法访问相机，可用复制/粘贴邀请码代替。';

  @override
  String get l6DeviceKeysAreHeldInSystem9ad4e =>
      '本机密钥由系统安全存储托管（仅限本设备，不进云备份/云同步）。';

  @override
  String get l6DeviceKeyRecordsAreMissingRecover91b1e =>
      '缺少本机密钥记录。请通过原设备恢复；不会生成替代密钥。';

  @override
  String get l6DeviceKeyRecordsAreDamagedUnlocking1284a =>
      '本机密钥记录损坏，已停止解锁且不会覆盖原记录。';

  @override
  String get l6SystemSecureStorageIsUnavailableOr5cc06 =>
      '系统安全存储不可用或已锁定，请解锁系统密钥库后刷新。';

  @override
  String get l6CheckingDeviceKeyStorage3d187 => '正在检查本机密钥存储…';

  @override
  String get l6AuthorizedDevicesb4736 => '已授权设备';

  @override
  String get l6RefreshDevices7b112 => '刷新设备列表';

  @override
  String get l6OwnerDevicede929 => '主设备';

  @override
  String get l6RunsAiAnalysis6697e => '负责 AI 分析';

  @override
  String get l6ThisDevice8b4c1 => '本机';

  @override
  String get l6Revoke9fcef => '撤销';

  @override
  String get l6TransferAiAnalysisResponsibilityBeforeRevoking455d7 =>
      '需先移交 AI 分析职责后再撤销';

  @override
  String get l6RevokeThisDeviceSSyncAccess2a8d0 => '撤销设备同步权限？';

  @override
  String get l6KnowledgeSyncConflicts67265 => '知识库同步冲突';

  @override
  String get l6KnowledgeChangesFromAnotherDeviceConflict5a183 =>
      '来自其他设备的知识页改动与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。';

  @override
  String get l6NoKnowledgeSyncConflictsNeedReview2246e => '没有待审阅的知识库同步冲突。';

  @override
  String get l6TheIncomingVersionDeletesThisPagec27d8 => '对端已删除本页，而本端之后仍有改动要送达';

  @override
  String get l6BothDevicesRenamedThisPageDifferently200f1 => '两端把本页改名到了不同标题';

  @override
  String get l6TheIncomingPathConflictsWithAnother49464 => '对端的新路径与本端另一页重名';

  @override
  String get l6TheIncomingEditIsBasedOnf0ddf => '对端的修改基于旧版本，本页已先变过';

  @override
  String get l6LocalVersionDeleted25fe7 => '本端（已删除 State）';

  @override
  String get l6ThisPageDoesNotExistLocallya6853 => '（本端没有该页）';

  @override
  String get l6Untitledd8f9b => '（无标题）';

  @override
  String get l6OrganizeTopicscabf2 => '整理主题结构';

  @override
  String get l6MergeSimilarTopicsOrSplitA6542f => '合并相近主题或拆分过大的主题，预览确认后生效';

  @override
  String get l6OldTopicsAreArchivedAfterConfirmationf5e56 =>
      '确认后旧主题归档保留，来源和历史仍可追溯。新主题作为参考知识，不会升级为个人规则。';

  @override
  String get l6SearchTopicsToMergef60aa => '搜索待合并的主题…';

  @override
  String get l6FindTopicsa638d => '查找主题';

  @override
  String get l6PreviewMerge6d8f5 => '预览合并';

  @override
  String get l6PreviewSplit6f910 => '预览拆分本页';

  @override
  String get l6AwaitingConfirmation27b58 => '待确认';

  @override
  String get l6Revoked61063 => '已撤销';

  @override
  String get l6UndoThisBatch7aa0a => '撤销整批整理';

  @override
  String get l6ConfirmAndArchiveOldTopics37635 => '确认整理并归档旧主题';

  @override
  String get l6RejectPlan8b441 => '拒绝方案';

  @override
  String get l6LoadEarlierPlans65e31 => '加载更早整理方案';

  @override
  String get l6RestartTheAppAfterSwitchingToea99d => '切换后重启应用即可看到全部同步数据。';

  @override
  String get l6EnterDatasete3665 => '进入数据集';

  @override
  String get l6CouldNotLoadSyncStatusSyncb8a20 => '同步状态读取失败\n（同步可能尚未启用）';

  @override
  String get l6Processorb4366 => '处理器';

  @override
  String get l6NoSyncYet8ce44 => '尚未同步过';

  @override
  String get l6HeartbeatOnStandbyIdleda794 => '心跳待命（空闲）';

  @override
  String get l6Status62e95 => '状态';

  @override
  String get l6LoadLinkableKnowledgea71cb => '读取可关联知识';

  @override
  String get l6LoadResolutionRevision2bf89 => '读取解决修订';

  @override
  String get l6ResolutionSavedf80e5 => '已保存解决记录';

  @override
  String get l6LinkedResolutionsb26bd => '关联解决记录';

  @override
  String get l6LinkTheIssueToAKnowledge08e6c => '将问题关联到覆盖全部依据的知识修订';

  @override
  String get l6NoKnowledgePageCoversTheseSources10eed =>
      '暂无覆盖这些来源的知识页。请先在相关主题中整理分歧及适用条件，再回到此处关联。';

  @override
  String get l6KnowledgePageThatResolvesTheIssueb3d20 => '解决问题的知识页';

  @override
  String get l6ReviewRevisionContent73b70 => '核对修订正文';

  @override
  String get l6HowThisRevisionResolvesTheDisagreemented5c3 => '这次修订如何处理分歧';

  @override
  String get l6ConfirmLinkedResolution5427f => '确认关联解决';

  @override
  String get l6RefreshLinkableKnowledge5a5b4 => '刷新可关联知识';

  @override
  String get l6LoadAvailableSourcesefee2 => '读取可用来源';

  @override
  String get l6RepairSourceEvidence7c494 => '修复来源依据';

  @override
  String get l6KeepValidSourcesOrChooseReplacementse7069 =>
      '保留有效来源，或选择替代原料；审阅后更新知识';

  @override
  String get l6RejectedOrArchivedSourcesCannotSupport758ff =>
      '不认可或已归档的来源不能继续支持新结论。下方默认保留有效来源，原知识在确认修订前保留。';

  @override
  String get l6FindReplacementSources05a6a => '查找替代原料';

  @override
  String get l6TheFirst50SourcesAreShown7e891 =>
      '列表显示前 50 项，可输入标题定位其他原料。已选来源仍保留。';

  @override
  String get l6UnavailableExcludedFromEvidenceForThe84882 => '不可用，将从新修订依据中排除';

  @override
  String get l6SourcesForThisKnowledgec088b => '当前知识的来源';

  @override
  String get l6AvailableAsAReplacementSourceda692 => '可作为替代来源';

  @override
  String get l6ThereIsNotEnoughEvidenceSelect1b56c =>
      '剩余依据不足。请选择或先导入有效原料；当前知识仍待补证据。';

  @override
  String get l6ChooseUpTo8SourcesNarrow5655d => '最多选择 8 份原料，请缩小范围。';

  @override
  String get l6PrepareRepairProposale29a1 => '准备修复建议';

  @override
  String get l6RefreshSources59684 => '刷新来源';

  @override
  String get l6ReloadSources59500 => '重新读取来源';

  @override
  String get l6LoadReviewHistory6b7da => '读取处理历史';

  @override
  String get l6KnowledgeHistory37e16 => '知识演进';

  @override
  String get l6HistoryOfProposalsAndRevisionsAccepted1535c =>
      '这页内容被建议与修订的历史（采纳 / 拒绝 / 关联解决）';

  @override
  String get l6ThisPageHasNoRevisionsYet1b848 => '这页还没有过修订记录。';

  @override
  String get l6ResolutionNotesca3c9 => '处理说明';

  @override
  String get l6AcceptedChanges114f5 => '采纳的差异段';

  @override
  String get l6SuggestedConditions5bf71 => '建议适用条件';

  @override
  String get l6SavedConditions0ae9a => '保存的适用条件';

  @override
  String get l6LinkedRevision4f2da => '关联修订';

  @override
  String get l6PreviousContent5b344 => '修改前正文';

  @override
  String get l6SavedContentc80ef => '实际保存的正文';

  @override
  String get l6Reload77849 => '重新读取';

  @override
  String get l6LoadMoreRecords37269 => '加载更多记录';

  @override
  String get l6Waiting25d23 => '等待';

  @override
  String get l6Skipped9f38a => '已跳过';

  @override
  String get l6ProcessingQueueb0388 => '整理队列';

  @override
  String get l6ViewPendingWorkReadingProgressAnd0ed68 => '查看积压、阅读进度和等待原因';

  @override
  String get l6NoTasksInThisState7922e => '此状态下暂无任务。';

  @override
  String get l6ReloadQueueb9f40 => '重新读取队列';

  @override
  String get l6LoadMoreTasks1db39 => '加载更多任务';

  @override
  String get l6PrepareSourceRepair2b5a2 => '准备来源修复';

  @override
  String get l6ThereIsNotEnoughEvidenceAddfdb09 =>
      '剩余依据不足，请先补充有效原料。当前知识保留并等待补证据。';

  @override
  String get l6TheSelectedSourcesAreBeingReadb3e01 =>
      '所选原料正在后台阅读全文，请在阅读完成后准备修复建议。';

  @override
  String get l6UpstreamKnowledgeae238 => '上游知识';

  @override
  String get l6UpstreamKnowledgeNeedsReviewOpenItsb9c0d =>
      '上游知识仍待复核，请先打开上游页面检查更新。';

  @override
  String get l6Actionf3ea6 => '操作';

  @override
  String get l6PageDoesNotExist55c9e => '页面不存在';

  @override
  String get l6SourceRecords5bc8d => '来源记录';

  @override
  String get l6SourceRecordDoesNotExist25551 => '来源记录不存在';

  @override
  String get l6CouldNotLoadd9f60 => '读取失败';

  @override
  String get l6LoadSourceRecordse26e1 => '读取来源记录';

  @override
  String get l6DecideOnThisSource4e2fd => '摘录定论';

  @override
  String get l6DiscardAsRejectedKnowledge14322 => '丢弃为负知识';

  @override
  String get l6RequiredReasonForDiscarding0dc5c => '必填：丢弃原因';

  @override
  String get l6OrganizeKnowledge210a3 => '整理知识';

  @override
  String get l6ExtractOnDemandSaveNewReference28c44 => '按需提炼，新增参考知识直接保存';

  @override
  String get l6ExtractAMethod9c1f5 => '提炼方法';

  @override
  String get l6ReusableStepsAndConditions4c52e => '可复用的步骤与适用条件';

  @override
  String get l6OrganizeACaseac694 => '整理案例';

  @override
  String get l6ContextApproachAndOutcomeee83d => '具体情境、做法与结果';

  @override
  String get l6ExtractAPrinciple14a19 => '提炼规律';

  @override
  String get l6PrinciplesEvidenceAndLimits440fe => '规律、依据与适用边界';

  @override
  String get l6ExtractKnowledge196cf => '提炼知识';

  @override
  String get l6IncludeInKnowledge29c9c => '纳入知识';

  @override
  String get l6IncludedInKnowledge9a862 => '已纳入知识';

  @override
  String get l6DiscardedRejectedKnowledge57399 => '已丢弃（负知识）';

  @override
  String get l6ReferenceKnowledgeIsOrganizedAutomaticallyYoucefad =>
      '后台自动整理参考知识；人工修改与规则确认由你决定';

  @override
  String get l6LoadProposal021f2 => '读取提案';

  @override
  String get l6NoChangesNeedYourConfirmationNew743fb =>
      '暂无需要你确认的修改。新增参考知识已直接保存在产出中。';

  @override
  String get l6SourceAndContentChecks5517e => '来源与内容检查';

  @override
  String get l6LoadCheckse2e22 => '读取检查项';

  @override
  String get l6NoChecksNeedReview4053b => '暂无待核对提示。';

  @override
  String get l6FailedAutomaticRetryScheduledc9e00 => '失败，稍后自动重试';

  @override
  String get l6SupportingEvidenceabba3 => '引用依据';

  @override
  String get l6KnowledgeCitedInThisAnswer4ad6e => '本条回答实际引用的知识';

  @override
  String get l6ThisAnswerHasNoSystemVerified0ba58 => '这条回答没有经过系统核验的知识引用。';

  @override
  String get l6SourceVersionDoesNotExist8a671 => '来源版本不存在';

  @override
  String get l6ViewSourceEvent3c3f9 => '查看来源事件';

  @override
  String get l6BackgroundProcessingOrganizesReferenceKnowledgeAnd36787 =>
      '后台会整理参考知识，并结合相关资料维护共同主题。按需提炼的新增产出直接保存；涉及人工修改或已确认内容时才需审阅。原文保留，个人规则由你确认。';

  @override
  String get l6SuggestionsToReview822f0 => '待审建议';

  @override
  String get l6CompiledReferenceKnowledge9efb1 => '已整理的参考知识';

  @override
  String get l6NoKnowledgeOutputsYetYouCan01967 =>
      '暂无知识产出。原文仍可直接使用；后台整理会按内容决定是否形成知识。';

  @override
  String get l6LoadChanges6cd73 => '读取差异';

  @override
  String get l6ReviewProposal8e384 => '处理提案';

  @override
  String get l6Handled219bd => '已处理';

  @override
  String get l6CompareWithCurrentContenta3c20 => '对照当前正文';

  @override
  String get l6CurrentContent47be3 => '当前正文';

  @override
  String get l6LoadPageContent3f16e => '读取页面正文';

  @override
  String get l6ReviewParagraphByParagraph072ad => '逐段审阅';

  @override
  String get l6OriginalParagraphe62c3 => '原段落';

  @override
  String get l6SuggestedParagraphcd73c => '建议段落';

  @override
  String get l6AlsoAcceptTheSuggestedConditionsbd478 => '同时采纳建议的适用条件';

  @override
  String get l6WhichIssuesThisRevisionResolvesYou7c3a0 => '本次修订解决了哪些提示（由你确认）：';

  @override
  String get l6ChangesToSourceEvidenceRequireFull6139c =>
      '如果来源依据同时变化，需要完整确认；局部采纳只用于来源不变的修订。';

  @override
  String get l6ViewSourceMaterialdf8d0 => '查看原料';

  @override
  String get l6LoadSourceSnapshot1cba8 => '读取素材快照';

  @override
  String get l6ViewEventfdcb3 => '查看事件';

  @override
  String get l6HideComparisond3329 => '收起对照';

  @override
  String get l6CompareWithTheCurrentContentConfirming9fbf7 =>
      '请对照当前正文。确认会应用以上修改；若本页是个人规则，也表示你确认修订后的规则及适用条件。';

  @override
  String get l6ConfirmRevision97031 => '确认修订';

  @override
  String get l6RejectSuggestionb7fe0 => '拒绝建议';

  @override
  String get l6HandledOriginalTextPreserved3bc8e => '已处理，原文保持不变';

  @override
  String get l6RejectedDecisionPreserved97f65 => '已拒绝；保留决定记录';

  @override
  String get l6DismissHint0e8a6 => '忽略提示';

  @override
  String get l6OperationFailed09e42 => '操作失败';

  @override
  String get l6DismissCheck617f7 => '忽略检查项';

  @override
  String get l6LoadRevisions0cbe1 => '读取修订记录';

  @override
  String get l6GenerateRestorationProposal29825 => '生成恢复提案';

  @override
  String get l6TheRestorationProposalIsInOutputsde198 =>
      '恢复提案已放入产出页，确认后生效；当前来源和规则设置保留。';

  @override
  String get l6PrepareRestoration94c4c => '准备恢复';

  @override
  String get l6SourceAssessment7740d => '材料评价';

  @override
  String get l6Endorse8a231 => '认可';

  @override
  String get l6Reject802d6 => '不认可';

  @override
  String get l6UseThisAsAPersonalRulefc28c => '确认作为规则使用？';

  @override
  String get l6AiWillConsultThisRuleWhenf912f =>
      'AI 会在符合适用条件时对照这条规则。请确认内容可靠且适用条件准确。';

  @override
  String get l6ConfirmUpgrade8c3fd => '确认升级';

  @override
  String get l6SaveMetadata757dd => '保存元数据';

  @override
  String get l6Conditions24481 => '适用条件';

  @override
  String get l6ReferenceMaterialc3d33 => '参考材料';

  @override
  String get l6ReusableMethod12837 => '可复用方法';

  @override
  String get l6ConfirmedRuleba4b1 => '已确认规则';

  @override
  String get l6SaveConditionsAndStrength46f33 => '保存适用条件与强度';

  @override
  String get l6SourcesLetYouVerifyEvidenceRevisionsccae6 =>
      '来源用于核对依据，修订记录用于查看知识演进。日常阅读和 AI 检索不需要逐项操作。';

  @override
  String get l6UpstreamBasis0fd54 => '上游依据';

  @override
  String get l6NoVerifiableSourcesYet8b47f => '尚无可核验来源。';

  @override
  String get l6SourceEvents2c4b3 => '来源事件记录';

  @override
  String get l6OriginalSourceVersions24704 => '原文历史版本';

  @override
  String get l6KnowledgeRevisions42307 => '知识修订记录';

  @override
  String get l6ViewAndRestorePastRevisionsOfd9312 => '查看本页正文的历次修订与恢复';

  @override
  String get l6SourceIsTemporarilyUnavailablecb78e => '来源暂时无法读取';

  @override
  String get l6DeviceInformation67298 => '本机信息';

  @override
  String get l6Rename1cd80 => '重命名';

  @override
  String get l6DeviceId464c0 => '本机 ID：';

  @override
  String get l6RenameThisDevice304fd => '重命名本机设备';

  @override
  String get l6DeviceNamed889e => '设备名';

  @override
  String get l6RelayServiceUnlockDeviceKeysTo0103f => '中继服务：解锁本机密钥后可见';

  @override
  String get l6RelayServiceda32a => '中继服务：';

  @override
  String get l6CopyRelayAddress61683 => '复制中继地址';

  @override
  String get l6RelayAddressCopied7db6a => '中继地址已复制';

  @override
  String get l6Restore79748 => '恢复';

  @override
  String get l6TransferAiAnalysisResponsibilityfe457 => 'AI 分析职责移交';

  @override
  String get l6ThisDeviceIsNotInvolvedInaf62a => '本机状态：未参与移交。';

  @override
  String get l6ResponsibilityHasBeenHandedOffWaitingeac50 =>
      '本机已让出 AI 分析职责，等待目标设备确认接管。';

  @override
  String get l6AHandoffAwaitsThisDeviceSdd081 => '有移交等待本机确认（同步已追平）。';

  @override
  String get l6AHandoffAwaitsConfirmationButThis0f114 =>
      '有移交等待本机确认，但本机尚未同步到移交边界。';

  @override
  String get l6ThisDeviceRunsAiAnalysisThe54d4b => '本机正在负责 AI 分析（此前的一次接管已完成）。';

  @override
  String get l6AcceptResponsibility6c220 => '确认接管';

  @override
  String get l6NoEligibleTargetDevicesAuthorizeAaba52 => '暂无可接收设备，请先授权设备加入。';

  @override
  String get l6TargetDevice1b73f => '目标设备';

  @override
  String get l6TransferAiAnalysisResponsibility2e5ba => '让出 AI 分析职责';

  @override
  String get l6WaitingForTheOwnerToApprovebf73f => '等待主设备审批移交证明。';

  @override
  String get l6ProcessingHandoffed58d => '正在处理移交步骤，请等待…';

  @override
  String get l6TransferAiAnalysisResponsibility5d2ad => '让出 AI 分析职责？';

  @override
  String get l6Transferc79cc => '让出';

  @override
  String get l6JoinWithAnInvitation78bca => '通过邀请码加入';

  @override
  String get l6InvitationFromATrustedDeviceade9c => '来自可信设备的邀请码';

  @override
  String get l6PreviewInvitationOfflinee39bc => '预览邀请（不联网）';

  @override
  String get l6JoinAnExistingDatasetb3b50 => '加入现有数据集';

  @override
  String get l6PreviewAndVerifyInvitation1afde => '预览并核对邀请';

  @override
  String get l6SendJoinRequestd5d20 => '发送加入请求';

  @override
  String get l6WaitingForOwnerApproval9e900 => '等待主设备批准';

  @override
  String get l6ConfirmInstallation555cc => '安装确认';

  @override
  String get l6WaitingForAuthorizationAndInitialSync6cedf => '等待成员授权与初始同步';

  @override
  String get l6ReceivingTheInitialSnapshot032cf => '正在接收初始快照';

  @override
  String get l6RelayMailboxFullDeliveryResumesAutomatically4a933 =>
      '中继信箱已满：对方正在取件腾出空间，稍候会自动继续，无需操作。';

  @override
  String get l6ScanQrCodeb8a8f => '扫码';

  @override
  String get l6RelayToConnectTo06ef4 => '将连接的中继';

  @override
  String get l6InvitingDeviceSPublicKeyFingerprint283a5 => '邀请设备公钥指纹';

  @override
  String get l6ThisDeviceSPublicKeyFingerprintb7e3a => '本机公钥指纹（请在已有设备核对）';

  @override
  String get l6ReviewTheInvitationThenExplicitlyChoosece487 => '请核对邀请后明确选择加入。';

  @override
  String get l6PreparingAndVerifyingProtectedDeviceKeys60dc8 =>
      '正在准备并确认本机受保护密钥。';

  @override
  String get l6DeviceKeysAreReadyRequestDeliverye8427 => '本机密钥已准备，尚未确认请求投递。';

  @override
  String get l6WaitingForAnExistingDeviceTo330c1 => '等待已有设备核对指纹并明确批准。';

  @override
  String get l6WaitingForSystemConfirmationNoProof188b0 => '等待系统确认，确认前不发送证明。';

  @override
  String get l6InstallationIsSavedProofDeliveryIsf28de => '安装已保存，尚未确认安装证明投递。';

  @override
  String get l6InstallationProofDeliveredWaitingForAuthorization5d65b =>
      '安装证明已投递，等待授权与初始同步。';

  @override
  String get l6CoordinatingTheApprovedHandoffAutomaticallyCheck9b863 =>
      '正在自动协调已批准的初始交接，请查看交接进度。';

  @override
  String get l6ConfirmingHandoffRevisionsSendingResumesAftera5068 =>
      '正在确认交接修订，确认前不继续发送。';

  @override
  String get l6TheInvitationExpiredRequestANewf2a42 => '邀请已过期，需重新邀请。';

  @override
  String get l6JoiningRequiresANewInvitationOr2a99d => '加入需要重新邀请或恢复。';

  @override
  String get l6NewDeviceNamed42db => '新设备名称';

  @override
  String get l6IHaveCheckedTheInvitationAnd15ccd =>
      '我已核对邀请和中继，明确同意连接该地址，并接收全部 Provider 配置和 API Key。';

  @override
  String get l6RequestToJoinUsingASeparate835e5 => '在独立暂存库发起加入';

  @override
  String get l6ResendOriginalJoinRequest97725 => '重发原加入请求';

  @override
  String get l6CheckForApproval58c9a => '检查已有设备审批';

  @override
  String get l6ResendSavedInstallationProof3d80a => '重发已保存的安装证明';

  @override
  String get l6StopThisDeviceSEnrollmentb3a98 => '停止本机加入流程';

  @override
  String get l6ApprovalCleanupIsNotConfirmedResending9fd47 =>
      '审批邮件的清理尚未确认；重发证明可再次确认清理，不会重新安装。';

  @override
  String get l6ViewRecoverableEnrollmentStagingEntriesf095c => '查看可恢复的加入暂存项';

  @override
  String get l6UnlockAndRecovere9f7c => '解锁恢复';

  @override
  String get l6DeleteThisStagingEntrya06bf => '删除这条暂存项？';

  @override
  String get l6TheCurrentStorageBackendDoesNot2cb78 => '当前存储后端不支持设备同步。';

  @override
  String get l6SyncIsEnabled210fe => '同步已开启';

  @override
  String get l6SyncIsPausedThisDeviceS7fecc => '同步已暂停（本机密钥已锁定）';

  @override
  String get l6SyncIsDisabledThisDeviceIs6aad5 => '同步已关闭（本机未被授权）';

  @override
  String get l6SyncNeedsRepair8e4fa => '同步需要修复';

  @override
  String get l6SyncIsNotEnabledbccba => '同步未开启';

  @override
  String get l6LoadingSyncStatus861b0 => '正在读取同步状态';

  @override
  String get l6SyncStatusIsUnavailable0971a => '同步状态暂不可用';

  @override
  String get l6OwnerDeviceRunsBackgroundAiAnalysis73dc6 => '主设备 · 负责后台 AI 分析';

  @override
  String get l6MemberDeviceRunsBackgroundAiAnalysisf055d => '成员设备 · 负责后台 AI 分析';

  @override
  String get l6MemberDevicee071b => '成员设备';

  @override
  String get l6Pairing6aed7 => '绑定';

  @override
  String get l6Manage4989b => '管理';

  @override
  String get l6Clearingf54a6 => '正在清除…';

  @override
  String get l6ClearThisDeviceSSyncIdentityab953 => '清除本机同步身份';

  @override
  String get l6SavedProviderApiKeysArePreservedb2ce9 =>
      '· 已保存的 Provider API Key 会自动保留（重新配对后自动迁移）；';

  @override
  String get l6SavedProviderApiKeysWillStop714a6 =>
      '· 已保存的 Provider API Key 会失效，需要重新填写；';

  @override
  String get l6IUnderstandThatThisDeviceMust3caf2 => '我已知晓，本机需要重新配对才能同步';

  @override
  String get l6IUnderstandThatProviderApiKeys4b028 =>
      '我已知晓，Provider API Key 需要重新填写';

  @override
  String get l6ClearThisDeviceSSyncIdentityf7c83 => '清除本机同步身份？';

  @override
  String get l6AfterClearing12369 => '清除后：';

  @override
  String get l6ThisDeviceLeavesTheDatasetAnd73b97 =>
      '· 本机退出当前数据集，需要重新配对或重新加入才能再同步；';

  @override
  String get l6ChangesNotYetSyncedToOther8c274 => '· 还没同步到其它设备的改动不会自动补发。';

  @override
  String get l6NotesConversationsAndKnowledgeArePreserved9b3e9 =>
      '笔记、对话、知识库不会被删除。';

  @override
  String get l6ClearAndPairAgain845d1 => '清除并重新配对';

  @override
  String get l6DeviceName4433e => '设备名称';

  @override
  String get l6PrepareThisDevice9d671 => '准备本机';

  @override
  String get l6PrepareThisDeviceForSyncee24c => '准备本机加入同步？';

  @override
  String get l6IUnderstandThatAuthorizedDevicesWill1e411 =>
      '我理解：后续授权设备加入时，会共享所有 Provider 配置和 API Key。';

  @override
  String get l6IUnderstandPrepareThisDevice9598b => '我理解并准备';

  @override
  String get l6DeviceSyncIsDisabledInThise1b42 =>
      '设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。';

  @override
  String get l6AfterPairingDeviceListsAndSync6d4f5 => '完成绑定后，这里会显示设备列表与同步管理。';

  @override
  String get l6DeviceIdIsUnavailableRefreshAnd762d2 => '本机设备 ID 不可见，请刷新。';

  @override
  String get l6AuthorizedDevicesAppearHereAfterThey67f15 =>
      '授权其他设备后，这里会显示设备列表。';

  @override
  String get l6LockDeviceKeys6ac4e => '锁定本机密钥';

  @override
  String get l6UnlockDeviceKeysa84f8 => '解锁本机密钥';

  @override
  String get l6RefreshStatus7cc7f => '刷新状态';

  @override
  String get l6Diagnostics0b673 => '诊断信息';

  @override
  String get l6DatasetId395bc => '数据集标识';

  @override
  String get l6ConfirmDeviceEnrollment159c2 => '确认加入设备';

  @override
  String get l6CompareThePublicKeyFingerprintWitha360c => '请与新设备核对公钥指纹：';

  @override
  String get l6ConfirmThatYouTrustThisDevice20292 =>
      '确认这台设备可信，并允许其获得全部 Provider 配置和 API Key。';

  @override
  String get l6Back11d02 => '返回';

  @override
  String get l6ApproveThisDeviced11af => '批准这台设备';

  @override
  String get l6InviteADevice9f3fa => '邀请设备';

  @override
  String get l6NewDevicesCanRequestToJoinbe25a =>
      '新设备可通过“加入现有数据集”发起加入；批准后自动完成交接。';

  @override
  String get l6YourRelayUrl9ff3a => '你已部署的中继 URL';

  @override
  String get l6IUnderstandThatCreatingAnInvitation5f3db =>
      '我理解：创建邀请会连接此中继；批准加入后将共享 Provider 配置和 API Key。';

  @override
  String get l6CreateATemporaryInvitationa2809 => '创建短期邀请';

  @override
  String get l6InvitationCreatedShowTheQrCode15986 => '邀请已生成（出示二维码或复制邀请）';

  @override
  String get l6ANewDeviceHasRequestedTo3129b => '新设备已发起加入请求';

  @override
  String get l6ApprovedDeliveringApprovalc4bcf => '已批准，正在投递审批';

  @override
  String get l6ApprovalDeliveredWaitingForTheNewaeaa7 => '审批已送达，等待新设备完成安装';

  @override
  String get l6NewDeviceJoinsCheckTheDevice29220 => '新设备接入（在「管理」确认设备列表）';

  @override
  String get l6PreparingTheInitialSnapshotb1613 => '正在准备初始快照';

  @override
  String get l6WaitingForInitialSnapshotSelectione32c8 => '等待选择初始快照';

  @override
  String get l6SendingTheInitialSnapshotfa3fd => '正在发送初始快照';

  @override
  String get l6ShareInvitationsOnlyWithTrustedDevices47059 => '邀请码仅交给可信设备。';

  @override
  String get l6CopyInvitationf7293 => '复制邀请';

  @override
  String get l6CancelInvitation17e4c => '取消邀请';

  @override
  String get l6VerifyAndApprove5c161 => '核对并审批';

  @override
  String get l6ResendSavedApproval48def => '重发已保存的审批';

  @override
  String get l6ProviderSyncConflicts31e02 => 'Provider 同步冲突';

  @override
  String get l6KeysAreMaskedKeepTheLocal2e55f =>
      '密钥已掩码。选择「保留本地」或「接受远端」整包应用，不做字段拼接；地址提示仅为静态分析，不代表实际可达。';

  @override
  String get l6NoProviderSyncConflictsNeedReview4dc86 =>
      '没有待审阅的 Provider 同步冲突。';

  @override
  String get l6CurrentLocalConfiguration683a0 => '本机当前配置';

  @override
  String get l6RemoteConfiguration04e8c => '远端配置';

  @override
  String get l6KeepLocal6d6a5 => '保留本地';

  @override
  String get l6AcceptRemote4546a => '接受远端';

  @override
  String get l6NoConfigurationa33dc => '（无配置）';

  @override
  String get l6KeyNotSetefd96 => 'Key：未设置';

  @override
  String get l6LoopbackAddressReachableOnlyFromThis5a7ca =>
      '环回地址：仅本机可达，同步到其他设备后无法访问';

  @override
  String get l6PrivateNetworkAddressReachableOnlyOnf9ec8 => '内网地址：仅同一网络内设备可达';

  @override
  String get l6MissingHttpSPrefixMostClients85171 =>
      '缺少 http(s):// 前缀，多数客户端无法识别';

  @override
  String get l6UnusualProtocolMayNotBeSupportedf9ae0 => '非常规协议，可能不被支持';

  @override
  String get l6MissingHostNameb705c => '缺少主机名';

  @override
  String get l6LastSyncSkippedLockedOrBackground113af => '最近同步：本轮跳过（锁定或后台忙碌）';

  @override
  String get l6RelayMailboxFullWaitingForThec480e => '中继信箱已满，等对方取件';

  @override
  String get l6RelayMessageFormatMismatchUpdateThe1d3c3 => '与中继的消息格式不一致，需要更新版本';

  @override
  String get l6RepeatedContentValidationFailuresDeliveryPausedb04cc =>
      '内容校验反复失败，已暂停投递';

  @override
  String get l6NetworkIsUnstableRetrying2aaa3 => '网络不稳定，正在重试';

  @override
  String get l6Resyncing0f8da => '正在重新同步';

  @override
  String get l6DeliveredToRelayWaitingForPeer2bf3e => '已送达中继，等对方确认';

  @override
  String get l6Idle837e7 => '空闲';

  @override
  String get l6RetryingSoone5ae3 => '即将重试';

  @override
  String get l6DeleteEmotionAnnotationa51b5 => '删除情绪标注';

  @override
  String get l6DeleteRelationship16bbd => '删除关系';

  @override
  String get l6NoRelationshipsOrEmotionAnnotationsYetbecb5 =>
      '还没有积累。对话或快录里提到的人物与项目，经你确认（或 @/# 标注的权威组合）后会在这里长成一份可查可删的关系账本；有客观情绪反应的记录会给出简短文字标注（无数值、不是情绪分析报告）。删掉任何一条都不会动原始记录。';

  @override
  String l6LastArg0187f4(String arg0) {
    return '最近 $arg0';
  }

  @override
  String get l6DeleteThisRelationshipPreserveOriginalRecord6cc81 =>
      '删除这条关系（原始记录不动）';

  @override
  String get l6EmotionAnnotationsab8ab => '情绪标注';

  @override
  String get l6AboutThisEventbc042 => '对这件事';

  @override
  String l6AboutArg0171b0(String arg0) {
    return '对 $arg0';
  }

  @override
  String get l6DeleteThisEmotionAnnotationPreserveOriginal8d148 =>
      '删除这条情绪标注（原始记录不动）';

  @override
  String get l6OtherDimensionsCommitmentsReflectionPaceBoundaries873e8 =>
      '其余维度（承诺、复盘、节奏、偏好红线、健康、财务、地点、身份、创作线）以知识库页面承载，在知识库中查看与编辑。';

  @override
  String get l6EventRemindersaa957 => '事件提醒';

  @override
  String get l6ImmediateAndInReviewsd066b => '即时+回顾';

  @override
  String get l6ReviewsOnly71ead => '仅回顾';

  @override
  String get l6SaveReminderPreferencese0188 => '保存提醒偏好';

  @override
  String get l6EmotionalCheckIns74925 => '情感回访';

  @override
  String get l63Daysb30a9 => '3天';

  @override
  String get l67Daysecfe0 => '7天';

  @override
  String get l614Days1e982 => '14天';

  @override
  String get l630Daysff788 => '30天';

  @override
  String get l6SaveRevisitPreferences8123a => '保存回访偏好';

  @override
  String get l6RecordReminderFeedbackcfbf1 => '记录提醒反馈';

  @override
  String get l6Assessingd7b92 => '判定中';

  @override
  String get l6FollowedInConversationa3d02 => '已在对话遵守';

  @override
  String get l6NotApplicableeec46 => '不适用';

  @override
  String get l6RuleContextsInConversationsEvaluatedSilently95c0c =>
      '对话中的规则场景（后台静默判定，不打扰对话）';

  @override
  String get l6RecentReminderse3fba => '最近提醒';

  @override
  String l6HelpfulArg0NotRelevantArg18e65c(String arg0, String arg1) {
    return '有用 $arg0 · 不相关 $arg1';
  }

  @override
  String get l6TriggerConditionsf5c95 => '触发条件';

  @override
  String get l6TriggerKeywordsCommaSeparatedBlankDisablesdae9a =>
      '触发关键词（逗号分隔，留空 = 不预筛）';

  @override
  String get l6ForExampleProjectXDeadlinef216a => '如：X项目, deadline';

  @override
  String get l6TriggerContext02570 => '触发场景';

  @override
  String get l6AllContexts39fdd => '全场景';

  @override
  String get l6QuickCapturecfe6c => '快速捕获';

  @override
  String get l6DailyReview6bea3 => '每日回顾';

  @override
  String get l6SaveTriggerConditions8e04c => '保存触发条件';

  @override
  String get l6DeleteRulef9ad3 => '删除规则';

  @override
  String get l6PersonalRules03b44 => '个人规则库';

  @override
  String get l6UserModel88c56 => '用户模型';

  @override
  String get l6NoRulesYetWhenYouDiscuss96049 =>
      '还没有规则。在对话里分享踩坑或心得时，AI 会建议把其中的经验沉淀成规则，你在审阅卡接受具体规则后就会出现在这里，以后遇到类似情况 AI 会主动引用并提醒你。';

  @override
  String get l6TriggerConditionsConfigureda3704 => '触发条件（已配置）';

  @override
  String get l6ConfigureTriggerConditions168ff => '配置触发条件';

  @override
  String get l6PlaceEachSkillFolderWithIts94f37 =>
      '把技能目录放进 <数据目录>/skills（每个技能一个子目录，含 SKILL.md），对话中由模型按需加载；脚本执行前会单独向你确认。把某个技能标为「信任」后，它的脚本调用直接执行、不再审批——脚本可在本机执行任意操作，开启即视为你了解并自担风险。';

  @override
  String get l6InstallFromFolder22a87 => '从文件夹安装';

  @override
  String get l6InstallFromZip906f1 => '从 zip 安装';

  @override
  String get l6InstallFromGit0882e => '从 Git 安装';

  @override
  String get l6OpenSkillsFoldera4437 => '打开技能目录';

  @override
  String l6SelectedArg0Arg1fd75c(String arg0, String arg1) {
    return '已选 $arg0 / $arg1';
  }

  @override
  String l6TotalArg01e668(String arg0) {
    return '共 $arg0 个';
  }

  @override
  String get l6ExitSelectione23ed => '退出选择';

  @override
  String get l6DeleteMultiple362ae => '批量删除';

  @override
  String get l6DeselectAllba20e => '全不选';

  @override
  String get l6SelectAll3e44b => '全选';

  @override
  String l6DeleteSelectedArg0def95(String arg0) {
    return '删除所选 ($arg0)';
  }

  @override
  String get l6LoadingSkills8d716 => '正在读取技能列表…';

  @override
  String get l6NoUserSkillsInstalledInstallAe8aea =>
      '尚未安装任何用户技能\n从文件夹 / zip / Git 仓库安装第一个技能';

  @override
  String get l6LoadUserSkillsfb79f => '读取用户技能';

  @override
  String get l6ChooseASkillZipFile0e485 => '选择技能 zip 包';

  @override
  String get l6ChooseASkillFolderContainingSkill8789f => '选择包含 SKILL.md 的技能目录';

  @override
  String get l6InstallFromZip37d9a => 'zip 安装';

  @override
  String get l6InstallFromFolder71cd7 => '目录安装';

  @override
  String get l6InstallASkillFromGitfa921 => '从 Git 仓库安装技能';

  @override
  String get l6RepositoryUrlHttpsOrLocalPath3ae69 => '仓库地址（https 或本地路径）';

  @override
  String get l6Install087db => '安装';

  @override
  String get l6InstallFromGit6d266 => 'Git 安装';

  @override
  String l6InstalledArg0EnabledByDefaultdd2c2(String arg0) {
    return '已安装 $arg0，默认启用';
  }

  @override
  String get l6UninstallSkill053a6 => '卸载技能';

  @override
  String l6Arg0WillBeRemovedFromThis1a7c6(String arg0) {
    return '$arg0 将从本机删除；若已同步，其他设备也会一并删除。';
  }

  @override
  String get l6Uninstall200cf => '卸载';

  @override
  String get l6UninstallMultipleSkills13309 => '批量卸载技能';

  @override
  String l6SkillsToRemoveArg0SyncedCopiesaa41a(String arg0) {
    return '共 $arg0 个技能将从本机删除；若已同步，其他设备也会一并删除。';
  }

  @override
  String l6UninstallSkillArg0a3819(String arg0) {
    return '卸载技能 $arg0';
  }

  @override
  String l6SkillsUninstalledArg08db74(String arg0) {
    return '已卸载 $arg0 个技能';
  }

  @override
  String l6SucceededArg0FailedArg1Arg2Arg3a2cde(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return '成功 $arg0 个，失败 $arg1 个：$arg2$arg3';
  }

  @override
  String get l6ToggleUserSkill2de88 => '切换用户技能';

  @override
  String get l6MarkedAsTrustedThisSkillS1cf36 => '已标为信任：该技能的脚本调用不再弹审批卡';

  @override
  String get l6ChangeTrustSetting7ce93 => '切换信任状态';

  @override
  String get l6KnowledgeExport26466 => '知识导出';

  @override
  String get l6DatabaseMaintenance60286 => '数据库维护';

  @override
  String get l6DailyTokenUsage5a651 => '每日 Token 使用';

  @override
  String get l6TrustedNoApproval69302 => '信任·免审批';

  @override
  String get l6RequireApprovalEachTime673dd => '改为逐次审批';

  @override
  String get l6TrustNoApproval12269 => '标为信任（免审批）';

  @override
  String get l6EnabledAPasswordIsRequiredWhen1aee3 => '已开启：启动 App 时需要输入密码';

  @override
  String get l6DisabledTheAppOpensDirectlycbfe4 => '未开启：启动 App 直接进入主界面';

  @override
  String get l6SetAnAppPassword72f45 => '设置打开密码';

  @override
  String get l6ChangePassword0ae20 => '修改密码';

  @override
  String get l6TurnOffPassword7f7cb => '关闭密码';

  @override
  String get l6OnlyASaltedPasswordHashIsd5644 =>
      '密码只以加盐哈希形式保存在本机系统安全存储，不会上传、不会随同步带走。遗忘密码后无法找回，只能清除应用数据重置。';

  @override
  String get l6FingerprintOrFaceUnlock1b96c => '指纹/面容解锁';

  @override
  String get l6UseSystemBiometricsForQuickUnlocking22994 =>
      '开启后可用系统生物验证快速解锁，密码始终保留为兜底';

  @override
  String get l6ThisDeviceDoesNotSupportBiometrics40ff0 => '当前设备不支持或未录入指纹/面容';

  @override
  String get l6EnableFingerprintOrFaceUnlockb2a8f => '开启指纹/面容解锁';

  @override
  String get l6TheLockScreenWillFirstOffer49b2b =>
      '开启后锁屏页会优先调起系统指纹/面容验证。请输入当前打开密码确认。';

  @override
  String get l6CurrentPassword33abf => '当前密码';

  @override
  String get l6ConfirmEnable263d4 => '确认开启';

  @override
  String get l6IncorrectCurrentPasswordNotEnabled691f1 => '当前密码不正确，未开启';

  @override
  String get l6BiometricUnlockEnablede15eb => '指纹/面容解锁已开启';

  @override
  String get l6BiometricsAreCurrentlyUnavailableNotEnabled78c56 =>
      '设备当前不可用生物验证，未开启';

  @override
  String get l6AppPasswordSeta5390 => '打开密码已设置';

  @override
  String get l6ChangeAppPassword25e9a => '修改打开密码';

  @override
  String get l6AppPasswordChanged0493a => '打开密码已修改';

  @override
  String get l6TurnOffAppPassworda80d7 => '关闭打开密码';

  @override
  String get l6TheAppWillNoLongerRequire00cc0 =>
      '关闭后启动 App 将不再要求输入密码。请输入当前密码确认。';

  @override
  String get l6ConfirmDisable059b4 => '确认关闭';

  @override
  String get l6AppPasswordTurnedOff9f4c1 => '打开密码已关闭';

  @override
  String get l6IncorrectCurrentPasswordNotDisabled0660c => '当前密码不正确，未关闭';

  @override
  String get l6EnterTheCurrentPassword4b0b0 => '请输入当前密码';

  @override
  String get l6EnterANewPassword689e5 => '新密码不能为空';

  @override
  String get l6TheAppPasswordMustHave41cb25 => '打开密码固定为 4 位';

  @override
  String get l6TheNewPasswordsDoNotMatchd0b9c => '两次输入的新密码不一致';

  @override
  String get l6TheCurrentPasswordIsIncorrectb0dab => '当前密码不正确';

  @override
  String get l6CouldNotSaveTryAgainae0b2 => '设置失败，请重试';

  @override
  String get l6NewPassword4Digits51018 => '新密码（4 位）';

  @override
  String get l6ConfirmNewPassworde0327 => '再次输入新密码';

  @override
  String get l6Confirmf526c => '确定';

  @override
  String get l6ExportKnowledge863fb => '导出知识库';

  @override
  String get l6LoadDatabaseSize69850 => '读取库体量';

  @override
  String get l6OptimizeDatabase12519 => '整理数据库';

  @override
  String get l6FetchingServicebfb3a => '抓取服务';

  @override
  String get l6PostFetchingServiceUpdated1c8a1 => '推文抓取服务已更新';

  @override
  String get l6SavePostFetchingService23d23 => '保存推文抓取服务';

  @override
  String get l6ChooseASecretReference6a79c => '选择密钥引用';

  @override
  String get l6AddASecretInSecuritySecretb6b03 => '请先在安全 → 密钥管理中添加密钥';

  @override
  String get l6PostFetching22fc8 => '推文抓取';

  @override
  String get l6WebSearch678ed => '外网搜索';

  @override
  String get l6OutboundProxyabb5e => '出站代理';

  @override
  String get l6EncryptedDnsDohc262a => 'DoH 加密 DNS';

  @override
  String get l6UserSkills748e7 => '用户技能';

  @override
  String get l6McpServersad57a => 'MCP 服务器';

  @override
  String get l6ConnectModelContextProtocolServersTo6e5c7 =>
      '连接 Model Context Protocol 服务器，把外部工具带入对话。工具以 mcp__<服务器>__<工具名> 出现，模型按需调用；stdio 服务器以指定命令作为子进程启动（首期只支持 stdio 传输）。';

  @override
  String get l6EachToolCallRequiresApprovalBycbb3e =>
      '默认每次工具调用都会弹出审批卡逐次确认。把某个服务器标为「信任」后，它的调用将直接执行、不再审批——MCP 服务器是第三方代码，模型可能被诱导发起破坏性调用，开启即视为你了解并自担风险。若只想豁免确定只读的查询，可开每台服务器的「只读免审批」（仅对上游标了 readOnlyHint 的工具生效，其余照旧审批）。';

  @override
  String l6EnabledArg0Arg1d02a1(String arg0, String arg1) {
    return '已启用 $arg0 / $arg1';
  }

  @override
  String get l6AddServer35085 => '添加服务器';

  @override
  String get l6LoadingServers3023e => '正在读取服务器列表…';

  @override
  String get l6NoMcpServersAddedChooseAddac3f0 =>
      '尚未添加任何 MCP 服务器\n点右上角「添加服务器」接入第一个外部工具源';

  @override
  String get l6LoadMcpServers1900c => '读取 MCP 服务器';

  @override
  String get l6MarkedAsTrustedThisServerS74676 => '已标为信任：该服务器的工具调用不再弹审批卡';

  @override
  String get l6ReadOnlyExemptionEnabledForTools0d43a =>
      '已开只读免审批：仅标了 readOnlyHint 的工具不再弹审批卡；仍以上游标注为准，写类工具照旧逐次审批';

  @override
  String get l6ChangeReadOnlyApprovalExemptione8168 => '切换只读免审批';

  @override
  String get l6ToggleMcpServer14dd0 => '切换 MCP 服务器';

  @override
  String get l6DeleteMultipleServers13274 => '批量删除服务器';

  @override
  String l6ServersToRemoveArg0SyncedCopiesccae5(String arg0) {
    return '共 $arg0 台服务器将从本机删除；若已同步，其他设备也会一并删除。';
  }

  @override
  String l6DeleteMcpServerArg0f17d5(String arg0) {
    return '删除 MCP 服务器 $arg0';
  }

  @override
  String l6ServersDeletedArg05fc2a(String arg0) {
    return '已删除 $arg0 台服务器';
  }

  @override
  String l6SucceededArg0FailedArg1Arg2Arg312d85(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return '成功 $arg0 台，失败 $arg1 台：$arg2$arg3';
  }

  @override
  String get l6DeleteMcpServer34624 => '删除 MCP 服务器';

  @override
  String get l6AddMcpServerc9d22 => '添加 MCP 服务器';

  @override
  String get l6EditMcpServer4c8df => '编辑 MCP 服务器';

  @override
  String get l6NameLettersDigitsHyphensOrUnderscores0dc18 =>
      '名称（字母/数字/短横线/下划线）';

  @override
  String get l6RelatedIntentKeywordsOptionalb6b1f => '关联意图关键词（可选）';

  @override
  String get l6ForExampleProductReviewsRecommendationsSales606d3 =>
      '小红书, 种草, 卖货';

  @override
  String get l6SeparateKeywordsWithCommasToHelp6c10b =>
      '用逗号分隔，帮助 AI 在相关任务中优先选择此工具（最多 256 字）';

  @override
  String get l6LocalCommandaa556 => '本机命令';

  @override
  String get l6RemoteServer1b481 => '远程服务器';

  @override
  String get l6Commandb114b => '命令';

  @override
  String get l6ArgumentsJsonArray5ffa9 => '参数（JSON 数组）';

  @override
  String get l6InsertASecretReference6edc7 => '插入密钥引用';

  @override
  String get l6EnvironmentVariablesJsonObject364cf => '环境变量（JSON 对象）';

  @override
  String get l6EndpointUrl28548 => '端点 URL';

  @override
  String get l6CustomHeadersJsonObjectAuthenticationEtcd9233 =>
      '自定义 Header（JSON 对象，鉴权等）';

  @override
  String get l6TrustedCallsSkipApproval6254b => '信任（调用免审批）';

  @override
  String get l6ToolsFromThisServerExecuteWithoutf61a4 =>
      '开启后该服务器的工具调用直接执行，不再逐次弹审批卡。MCP 服务器是第三方代码，请只对你信任的服务器开启。';

  @override
  String get l6TestConnection10b7d => '测试连接';

  @override
  String get l6SaveMcpServer25c8a => '保存 MCP 服务器';

  @override
  String get l6EnterACommandBeforeTesting41aae => '先填命令再测试';

  @override
  String get l6EnterAnHttpSEndpointBeforec28cd => '先填 http(s) 端点 URL 再测试';

  @override
  String get l6ConnectedButTheServerProvidedNod0d33 => '连接成功，但该服务器未提供工具';

  @override
  String l6ConnectedArg0Arg1c4ea4(String arg0, String arg1) {
    return '连接成功：$arg0$arg1';
  }

  @override
  String get l6TestMcpConnection1eed1 => 'MCP 测试连接';

  @override
  String get l6Remote80194 => '远程';

  @override
  String get l6AllPlatforms52e2a => '全部平台';

  @override
  String l6OnlyArg064396(String arg0) {
    return '仅 $arg0';
  }

  @override
  String get l6ReadOnlyCallsSkipApproval63678 => '只读免审批';

  @override
  String l6RelatedIntentsArg0dff5f(String arg0) {
    return '关联意图：$arg0';
  }

  @override
  String get l6DisableReadOnlyApprovalExemptioned6aa => '关闭只读免审批';

  @override
  String get l6EnableReadOnlyApprovalExemption95d1c => '开只读免审批';

  @override
  String get l6AiAnalysisec6c3 => 'AI 分析';

  @override
  String l6BranchesArg079379(String arg0) {
    return '$arg0 个分支';
  }

  @override
  String l6Arg0MinutesAgo6b0c6(String arg0) {
    return '$arg0 分钟前';
  }

  @override
  String l6Arg0HoursAgoaf702(String arg0) {
    return '$arg0 小时前';
  }

  @override
  String l6Arg0DaysAgo70636(String arg0) {
    return '$arg0 天前';
  }

  @override
  String get l6ReadingSettings9033d => '正文阅读设置';

  @override
  String get l6TheThreeSettingsAreIndependentFollow172b0 =>
      '三项独立生效；「跟随全局」恢复继承，两个滑块和字体互不影响。';

  @override
  String get l6Fontb50d4 => '字体';

  @override
  String get l6InheritGlobal2a499 => '继承全局';

  @override
  String get l6OverrideGlobal93d3e => '覆盖全局';

  @override
  String get l6FontSize576cc => '字号';

  @override
  String get l6LineSpacing514b5 => '行距';

  @override
  String get l6ChooseAReadingFont8189e => '选择正文阅读字体';

  @override
  String get l6SetUpAnAiProviderToc8ad4 =>
      '首次使用提示：尚未配置 AI Provider，AI 对话与自动分析暂不可用。';

  @override
  String get l6Configurea44df => '去配置';

  @override
  String get l6Queuecd354 => '队列';

  @override
  String get l6NoRecords09f60 => '没有记录';

  @override
  String get l6FailedCoolingDown4a20e => '失败（冷却中）';

  @override
  String l6RecordedArg085217(String arg0) {
    return '记录于 $arg0';
  }

  @override
  String l6AttemptsArg0fcc38(String arg0) {
    return '尝试 $arg0 次';
  }

  @override
  String l6AutomaticallyRequeuedArg0Times3363a(String arg0) {
    return '已自动回队 $arg0 轮';
  }

  @override
  String l6NextAttemptArg0272da(String arg0) {
    return '下次处理 $arg0';
  }

  @override
  String l6EventsArg045727(String arg0) {
    return '$arg0 条事件';
  }

  @override
  String get l6Updated9db0 => '更新';

  @override
  String get l6NoNewKnowledgeExtractedc38ab => '没有提炼出新的知识';

  @override
  String get l6DohSettingsSavedeaeea => 'DoH 设置已保存';

  @override
  String get l6EnterADohServiceUrlBeforeff97f => '请先填写 DoH 服务地址再测试';

  @override
  String get l6ActivationPolicyfc94d => '启用策略';

  @override
  String get l6DohServiceUrlae8b4 => 'DoH 服务地址';

  @override
  String get l6OffTheUrlIsSavedButa536d => '当前为「关闭」：地址仅保存不生效，外网搜索走系统直连。';

  @override
  String get l6RequiredADohFailureReturnsAnaabc1 =>
      '当前为「强制」：DoH 失败将直接报错，不会回退直连。';

  @override
  String get l6AutomaticPreferDohAndEchFallingfc42f =>
      '当前为「自动」：优先 DoH+ECH，连不上自动回退直连。';

  @override
  String get l6Testingcf7e9 => '测试中';

  @override
  String get l6RecordWhatIsHappeningUseName925ca =>
      '记录此刻发生的事情…（可用 @人名 标注联系人、#事情 标注项目）';

  @override
  String get l6ConversationFormat9d903 => '对话格式';

  @override
  String get l6HowToRecord2bcbc => '怎么记录';

  @override
  String get l6UseNameToIdentifyAContact6d759 =>
      '@人名：明确标注联系人\n#事情：明确标注项目、事项或主题\n\n例如：@张伟 正在负责 #付款流程\n\n记一下：明确保存一条经历或进展\n保存到知识库：把结论沉淀下来\n帮我建待办：创建后续行动\n\n联系人关系、知识页、待办和规则会先给你看草稿。请打开“审阅提议”，查看具体内容后接受或拒绝；普通聊天回复不会执行草稿。';

  @override
  String l6Arg0Revoked9671e(String arg0) {
    return '$arg0（已撤销）';
  }

  @override
  String l6DeviceArg0WillImmediatelyLoseSyncadbd3(String arg0) {
    return '设备 \"$arg0\" 将立即失去再同步资格。\n\n丢失设备须知：撤销同步权限不会回收该设备已持有的 Provider API Key。请到对应 Provider 控制台吊销或轮换密钥，剩余设备会接收新版本。';
  }

  @override
  String l6ChangesFromArg0710df(String arg0) {
    return '来自 $arg0 的改动';
  }

  @override
  String l6RenamedFromArg0e3178(String arg0) {
    return '由「$arg0」改名而来';
  }

  @override
  String l6ContentLengthArg0CharactersUpdatedArg1f8788(
    String arg0,
    String arg1,
  ) {
    return '正文 $arg0 字 · 更新 $arg1';
  }

  @override
  String l6ConditionsArg006edc(String arg0) {
    return '适用条件：$arg0';
  }

  @override
  String get l6WebSearchSettingsSaved30dee => '外网搜索设置已保存';

  @override
  String get l6SearchEnginee5837 => '搜索引擎';

  @override
  String get l6SearxngInstanceUrla3267 => 'SearxNG 实例地址';

  @override
  String get l6ApiUrlacd8a => 'API 地址';

  @override
  String get l6HttpLocalhost8080OrYourSearxngba754 =>
      'http://localhost:8080 或你的 SearxNG 地址';

  @override
  String get l6SearxngIsSelfHostedMetasearchNoe6c57 =>
      'SearxNG 是自托管元搜索，无需 API key。需在你的实例开启 JSON 输出格式。';

  @override
  String get l6EnableWebSearch9ee9a => '启用外网搜索';

  @override
  String get l6AllowAiToSearchTheWeb1ea6a => '开启后，AI 可在你询问最新/新闻/热点类问题时联网检索';

  @override
  String get l6TvlyFreeAllowanceAtTavilyCom279ec =>
      'tvly-...（tavily.com 免费注册即有额度）';

  @override
  String get l6BsaApplyAtBraveComSearch6c59c =>
      'BSA...（brave.com/search/api 申请）';

  @override
  String get l6ConfiguredEnterAValueToReplace1d9d7 => '已配置（输入新值覆盖，留空保持不变）';

  @override
  String get l6ProxySettingsSavedOutboundRequestsUseac39b => '代理设置已保存，出站请求即时生效';

  @override
  String get l6EnterAProxyUrlBeforeTesting89cfb => '请先填写代理地址再测试';

  @override
  String get l6UseHttpsProxyOrAllProxy33ed0 =>
      '跟随时下环境：HTTPS_PROXY / ALL_PROXY，未设置则直连';

  @override
  String get l6ConfigureAProxyea2d0 => '手动配置代理';

  @override
  String get l6UseTheUrlBelowInsteadOf96914 =>
      '以下方地址为准，覆盖环境变量；挂代理期间 DoH 隧道自动让位';

  @override
  String get l6NoProxy4f187 => '不使用代理';

  @override
  String get l6ConnectDirectlyIgnoringProxyEnvironmentVariablesa4dbf =>
      '强制直连，环境变量中的代理也被忽略';

  @override
  String get l6Http1270017890a9d59 => 'http://127.0.0.1:7890 或 socks5://…';

  @override
  String get l6SelectAnEventToViewItsd5e60 => '左侧选择事件查看详情';

  @override
  String get l6EventDoesNotExistf5542 => '事件不存在';

  @override
  String get l6EventRecordsbabe0 => '事件记录';

  @override
  String get l6RecordedAt650b3 => '记录时间';

  @override
  String get l6NoAnalysisYetPendingOrNo90172 => '尚无可用分析（等待处理或未配置分析器）。';

  @override
  String get l6AnalysisSummary83a0f => '分析摘要';

  @override
  String get l6Peopleb755b => '人物';

  @override
  String get l6NoRelatedRuleReminders24596 => '无相关规则提醒';

  @override
  String get l6CouldNotLoadEvent7ba1f => '事件加载失败';

  @override
  String get l6ConversationsWithAiAreRecordedAs6eb67 =>
      '和 AI 的对话会被记录为事件，用于回顾与知识沉淀。';

  @override
  String get l6TodaySRecordse0105 => '今天记录';

  @override
  String get l6Last7Days2a0a0 => '最近 7 天';

  @override
  String get l6NoEventsRecordedYetConversationsWithb4ac9 =>
      '还没有记录过事件\n和 AI 的对话会自动作为事件记录在这里';

  @override
  String get l6NoRecentlyRecordedEventsf0c62 => '最近没有记录的事件';

  @override
  String get l6TryAgainLaterb8e0d => '稍后重试';

  @override
  String l6Arg0CompletedArg1Items30653(String arg0, String arg1) {
    return '$arg0完成 · $arg1 项';
  }

  @override
  String l6AutomaticRetryArg06d99e(String arg0) {
    return '自动重试：$arg0';
  }

  @override
  String l6Arg0FromThisPageSSource92ed3(String arg0) {
    return '$arg0 · 来自本页原料';
  }

  @override
  String l6AcceptChangesArg0a09f4(String arg0) {
    return '采纳修改 $arg0';
  }

  @override
  String get l6RemoveThisParagraphe26cb => '删除这一段';

  @override
  String l6HistoricalSnapshotsArg0cbbd3(String arg0) {
    return '共有 $arg0 个历史快照';
  }

  @override
  String l6LatestArg0Arg1aa89a(String arg0, String arg1) {
    return '最新：$arg0 · $arg1';
  }

  @override
  String l6DeviceNameArg0a02e4(String arg0) {
    return '设备名：$arg0';
  }

  @override
  String l6RoleArg0f3803(String arg0) {
    return '角色：$arg0';
  }

  @override
  String get l6NoRelayConfiguredSetItWhenfc098 => '尚未配置中继（首次邀请成员时填写）';

  @override
  String get l6StartANewChapterHereEarliere67c8 => '翻篇到此——此前历史收进章节摘要';

  @override
  String get l6ThisChapterMarkerNoLongerExistsec974 => '翻篇标记已不存在，可能已被撤销';

  @override
  String get l6NewChapterSummarycf20d => '翻篇 · 章节摘要';

  @override
  String get l6GeneratingChapterSummary277ba => '正在生成章节摘要…';

  @override
  String get l6DiscardChanges1cead => '放弃修改';

  @override
  String get l6ConfirmNewChapter12069 => '确认翻篇';

  @override
  String get l6RemoveThisEntry52142 => '删除这一条';

  @override
  String l6AddArg03e9a8(String arg0) {
    return '加一条$arg0';
  }

  @override
  String get l6AfterStartingANewChapterThis0bfa3 =>
      '翻篇后这段历史不再原文进入请求，仅以本摘要常驻；需要精确数值或原话时 AI 会回读原始消息。可编辑或删减条目后确认。';

  @override
  String get l6NoChapterHistoryInThisConversation36eaa => '本会话还没有翻篇记录';

  @override
  String get l6Day15917 => '日';

  @override
  String get l6Week451b8 => '周';

  @override
  String get l6Monthd9b59 => '月';

  @override
  String get l6PreviousDaydede7 => '前一天';

  @override
  String get l6NextDaya4161 => '后一天';

  @override
  String get l6CouldNotLoadReview55475 => '回顾加载失败';

  @override
  String get l6AiReviewbb899 => 'AI 回顾';

  @override
  String get l6NoAiReviewForThisDay286ea => '当天还没有 AI 回顾';

  @override
  String get l6EventsForThisDayfbec1 => '当日事件';

  @override
  String get l6NoEventsRecordedOnThisDay6a37a => '当天没有记录事件';

  @override
  String get l6ThisDayLastWeekfc29d => '上周的今天';

  @override
  String l6EarlierReviewsArg0f4cc0(String arg0) {
    return '更早的盘点（$arg0）';
  }

  @override
  String l6PeriodReviewArg0ToArg1Arg269c4d(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '周期盘点 · $arg0 ~ $arg1$arg2';
  }

  @override
  String get l6Events550e3 => '事件';

  @override
  String get l6ToDosCompletede1bfa => '待办完成';

  @override
  String get l6DeviationChecksPending648aa => '偏差检测待进行';

  @override
  String get l6NoDeviationsToReport28ee2 => '未发现待说出的偏差';

  @override
  String get l6MomentsThatDivergedFromYourGoals39f82 => '偏离目标的时刻——只陈述，不评判：';

  @override
  String l6Arg0Dismissede66fc(String arg0) {
    return '$arg0 · 已忽略';
  }

  @override
  String l6GoalArg06fb2b(String arg0) {
    return '目标：$arg0';
  }

  @override
  String get l6CouldNotLoadStatistics88ae7 => '统计加载失败';

  @override
  String get l6NoRecordsThisWeeka75a5 => '本周没有记录';

  @override
  String get l6NoRecordsThisMonthd57d9 => '本月没有记录';

  @override
  String get l6EventTypes40b68 => '事类';

  @override
  String get l6Activitiesb2548 => '活动';

  @override
  String l6KnowledgeProcessingArg0ActivePagesArg179c73(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return '知识管道 · $arg0 页活跃$arg1$arg2 · 摘要 30d 内 $arg3 成$arg4';
  }

  @override
  String get l6KnowledgeProcessingHealthLast30Days3bf5c => '知识管道健康（近 30 天）';

  @override
  String get l6Compilation8669e => '编译';

  @override
  String l6PendingProposalsArg030DaysAcceptedf2e76(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return '待审建议 $arg0（30d 采纳 $arg1 / 拒绝 $arg2）· 后台任务失败 $arg3/$arg4';
  }

  @override
  String get l6Librarydfd6b => '库存';

  @override
  String l6ActiveArg0ArchivedArg1NoSummaryb4539(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return '活跃 $arg0 · 归档 $arg1 · 无摘要 $arg2 · 长期未更新 $arg3 · 从未投递 $arg4';
  }

  @override
  String get l6Delivery412a9 => '投递';

  @override
  String l6AttachedArg0TimesArg1PagesUsageabc07(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return '挂载 $arg0 次（$arg1 页）· 使用记录 $arg2 · 被引用 $arg3 页';
  }

  @override
  String get l6Retrieval1c037 => '检索';

  @override
  String l6AiSearchesArg0PageReadsArg15216f(String arg0, String arg1) {
    return 'AI 检索 $arg0 次 · 取页 $arg1 次';
  }

  @override
  String get l6SummaryPipeline00b38 => '摘要管线';

  @override
  String l6CompletedArg0UnavailableArg1Arg2ee56d(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '完成 $arg0 · 不可用 $arg1$arg2';
  }

  @override
  String get l6SourceReviewGate877bd => '孵化闸门';

  @override
  String l6SourcesAwaitingADecisionArg0Rejected9c019(String arg0, String arg1) {
    return '待定论 $arg0 页 · 负知识 $arg1 条';
  }

  @override
  String get l6SummaryPreparationHistoryProcessedInBackground2648e =>
      '摘要预热账本（背景 tick 多批推进，事故 2026-10-19 P1(c)）';

  @override
  String get l6NoPreparationHistoryYetBackgroundBatchescc092 =>
      '暂无预热记录（大树冷启动时由背景分批追平）。';

  @override
  String get l6LoadingHandoffStatus19264 => '正在读取移交状态…';

  @override
  String l6TransferAiAnalysisResponsibilityToArg05bc00(
    String arg0,
    String arg1,
  ) {
    return '将把 AI 分析职责移交给 \"$arg0\"（$arg1）。\n\n本机会立即停止认领后台任务；目标设备需同步到移交边界并显式确认后才开始 AI 分析。确认后本机不能单方面恢复。';
  }

  @override
  String get l6EnrollmentUsesASeparateStagingDatabase0ddf7 =>
      '使用独立暂存库，原记录和 Provider 不会覆盖。明确批准并安装后，自动协调正式通道、成员名册及初始快照；当前数据集不会切换。';

  @override
  String get l6UsingKey99422 => '通过 Key';

  @override
  String l6InvitingDeviceArg0d440c(String arg0) {
    return '邀请设备：$arg0';
  }

  @override
  String l6DatasetArg085f96(String arg0) {
    return '数据集：$arg0';
  }

  @override
  String l6InvitationExpiresArg0abd8a(String arg0) {
    return '邀请到期：$arg0';
  }

  @override
  String l6DatasetArg0OnlyApprovedCheckpointsCand6435(String arg0) {
    return '数据集：$arg0\n仅恢复已审批检查点；未审批状态重启后需新邀请。';
  }

  @override
  String l6StagedDataAndDeviceKeysFor0f456(String arg0) {
    return '「$arg0」的暂存数据和本机密钥会被删除，且不可恢复。如果还想加入这个数据集，之后需要重新扫码并经对方批准。';
  }

  @override
  String l6ThisDeviceArg0Arg1ab420(String arg0, String arg1) {
    return '本机：$arg0 · $arg1';
  }

  @override
  String get l6ThisDeviceSSyncIdentityIs46884 =>
      '本机的同步身份不完整（常见于配对过程中断、掉电或恢复备份）。可以清除本机同步身份后重新配对 —— 笔记、对话、知识库都会保留。';

  @override
  String l6SyncOperationsAwaitingConfirmationArg0224a8(String arg0) {
    return '待确认同步操作：$arg0';
  }

  @override
  String l6DeviceArg0d7706(String arg0) {
    return '设备：$arg0';
  }

  @override
  String l6PlatformArg0e4ee7(String arg0) {
    return '平台：$arg0';
  }

  @override
  String l6ExpiresArg0a7daf(String arg0) {
    return '到期时间：$arg0';
  }

  @override
  String l6AwaitingReviewArg0Arg17fa78(String arg0, String arg1) {
    return '待审阅：$arg0（$arg1）';
  }

  @override
  String l6RevisionFromArg0beef3(String arg0) {
    return '来自 $arg0 的修订';
  }

  @override
  String l6AddressHintArg03f1aa(String arg0) {
    return '地址提示：$arg0';
  }

  @override
  String l6LastSyncUploadedArg0ReceivedArg120f78(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
    String arg5,
  ) {
    return '最近同步：上行 $arg0 · 到达 $arg1$arg2$arg3$arg4$arg5';
  }

  @override
  String get l6Sendingd948e => '发送中';

  @override
  String l6RetryingLaterArg0baf89(String arg0) {
    return '稍后重试：$arg0';
  }

  @override
  String l6UnknownStateArg0c8bbc(String arg0) {
    return '未知状态：$arg0';
  }

  @override
  String l6RetryInArg0Secondsccd79(String arg0) {
    return '$arg0 秒后重试';
  }

  @override
  String l6RetryInArg0Minutesef5e1(String arg0) {
    return '$arg0 分钟后重试';
  }

  @override
  String l6DeliveryArg0QueuedArg1AwaitingConfirmationa5864(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return '投递 $arg0（待发 $arg1 · 待确认 $arg2）：$arg3';
  }

  @override
  String l6PeerHasNotConfirmedArg0Itemsc4ccb(String arg0, String arg1) {
    return '对端未确认 $arg0 条 · $arg1';
  }

  @override
  String get l6EnterTheTargetDeviceIdb0da4 => '请填写目标设备 ID。';

  @override
  String get l6ThisDeviceIsNotTheProcessor82939 => '本机当前不是加工设备，无需让出。';

  @override
  String get l6OnlyTheDatasetOwnerCanApprovec5d36 => '只有 owner 设备才能审批移交证明。';

  @override
  String get l6WaitForThisDeviceSActive1e382 =>
      '本机仍有进行中的后台任务，必须等它们完成后才能让出 AI 分析职责。';

  @override
  String get l6WaitForSyncToReachThe58a6b => '本机尚未同步到移交边界，请先等待同步追平再确认接管。';

  @override
  String get l6NoProcessorHandoffAwaitsThisDevice12c2e => '本机没有待确认的加工接管。';

  @override
  String get l6TheHandoffProofSignatureIsInvalid82d09 => '移交证明签名无效，可能已被篡改。';

  @override
  String get l6HandoffProofInformationIsOutsideIts3c0a0 => '移交证明信息超出有效范围。';

  @override
  String get l6TheProofDoesNotMatchThe90664 => '移交证明版本与当前名册不一致，请重新在原加工设备上让出。';

  @override
  String get l6TheProofDoesNotMatchThe7ec43 => '移交证明与当前加工设备不符，请重新在原加工设备上让出。';

  @override
  String get l6TheTargetIsNotAnActivea00bf => '移交目标不是当前名册中的活跃设备。';

  @override
  String get l6TheOriginalProcessorIsNoLongerd44b7 => '原加工设备已不在名册中，无法审批此证明。';

  @override
  String get l6TheHandoffProofBelongsToA7b34b => '移交证明与当前数据集不符。';

  @override
  String get l6AiAnalysisCannotBeTransferredTod5ffa => 'AI 分析职责不能移交给本机自己。';

  @override
  String get l6TheHandoffProofDoesNotMatch31e8f => '移交证明与本机状态不符。';

  @override
  String get l6HandoffProofIsMissingTheDevice668d5 => '缺少移交证明，无法应用此设备变更。';

  @override
  String get l6TheProofSSourceDoesNot02b44 => '移交证明来源与本机状态不符。';

  @override
  String get l6TheHandoffProofHasNoSource9502a => '移交证明缺少来源设备。';

  @override
  String get l6TheHandoffProofTargetsADifferent2a815 => '移交证明目标与本机不符。';

  @override
  String get l6ProcessorHandoffProofIsMissingThis0e9df =>
      '缺少加工移交证明，本设备无法采纳此变更。';

  @override
  String get l6TheRosterIncludesHandoffProofWithout363f4 => '名册未变更加工设备却携带移交证明。';

  @override
  String get l6UnlockThisDeviceSSyncKeyse19a6 => '本机同步密钥已锁定，请先解锁后再操作。';

  @override
  String get l6ThisDeviceSSyncIdentityIs5c640 => '本机尚未准备同步身份。';

  @override
  String get l6TransferThisDeviceSAiAnalysis9e01d =>
      '该设备当前负责 AI 分析，必须先移交职责才能撤销。';

  @override
  String get l6AnotherDeviceUpdatedTheRosterRefreshb7381 =>
      '名册刚被其他设备更新，请刷新后重新审阅撤销。';

  @override
  String get l6OnlyTheDatasetOwnerCanRevoke2ce42 => '只有 owner 设备才能撤销成员。';

  @override
  String get l6ThisDeviceSAccessHasAlreadyb76c9 => '该设备已被撤销。';

  @override
  String get l6ThisDeviceIsNotInThe3f9c7 => '该设备不在名册中。';

  @override
  String get l6InvalidRosterRevisione21c2 => '名册修订号无效。';

  @override
  String get l6TheRosterHasReachedItsMemberee48e => '名册成员数已达上限。';

  @override
  String get l6DeviceIdentitiesConflictInTheRosterea7bc => '名册设备身份冲突（重复密钥或流）。';

  @override
  String get l6TheLocalPageChangedRefreshAnd4e980 => '本端页面已变化，请刷新后重新审阅。';

  @override
  String get l6ChannelAuthorizationOrRevisionChangedAutomatic779c0 =>
      '正式通道的授权或修订已改变，已停止自动交接。请检查原设备与中继授权；不会读取新修订后覆盖权限。';

  @override
  String get l6TheHandoffRevisionOrIdentityCould284a1 =>
      '交接的系统修订或身份未能确认，已暂停。请解锁本机或恢复原暂存项；不会重置密钥。';

  @override
  String get l6TheRevisionAndReleaseOfThebd873 =>
      '系统修订未确认，且无法确认本机会话已释放。请保留数据并关闭、重开应用后恢复。';

  @override
  String get l6TheHandoffStepIsIncompleteThe70ae7 =>
      '交接步骤暂未完成，稍后按原请求重试；不会重新审批或更换密钥。';

  @override
  String l6Arg0Secondse612a(String arg0) {
    return '$arg0 秒';
  }

  @override
  String l6Arg0Milliseconds5f522(String arg0) {
    return '$arg0 毫秒';
  }

  @override
  String l6TheSyncStepDidNotRespond28e88(String arg0) {
    return '同步步骤超过 $arg0没有响应，已重新排队重试；数据没有丢失。';
  }

  @override
  String get l6TheApprovedHandoffIsBeingCoordinated7e9e2 =>
      '已批准的交接正在自动协调；初始快照、回执和当前数据集切换是不同阶段。';

  @override
  String get l6TheNativeHandoffRevisionCouldNot775d0 =>
      '交接的原生修订未能确认，已停止本机会话。请解锁恢复原暂存项，不要重新加入。';

  @override
  String get l6PreviewDidNotConnectToThed4c19 => '预览未连接中继。请核对邀请来源、地址和设备指纹。';

  @override
  String get l6EnterADeviceNameAndExplicitly802c8 =>
      '填写设备名称，并明确同意连接显示的中继、接收 Provider 配置和 API Key。';

  @override
  String get l6JoinRequestDeliveredVerifyThisDevice8e98f =>
      '加入请求已投递。请在已有设备核对本机指纹并明确批准。';

  @override
  String get l6NoUnfinishedEnrollmentStagingEntriesaa853 => '没有未完成的加入暂存项。';

  @override
  String get l6RecoveryReadsTheOriginalInstallationS770c9 =>
      '恢复只读取原安装的密钥，不会创建替代密钥。';

  @override
  String get l6StagingEntryDeletedButKeyCleanup3ce20 =>
      '暂存项已删除，但本机密钥清理失败，可稍后在密钥健康里检查。';

  @override
  String l6StagingEntryArg0Deleted0f067(String arg0) {
    return '已删除暂存项「$arg0」。';
  }

  @override
  String get l6OriginalInstallationProofResentWaitingForee8e7 =>
      '已重发原安装证明，仍等待成员授权和初始同步；没有重新批准或重新生成密钥。';

  @override
  String get l6OriginalRequestResentWaitingForExplicitb70c0 =>
      '已按原请求重发，等待已有设备明确批准。';

  @override
  String get l6InstallationProofDeliveredWaitingForAuthorization69cc4 =>
      '安装证明已投递；仍等待成员授权和初始同步，当前数据集没有切换。';

  @override
  String get l6NoUsableApprovalYetVerifyAnd0b6c4 => '尚未收到可用审批，请在已有设备核对并批准。';

  @override
  String get l6TheSameInstallationProofWasResent1abc4 =>
      '已重发同一安装证明；投递不表示设备已加入或同步完成。';

  @override
  String get l6EnrollmentStoppedStagedDataAndKeys9b648 =>
      '已停止本机加入流程，暂存库和密钥保留。临时投递权限会到期；本操作不撤销已发出的请求或可能已获批准的成员资格。';

  @override
  String get l6DeviceKeysOrProtectedRevisionsAre5de96 =>
      '缺少本机密钥或受保护修订，不能恢复；不会生成替代密钥。';

  @override
  String get l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35 =>
      '受保护修订发生回退，已停止加入；原库和暂存数据保留。';

  @override
  String get l6DeviceKeyRecordsAreDamagedEnrollment76c86 =>
      '本机密钥记录损坏，已停止加入；不会覆盖已有密钥。';

  @override
  String get l6SecureKeyStorageCouldNotSave6f761 =>
      '系统密钥库未能安全保存或回读，已停止加入；解锁系统密钥库后恢复暂存项。';

  @override
  String get l6ThisWorkspaceHasAnActiveSync69142 =>
      '工作区已有同步会话或后台任务，请先结束任务或锁定当前同步，再发起加入。';

  @override
  String get l6TheInvitationExpiredOrHasNo6c654 =>
      '本次邀请已过期或没有可恢复的已审批检查点。请停止本次流程并获取新邀请；已有数据保留。';

  @override
  String get l6TheRelayStepIsUnconfirmedCheck92aea =>
      '中继步骤未确认。可检查审批或重发原请求/已保存的证明，不要重新生成密钥。';

  @override
  String get l6EnrollmentIsIncompleteAndHasStopped1c448 =>
      '加入步骤未完成，已停止继续推进。请检查原邀请、解锁状态和暂存项；当前数据集未切换。';

  @override
  String get l6ConfirmThatTheJoiningDeviceMay8297c =>
      '请确认加入设备将获得全部 Provider 配置和 API Key。';

  @override
  String get l6TheInvitationIsValidFor245be17 =>
      '邀请码 24 小时内有效。新设备扫码发起加入后，这里会自动收到请求，核对设备与指纹后批准即可。';

  @override
  String get l6NoJoinRequestReceivedYet10542 => '尚未收到加入请求。';

  @override
  String get l6VerifyTheDeviceAndFingerprintBefore5e3e6 => '核对设备及指纹后再明确批准。';

  @override
  String get l6ExplicitlyConfirmSharingProviderApiKeysd0cdd =>
      '请明确确认向这台设备共享 Provider API Key。';

  @override
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8 =>
      '审批已投递，等待新设备安装确认；这不表示设备已加入或同步完成。';

  @override
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation9b218 =>
      '审批已投递，等待新设备安装确认。';

  @override
  String get l6InvitationCanceled35e1b => '本次邀请已取消。';

  @override
  String get l6ApprovalDeliveredWaitingForTheNew361e9 => '审批已投递，等待新设备完成安装确认。';

  @override
  String get l6TheInvitationOrReviewVersionExpireda53ec =>
      '邀请或审阅版本已过期，请重新发起邀请并核对设备。';

  @override
  String get l6TheInvitationWasCanceledLocallyRelay360d8 =>
      '本机已取消邀请，但中继撤销尚未确认；临时发送权限会随短期邀请到期。';

  @override
  String get l6ApprovalIsSavedButHasNot99508 =>
      '批准已保存，但暂时没送到新设备（网络或中继不可用）。系统会自动重试，无需重新批准。';

  @override
  String get l6BackgroundSyncIsUsingTheRuntime0a21c =>
      '后台同步正在占用本机同步运行时，本次配对写入未执行；请稍候再点一次。没有自动授权新设备。';

  @override
  String get l6PairingIsIncompleteCheckDeviceUnlock85108 =>
      '配对步骤未完成，请检查本机解锁状态和中继地址；没有自动授权新设备。';

  @override
  String get l6UnlockThisDeviceSSyncKeysfc734 => '本机同步密钥已锁定：请先在上方解锁后审阅冲突。';

  @override
  String get l6LocalSettingsChangedReviewTheUpdated3cd59 =>
      '本地配置已变化，请重新审阅下方最新列表。';

  @override
  String get l6TheConflictChangedReviewTheUpdated936b5 =>
      '冲突内容已变化，请重新审阅下方最新列表。';

  @override
  String get l6CouldNotReadDeviceSyncState278eb => '无法读取本机同步状态；原有记录未改动。';

  @override
  String get l6DeviceSyncIsNotReady3fc30 => '本机同步尚未就绪。';

  @override
  String get l6DeviceNamesMustContain12561d9b2 => '设备名需为 1–256 个字符。';

  @override
  String get l6AnotherOperationUpdatedTheStateRefresh39e75 =>
      '状态刚被其他操作更新，请刷新后重试。';

  @override
  String get l6UnlockDeviceKeysBeforeRenaming10735 => '本机密钥已锁定，请先解锁再改名。';

  @override
  String get l6TheDeviceNameContainsInvalidCharacters438ed => '设备名包含无效字符，请换一个。';

  @override
  String get l6EnterADeviceNameAndConfirm02f54 =>
      '填写设备名称，并确认配对将共享 Provider API Key。';

  @override
  String get l6StopEnrollmentBeforePreparingOrUnlocking4420c =>
      '请先停止本机加入流程，再准备或解锁当前数据集。';

  @override
  String get l6LockingIsUnconfirmedTryAgainKeys6bf44 => '锁定操作未确认，请重试；不会重新生成密钥。';

  @override
  String get l6DeviceKeysOrProtectedRevisionsAre845e9 =>
      '缺少本机密钥或受保护的修订记录。请通过原设备恢复；不会生成替代密钥。';

  @override
  String get l6KeyRevisionsRolledBackUnlockingStoppedc58f3 =>
      '密钥修订发生回退，已停止解锁。请检查恢复的数据，原记录保留。';

  @override
  String get l6SystemKeyRecordsAreDamagedUnlocking23c95 =>
      '系统密钥记录损坏，已停止解锁。原数据和密钥不会被覆盖。';

  @override
  String get l6TheSystemKeyStoreIsUnavailable8d23d =>
      '系统密钥库不可用或已锁定。解锁系统密钥库后重试，仍可保存本地记录。';

  @override
  String get l6AnotherWindowOrBackgroundTaskIs610c2 =>
      '另一个窗口或后台任务正在使用此工作区，请等待结束后重试。';

  @override
  String get l6BackgroundJobsAreStillRunningPrepare56569 =>
      '后台任务仍在执行，完成后再准备同步；本次未切换加工归属。';

  @override
  String get l6TheDeviceSyncIdentityNeedsRecoveryd2a55 =>
      '本机同步身份需要恢复，已停止自动初始化。请保留原库并通过授权设备重新加入。';

  @override
  String get l6SyncPreparationDidNotFinishAnd1c512 =>
      '本机同步准备未完成，已保持锁定。请重试；原记录保留。';

  @override
  String get l6AndMore93ac7 => ' 等';

  @override
  String get l6SupportedServiceFxtwitterConvertsXCom0989a =>
      '当前支持的抓取服务：fxTwitter —— 把 x.com 推文链接自动解析为长文，\n接口 https://api.fxtwitter.com/status/<推文id>';

  @override
  String l6AndArg0MoreTools52a1a(String arg0) {
    return ' 等 $arg0 个工具';
  }

  @override
  String l6Arg0YouRecordedArg1Arg2Ac3209(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '$arg0你记录过：「$arg1」$arg2——还记得这件事。';
  }

  @override
  String l6SupplyYourOwnEndpointNoneIsd855e(String arg0) {
    return '仅自定义、无内置默认。填了地址后，外网搜索会经 DoH+ECH 传输，在国内网络下也能到达被 DNS 污染 / SNI 封锁的服务（如 x.com）。当前仅作用于外网搜索。$arg0';
  }

  @override
  String get l6Check40ffb => '检查';

  @override
  String get l6LoadingDevices5c6b5 => '正在读取设备列表…';

  @override
  String l6Arg0ProposalArg1PagesArg2Pages2ee4f(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '$arg0方案 · $arg1 页 → $arg2 页';
  }

  @override
  String l6SourcesArg0EventsArg10f288(String arg0, String arg1) {
    return '$arg0 份原料 · $arg1 个事件';
  }

  @override
  String l6SwitchedToArg0QuitAndReopenc2285(String arg0) {
    return '已切换到「$arg0」。请完全退出并重新打开应用，即可看到同步的数据。';
  }

  @override
  String l6JoinedDatasetArg0ItsDataIs7a081(String arg0) {
    return '已加入数据集「$arg0」，其数据尚未在此窗口生效。';
  }

  @override
  String l6DatasetsReadyToOpenArg0c5aa4(String arg0) {
    return '发现 $arg0 个待进入的数据集';
  }

  @override
  String l6DevicesOnlineArg0Arg1c5eda(String arg0, String arg1) {
    return '设备 $arg0/$arg1 在线';
  }

  @override
  String l6HeartbeatHealthyPendingArg05c133(String arg0) {
    return '最近心跳正常 · 待发 $arg0';
  }

  @override
  String get l6FollowUpSuggestions99ccc => '跟进建议';

  @override
  String get l6Guidance79851 => '指导';

  @override
  String l6RelatedRuleRemindersArg05615f(String arg0) {
    return '相关规则提醒（$arg0）';
  }

  @override
  String get l6AwaitingAnalysis605dc => '待分析';

  @override
  String get l6RecordedAwaitingAnalysis8e5e7 => '已记录，待分析';

  @override
  String l6CurrentRevisionArg0beb34(String arg0) {
    return '当前修订：$arg0';
  }

  @override
  String get l6ResolvedWithALinkedPagef44a7 => '已关联解决';

  @override
  String get l6Dismissedbd172 => '已忽略';

  @override
  String get l6OriginalSuggestiondb86c => '原始建议';

  @override
  String get l6OpenLinkedKnowledgePage88f20 => '打开对应知识页';

  @override
  String get l6LoadProcessingQueue079ca => '读取整理队列';

  @override
  String l6TotalArg0PendingArg1RunningArg2a5909(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
    String arg5,
    String arg6,
  ) {
    return '共 $arg0 项任务 · 待处理 $arg1 · 处理中 $arg2 · 等待 $arg3 · 重试 $arg4 · 跳过 $arg5 · 完成 $arg6';
  }

  @override
  String get l6RefreshListbd82e => '刷新列表';

  @override
  String get l6FullDocumentReadingc1da8 => '全文阅读';

  @override
  String get l6SourceProcessingfbebb => '原料整理';

  @override
  String get l6CrossSourceProcessing5240c => '跨资料整理';

  @override
  String get l6KnowledgeReview4f5bb => '知识复核';

  @override
  String get l6SourceRefresh65f83 => '来源更新';

  @override
  String l6ScheduledArg020b73(String arg0) {
    return '\n计划时间：$arg0';
  }

  @override
  String l6LastProcessedArg0NewEvidenceArg124fb9(String arg0, String arg1) {
    return '最近整理 $arg0 · 新增证据 $arg1 条';
  }

  @override
  String get l6RecentProcessingAndInsights23cc8 => '最近自动整理与洞察记录';

  @override
  String get l6LoadInsights25b9e => '读取洞察记录';

  @override
  String get l6CheckedAutomaticallyInTheBackgroundAs34c09 =>
      '积累事件后由后台自动检查，无需手动触发。';

  @override
  String get l6SourceUpdateCheckc6668 => '来源更新检查';

  @override
  String get l6TopicProcessingAndChecks872c6 => '主题整理与检查';

  @override
  String get l6LoadSourceEventsaaf09 => '读取来源事件';

  @override
  String get l6LoadKnowledgeOutputs54f00 => '读取知识产出';

  @override
  String get l6CrossSourceTopicIncludesThisPageae2b4 => '跨资料主题 · 包含本页及相关原料';

  @override
  String get l6AwaitingReview28cc1 => '待审阅';

  @override
  String get l6ConfirmAndSave706df => '确认保存';

  @override
  String get l6ReferenceKnowledgeSaved962eb => '已保存参考知识';

  @override
  String get l6RecordAnOpinionb7461 => '记录观点';

  @override
  String get l6LoadSourcesAndRevisionsd1d70 => '读取来源与修订';

  @override
  String get l6UnnamedDevice008d5 => '未命名设备';

  @override
  String get l6Memberc1ee9 => '成员';

  @override
  String get l6RuleReminders79233 => '规则提醒';

  @override
  String get l6Latest9d501 => '（最近一份）';

  @override
  String l6IncubatingArg07051b(String arg0) {
    return ' · $arg0 孵化中';
  }

  @override
  String l6RejectedArg02115d(String arg0) {
    return ' · $arg0 已否定';
  }

  @override
  String l6Arg0Unavailable85bdd(String arg0) {
    return ' $arg0 不可用';
  }

  @override
  String get l6UnlockToVerifycd78c => '解锁后核验';

  @override
  String l6WaitingForDependenciesArg08ff9b(String arg0) {
    return ' · 等待依赖 $arg0';
  }

  @override
  String get l6RosterResentdba01 => ' · 名册已补发';

  @override
  String l6VersionMismatchesArg02d8cc(String arg0) {
    return ' · 版本不匹配 $arg0';
  }

  @override
  String get l6ReceiptFailedf7d93 => ' · 回执失败';

  @override
  String get l6OffDirectConnection371c7 => '关闭（强制直连）';

  @override
  String get l6AutomaticPreferDohFallBackTo02ddb => '自动（优先 DoH，失败回退直连）';

  @override
  String get l6RequiredDohOnly763c2 => '强制（只用 DoH，不降级）';

  @override
  String get l6HostedSearchOptimizedForAiWith46b67 => '托管服务，面向 AI 检索优化，免费额度';

  @override
  String get l6HostedSearchWithAnIndependentPrivacy51ea0 => '托管服务，隐私导向的独立索引';

  @override
  String get l6SelfHostedMetasearchNoApiKey0926f => '自托管元搜索，无需 API key';

  @override
  String get l6Factsb27b7 => '事实';

  @override
  String get l6Decisions13750 => '决定';

  @override
  String get l6Unfinishedb61b0 => '未完成';

  @override
  String get l6IdeasAndDecisionsa2d17 => '想法与决定';

  @override
  String get l6PeopleAndProjectse73dc => '人与事';

  @override
  String get l6FollowUps068c6 => '后续';

  @override
  String get l6Helpful2f55f => '有用';

  @override
  String get l6NotRelevanta8361 => '不相关';

  @override
  String get l6ContentContainingProtocolMarkersIsHidden50ad8 =>
      '含协议标记的内容未在进度中展开，请查看原始记录。';

  @override
  String get l6Excerptfba71 => '…（片段）';

  @override
  String get l6ThisStepFailedCheckTheFinal1ac2e =>
      '此步骤未成功；请查看最终状态和审阅回执，不会自动重复执行。';

  @override
  String get l6NoCompletionReceiptCouldBeVerifiede00f1 =>
      '没有可核实的完成回执；请检查实际结果，不会自动重做。';

  @override
  String get l6SeeTheReviewCardBelowFord5a60 => '具体内容、目标版本和接受/拒绝操作见下方审阅卡。';

  @override
  String l6EvidenceAtRetrievalArg0cb747(String arg0) {
    return '检索时的证据片段 · $arg0';
  }

  @override
  String l6SourceArg0VersionArg1SnapshotArg2d0032(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '来源：$arg0 · 版本 $arg1 · 快照 $arg2';
  }

  @override
  String l6PageHashAtRetrievalArg05808a(String arg0) {
    return '检索时页面依据：$arg0';
  }

  @override
  String l6UserRecordArg0Arg1902d4(String arg0, String arg1) {
    return '用户记录 $arg0：$arg1';
  }

  @override
  String l6UnverifiedPageArg01a6ef(String arg0) {
    return '未核验页面 · $arg0';
  }

  @override
  String get l6NoEvidenceExcerptsWereReturned013a1 => '没有返回可展开的证据片段。';

  @override
  String get l6TheseAreTheResultsReturnedBy46276 =>
      '以上是本次查询返回的数据；来源可能随后变化，最终回答引用另行核验。';

  @override
  String get l6GeneratingAReplyItContinuesWhence0b9 => '正在生成回复；切走也能继续，完成后自动停止。';

  @override
  String get l6CouldNotLoadGenerationStatusReconnect2eabb =>
      '无法读取生成状态，请重新连接；不会自动重新生成。';

  @override
  String get l6TheStartResultIsUnconfirmedReconnecting3cc5d =>
      '启动结果待核实。重新连接会查询同一次请求，不会重复保存输入。';

  @override
  String get l6TheStopRequestIsUnconfirmedReconnectdaf8d =>
      '停止请求尚未确认，请重新连接查看状态。';

  @override
  String get l6GenerationStatusIsTemporarilyUnavailableSaved71c5d =>
      '生成状态暂不可用；已保存的输入不会重发。';

  @override
  String get biometricReason => '验证指纹或面容以解锁 Elsewhen';

  @override
  String get l6Code33246 => '代码';

  @override
  String get l6CodeCopied8df90 => '代码已复制';

  @override
  String l6HeadingArg01576c(String arg0) {
    return '标题 $arg0';
  }

  @override
  String get l6Heading186e3d => '一级标题';

  @override
  String get l6Heading222dfe => '二级标题';

  @override
  String get l6Heading3516f9 => '三级标题';

  @override
  String get l6Boldf670b => '加粗';

  @override
  String get l6Italicaf5a2 => '斜体';

  @override
  String get l6Underline9bc18 => '下划线';

  @override
  String get l6Strikethroughc85de => '删除线';

  @override
  String get l6InlineCode1a813 => '行内代码';

  @override
  String get l6BulletedListb9e9a => '无序列表';

  @override
  String get l6NumberedList0deb8 => '有序列表';

  @override
  String get l6Quote23c0e => '引用';

  @override
  String get l6Mergee5bad => '合并';

  @override
  String get l6Split726ca => '拆分';

  @override
  String get l6Reprocess796dd => '重整理';

  @override
  String get l6WhyIsThisExcerptUnreliableOra83f2 =>
      '这条摘录为什么不可信/不可用？原因会成为负知识一部分，下次同源导入会提前提示。';

  @override
  String get l6Discardea489 => '丢弃';

  @override
  String get l6Processing1a5c2 => '正在整理…';

  @override
  String get l6CheckForUpdatesa6df3 => '检查更新';

  @override
  String get l6ReprocessThisPaged752f => '重新整理此页';

  @override
  String get l6Unknownd9c32 => '未知';

  @override
  String get l6RetryProcessingf8833 => '重试整理';

  @override
  String get deviationNoProgress => '无进展';

  @override
  String get deviationStalled => '停滞';

  @override
  String get deviationConflict => '相悖';

  @override
  String get deviationScopeDrift => '目标偏移';

  @override
  String get initialSnapshotTransfer => '初始快照传输';

  @override
  String get legacyPeopleProjects => '联系人/项目';

  @override
  String get legacyArtifacts => '派生产物';

  @override
  String get legacyPlanPending => '待定';

  @override
  String get legacyPlanActive => '推进中';

  @override
  String get legacyNewConversation => '新对话';

  @override
  String get reprocessingResult => '已重整：请查看处理结果';

  @override
  String get processingDetails => '处理说明';

  @override
  String get errorKnowledgeSourceChanged => '来源已更新或被拒绝，请重新生成建议';

  @override
  String get connectionSucceeded => '连接成功';

  @override
  String syncLockedBacklog(int count, String size) {
    return '待解锁积压 $count 条 · $size，解开本机密钥后自动同步';
  }

  @override
  String syncAwaitingPeer(int count, String size) {
    return '同步延迟：对端未确认 $count 条 · $size，正在重试';
  }

  @override
  String get syncGoUnlock => '去解锁';

  @override
  String legacyOperationFailed(String action) {
    return '$action失败，请重试一次';
  }

  @override
  String pageReprocessed(int count) {
    return '已重整：正文按全部 $count 条来源事件增量修补';
  }

  @override
  String get pageHumanProtected => '页面有人工编辑保护，正文不动；如需机器改写请走「提炼/建议」审阅通道';

  @override
  String get processingNotes => '处理说明';

  @override
  String get queueDetailPending => '等待后台处理';

  @override
  String get queueDetailRunning => '正在处理';

  @override
  String get queueDetailWaiting => '等待来源阅读或上游审阅完成';

  @override
  String get queueDetailRetry => '处理失败，等待自动重试';

  @override
  String get queueDetailSkipped => '已跳过；可查看原因';

  @override
  String get queueDetailCompleted => '处理完成';

  @override
  String get aiReplyFailedGeneric => 'AI 回复失败，请重试或查看错误详情。';

  @override
  String get manualLabel => '手动';

  @override
  String get automaticLabel => '自动';

  @override
  String get activeProviderSuffix => '（当前激活）';

  @override
  String nodesAdvanced(int count) {
    return '推进 $count 节点';
  }

  @override
  String get errorKnowledgeReviewChanged => '页面、来源或规则设置已变化，请重新审阅。';
}
