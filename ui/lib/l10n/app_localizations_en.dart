// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'Elsewhen';

  @override
  String get initializationFailed => 'Initialization failed';

  @override
  String initializationError(String error) {
    return 'Initialization failed: $error';
  }

  @override
  String get startingApp => 'Starting…';

  @override
  String get language => 'Language';

  @override
  String get followSystem => 'Follow system';

  @override
  String get chinese => '简体中文';

  @override
  String get english => 'English';

  @override
  String get save => 'Save';

  @override
  String get saving => 'Saving…';

  @override
  String get cancel => 'Cancel';

  @override
  String get delete => 'Delete';

  @override
  String get edit => 'Edit';

  @override
  String get add => 'Add';

  @override
  String get close => 'Close';

  @override
  String get confirm => 'Confirm';

  @override
  String get retry => 'Retry';

  @override
  String get settings => 'Settings';

  @override
  String get backToMain => 'Back to main screen';

  @override
  String get conversation => 'Chat';

  @override
  String get knowledge => 'Knowledge';

  @override
  String get goals => 'Goals';

  @override
  String get review => 'Review';

  @override
  String get sync => 'Sync';

  @override
  String get today => 'Today';

  @override
  String get recentlyUsed => 'Recent pages';

  @override
  String get noRecentPages => 'No recent pages';

  @override
  String get conversationLoadFailed => 'Could not load conversations';

  @override
  String get lastSevenDays => 'Last 7 days';

  @override
  String get rejectedSources => 'Rejected sources';

  @override
  String get noRejectedSources => 'No rejected sources yet.';

  @override
  String get models => 'Models';

  @override
  String get appearance => 'Appearance';

  @override
  String get integrations => 'Integrations';

  @override
  String get network => 'Network';

  @override
  String get skills => 'Skills';

  @override
  String get habits => 'Habits';

  @override
  String get data => 'Data';

  @override
  String get rules => 'Rules';

  @override
  String get security => 'Security';

  @override
  String get aiProviders => 'AI providers';

  @override
  String get memoryStrategy => 'Conversation memory';

  @override
  String get prompts => 'Prompts';

  @override
  String get memorySaved => 'Memory settings saved';

  @override
  String get strategyType => 'Strategy';

  @override
  String get simpleMemory => 'Recent messages (last N)';

  @override
  String get slidingWindowMemory => 'Sliding window (token limit)';

  @override
  String get maximumMessages => 'Maximum messages';

  @override
  String get memoryBudget => 'Conversation memory budget (optional)';

  @override
  String get memoryBudgetHint =>
      'Leave blank to derive from the provider context window';

  @override
  String get themeMode => 'Theme mode';

  @override
  String get themeColors => 'Color palette';

  @override
  String get lightTheme => 'Light';

  @override
  String get darkTheme => 'Dark';

  @override
  String get amberTheme => 'Amber';

  @override
  String get indigoTheme => 'Indigo';

  @override
  String get aquaTheme => 'Aqua';

  @override
  String get violetTheme => 'Violet';

  @override
  String get bodyFontSize => 'Content font size';

  @override
  String get contentFont => 'Content font';

  @override
  String get contentFontDescription =>
      'Applies to knowledge pages and messages. Interface text uses a fixed font.';

  @override
  String get systemDefault => 'System default';

  @override
  String get desktopWindow => 'Desktop window';

  @override
  String get noTrayAvailable =>
      'No system tray is available. Closing the window quits the application.';

  @override
  String get onWindowClose => 'When closing the window';

  @override
  String get minimizeToTray => 'Minimize to tray';

  @override
  String get quitApp => 'Quit application';

  @override
  String get stayInTrayDescription =>
      'Keep running in the background. Reopen from the tray menu.';

  @override
  String get quitDescription =>
      'Quit the application and remove its tray icon.';

  @override
  String get pending => 'Pending';

  @override
  String get processing => 'Processing';

  @override
  String get waitingRetry => 'Waiting to retry';

  @override
  String get completed => 'Completed';

  @override
  String get failed => 'Failed';

  @override
  String get skipped => 'Skipped';

  @override
  String get lastSuccess => 'Last success';

  @override
  String get analysisStatusFailed => 'Could not load analysis queue status';

  @override
  String get digestStatusFailed => 'Could not load knowledge processing status';

  @override
  String get digestDescription =>
      'Recorded events are organized into the knowledge base automatically. Failed jobs retry after a cooldown.';

  @override
  String get viewQueueLogs => 'View queue and run logs';

  @override
  String get exporting => 'Exporting…';

  @override
  String get chooseExportFolder => 'Choose folder and export';

  @override
  String get exportFolder => 'Choose export folder';

  @override
  String get storageExportUnsupported =>
      'The current storage backend does not support export';

  @override
  String get loading => 'Loading…';

  @override
  String get compacting => 'Compacting…';

  @override
  String get compactNow => 'Compact now';

  @override
  String get databaseAlreadyCompact =>
      'Compaction complete. The database is already compact.';

  @override
  String get selectSecret => 'Choose a secret';

  @override
  String get noSecretsHint =>
      'Add a secret in Settings → Security → Secret vault first.';

  @override
  String get secretLoadFailed => 'Could not load the secret vault. Try again.';

  @override
  String get duplicateProvider => 'Duplicate AI provider';

  @override
  String get addProvider => 'Add AI provider';

  @override
  String get editProvider => 'Edit provider';

  @override
  String get configurationNameRequired => 'Configuration name *';

  @override
  String get configurationNameHint => 'For example: Main GPT';

  @override
  String get providerType => 'Provider type';

  @override
  String get openAiCompatible => 'OpenAI compatible';

  @override
  String get modelRequired => 'Model *';

  @override
  String get secretVault => 'Secret vault';

  @override
  String get manualInput => 'Manual input';

  @override
  String get apiKeyHintNew =>
      'Enter an API key, or choose one using the key button';

  @override
  String get apiKeyHintExisting => 'Leave blank to keep the current key';

  @override
  String get chooseFromVault => 'Choose from secret vault';

  @override
  String get contextWindow => 'Context window (tokens)';

  @override
  String get contextWindowHint =>
      'Leave blank for a conservative default, or enter the measured limit';

  @override
  String get maximumOutput => 'Maximum output (tokens)';

  @override
  String get optional => 'Optional';

  @override
  String get selectSecretFirst => 'Choose a secret first';

  @override
  String get saveProvider => 'Save provider';

  @override
  String get noProviders =>
      'No AI providers configured. Select Add to get started.';

  @override
  String get active => 'Active';

  @override
  String get deleteConfiguration => 'Delete configuration';

  @override
  String get addSecret => 'Add secret';

  @override
  String get editSecret => 'Edit secret';

  @override
  String get secretCodeRequired => 'Unique code *';

  @override
  String get secretCodeCreateHint =>
      'Letters, numbers, underscores or hyphens. Fixed after creation.';

  @override
  String get secretCodeEditHint =>
      'The reference code is fixed. You can change the name.';

  @override
  String get nameRequired => 'Name *';

  @override
  String get secretNameHint => 'For example: Personal GitHub token';

  @override
  String get secretValueRequired => 'Secret value *';

  @override
  String get newSecretValueOptional => 'New secret value (optional)';

  @override
  String get keepSecretValueHint =>
      'Leave blank to keep the secret and update only its name';

  @override
  String get replaceMatchingCredentials =>
      'Replace matching credentials with references';

  @override
  String get matchingCredentialsHint =>
      'Match MCP environment variables, headers and search keys exactly';

  @override
  String get notReferenced => 'Not referenced';

  @override
  String get nameRequiredError => 'Enter a name';

  @override
  String get secretFieldsRequired => 'Enter a unique code and secret value';

  @override
  String get deleteSecretTitle => 'Delete secret?';

  @override
  String get secretVaultDescription =>
      'Secrets stay on this device. Use {{secret:code}} in MCP environment variables, headers or search API keys to reference them.';

  @override
  String get promptBody => 'Prompt body';

  @override
  String get promptEditHint =>
      'Saving creates a draft. Activation affects the next run. Protocols and execution permissions remain enforced by the application.';

  @override
  String get promptReadOnlyHint =>
      'Output contracts and execution rules are read-only.';

  @override
  String get promptDraftSaved => 'Draft saved. You can activate it now.';

  @override
  String get viewDefault => 'View default';

  @override
  String get settingsEditNote => 'Edited in settings';

  @override
  String get saveDraft => 'Save draft';

  @override
  String get activateDraft => 'Activate draft';

  @override
  String get builtin => 'Built-in';

  @override
  String get custom => 'Custom';

  @override
  String get currentlyActive => 'Currently active';

  @override
  String get activateRevision => 'Activate this revision';

  @override
  String get recentRunVersions => 'Versions for the last 20 runs';

  @override
  String get promptLocalDescription =>
      'Prompts stay on this device. Roles and processing methods support revisions and rollback; fixed contracts are read-only. Current resources are Chinese.';

  @override
  String get loadPrompts => 'Load prompts';

  @override
  String get viewRunVersions => 'View run versions';

  @override
  String get editable => 'Editable';

  @override
  String get readOnlyContract => 'Read-only contract';

  @override
  String get revisionHistory => 'Revision history and rollback';

  @override
  String get restoreDefault => 'Restore default';

  @override
  String get syncPlatformUnavailable =>
      'Sync is not available on this platform';

  @override
  String get todos => 'To-dos';

  @override
  String get drafts => 'Drafts';

  @override
  String get search => 'Search';

  @override
  String get lockedTitle => 'Elsewhen is locked';

  @override
  String get incorrectPassword => 'Incorrect password';

  @override
  String get secureStorageUnavailable =>
      'Secure storage is unavailable. Try again later.';

  @override
  String get passwordInputHint => 'Enter your password, then press Enter';

  @override
  String get verifying => 'Verifying…';

  @override
  String get biometricUnlock => 'Unlock with fingerprint or face';

  @override
  String get usePinEntry => 'Use the 4-digit keypad';

  @override
  String get legacyPasswordEntry =>
      'Old password is not 4 digits? Use text entry';

  @override
  String sectionLoadFailed(String section, String error) {
    return 'Could not load $section: $error';
  }

  @override
  String secretUsedBy(String references) {
    return 'Used by: $references';
  }

  @override
  String deleteSecretConfirmation(String name) {
    return 'Delete $name?';
  }

  @override
  String secretStillUsedBy(String references) {
    return 'Still referenced by $references. Deleting it will prevent those services from connecting.';
  }

  @override
  String deleteProviderConfirmation(String name, String warning) {
    return 'Delete provider “$name”?$warning';
  }

  @override
  String get activeProviderDeletionWarning =>
      ' The first remaining provider will become active.';

  @override
  String providerCopyName(String name) {
    return '$name copy';
  }

  @override
  String get switchProvider => 'Switch provider';

  @override
  String get deleteProvider => 'Delete provider';

  @override
  String promptHistoryTitle(String title) {
    return '$title · Last 30 revisions';
  }

  @override
  String approximateTokens(int tokens) {
    String _temp0 = intl.Intl.pluralLogic(
      tokens,
      locale: localeName,
      other: 'About $tokens tokens',
      one: 'About 1 token',
    );
    return '$_temp0';
  }

  @override
  String get promptPersonalPolicy => 'Conversation role and style';

  @override
  String get promptChatContract => 'Conversation protocol and execution rules';

  @override
  String get promptSummaryContract => 'History summary contract';

  @override
  String get promptReadingContract => 'Full-text reading contract';

  @override
  String get promptDigestContract => 'Knowledge processing contract';

  @override
  String get promptSummaryPolicy => 'History summary focus';

  @override
  String get promptReadingPolicy => 'Full-text reading focus';

  @override
  String get promptDigestPolicy => 'Knowledge processing method';

  @override
  String get promptAnalysisPolicy => 'Event analysis method';

  @override
  String lastFailure(String time, String error) {
    return 'Last failure ($time): $error';
  }

  @override
  String exportSuccess(int count, String folder) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Exported $count knowledge pages to $folder',
      one: 'Exported 1 knowledge page to $folder',
    );
    return '$_temp0';
  }

  @override
  String get wikiExportDescription =>
      'Export knowledge pages as Markdown with metadata for tools such as Obsidian. Derivatives and archived or rejected pages are excluded. Repeating an export replaces files with the same names.';

  @override
  String databaseSize(String file, String free, String advice) {
    return 'Database: $file MB, including $free MB reclaimable from deleted records.$advice';
  }

  @override
  String get compactionRecommended => ' Compaction is recommended.';

  @override
  String databaseSpaceReclaimed(String size) {
    return 'Compaction complete. Reclaimed $size MB.';
  }

  @override
  String get noAiUsage =>
      'No AI usage yet. After an AI reply, daily token usage appears here.';

  @override
  String get last30DaysTokens => 'Tokens in the last 30 days';

  @override
  String get totalCalls => 'Total calls';

  @override
  String callCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count calls',
      one: '1 call',
    );
    return '$_temp0';
  }

  @override
  String get listSeparator => ', ';

  @override
  String get providerConfiguration => 'AI provider configuration';

  @override
  String get dailyTokenUsage => 'Daily token usage';

  @override
  String get analysisQueue => 'Event analysis queue';

  @override
  String get knowledgeProcessing => 'Knowledge processing';

  @override
  String get ruleLibrary => 'Rule library';

  @override
  String get tweetFetchService => 'Post retrieval service';

  @override
  String get providerInUse => 'Used for conversations';

  @override
  String get activateProvider => 'Set active';

  @override
  String get duplicateProviderTooltip =>
      'Duplicate configuration (enter the API key again)';

  @override
  String get keyNotConfigured => 'Key not configured';

  @override
  String get notConfigured => 'Not configured';

  @override
  String get providerNameMissing => 'Enter a configuration name';

  @override
  String get providerFieldsMissing => 'Enter a base URL and model';

  @override
  String get apiKeyRequiredForNew =>
      'Enter an API key for the new configuration';

  @override
  String get chooseFont => 'Choose font';

  @override
  String get searchFonts => 'Search fonts…';

  @override
  String get clear => 'Clear';

  @override
  String get followGlobal => 'Follow global setting';

  @override
  String get useGlobalFont => 'Use the global font setting';

  @override
  String get useSystemFont => 'Use the operating system font';

  @override
  String get noMatchingFonts => 'No matching fonts';

  @override
  String get fontPreview => 'AaBbGg 1234 · ';

  @override
  String get endpointMalformed =>
      'Check the provider URL format. The address will not be changed or tested automatically.';

  @override
  String get endpointLoopback =>
      'This loopback address points to the device using it. On another device, it will point to that device itself. Sync preserves the address without probing it or switching providers.';

  @override
  String get endpointUnspecified =>
      'This unspecified address is usually used for listening. Check which provider address is reachable from each device. It will not be replaced or probed automatically.';

  @override
  String get endpointPrivate =>
      'This is a private or link-local address. Other devices need access to that network. Sync preserves the configuration without verifying reachability or sending model requests.';

  @override
  String operationFailed(String action) {
    return '$action failed. Please try again.';
  }

  @override
  String localFontCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Local fonts ($count)',
      one: 'Local fonts (1)',
    );
    return '$_temp0';
  }

  @override
  String get secretManagement => 'Secret management';

  @override
  String get appPassword => 'App password';

  @override
  String get openNavigation => 'Open navigation';

  @override
  String get createTopic => 'Create topic';

  @override
  String get importUrl => 'Import URL';

  @override
  String get hideSearch => 'Hide search';

  @override
  String get searchAndDate => 'Search and date';

  @override
  String get aiReplying => 'Replying';

  @override
  String get aiReplyingHint => 'AI is generating a reply';

  @override
  String get aiAnalyzing => 'Analyzing';

  @override
  String get aiAnalyzingHint => 'AI is analyzing records';

  @override
  String get aiDigesting => 'Processing';

  @override
  String get aiDigestingHint => 'AI is processing new records';

  @override
  String get aiOrganizing => 'Organizing';

  @override
  String get aiOrganizingHint => 'AI is organizing the knowledge base';

  @override
  String get aiChecking => 'Checking';

  @override
  String get aiCheckingHint => 'AI is checking periodic reports';

  @override
  String goalsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Goals ($count)',
      zero: 'Goals',
    );
    return '$_temp0';
  }

  @override
  String todoCount(int count) {
    return 'To-dos ($count)';
  }

  @override
  String get locateByDate => 'Find by date';

  @override
  String get searchMessages => 'Search your messages';

  @override
  String get clearLocation => 'Clear location';

  @override
  String get backToNow => 'Back to now';

  @override
  String get jumpToDate => 'Jump to date';

  @override
  String get jump => 'Go';

  @override
  String get showEarlierMessages => 'Show earlier messages';

  @override
  String get startConversation => 'Choose a conversation to start chatting';

  @override
  String get noMessages => 'No messages yet';

  @override
  String get startTyping => 'Start a conversation below';

  @override
  String conversationCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count conversations',
      one: '1 conversation',
    );
    return '$_temp0';
  }

  @override
  String todoCountItems(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count to-dos',
      one: '1 to-do',
    );
    return '$_temp0';
  }

  @override
  String get copyAll => 'Copy all messages';

  @override
  String get flipPast => 'Mark up to here';

  @override
  String get topics => 'Topics';

  @override
  String get systemNotice => 'System notice';

  @override
  String get copyMessage => 'Copy';

  @override
  String get saveAsTopic => 'Save as topic';

  @override
  String get saveAsTopicDescription =>
      'Extract a title and body, then confirm to save it as a topic';

  @override
  String get regenerate => 'Regenerate';

  @override
  String get recordCategory => 'Record category';

  @override
  String get record => 'Record';

  @override
  String get discussion => 'Discussion';

  @override
  String get includeInRecords => 'Include in records';

  @override
  String get asDiscussion => 'Keep as discussion';

  @override
  String get reanalyze => 'Analyze again';

  @override
  String get savedAsRecord => 'Included in records';

  @override
  String get savedAsDiscussion =>
      'Kept as discussion; it will not enter the daily stream or long-term facts';

  @override
  String get reanalyzeQueued => 'Added to the analysis queue';

  @override
  String get saveMessageAsTopic => 'Save this message as a topic';

  @override
  String get thinking => 'AI is thinking…';

  @override
  String get startGenerating => 'Starting generation…';

  @override
  String get readingGeneration => 'Reading generation status…';

  @override
  String get generationFailedSaved => 'Generation failed; input was saved';

  @override
  String get generationStopped => 'Generation stopped';

  @override
  String get generationInterrupted => 'Generation interrupted';

  @override
  String get stopping => 'Stopping…';

  @override
  String get stopGeneration => 'Stop generation';

  @override
  String get regenerateThis => 'Regenerate this message';

  @override
  String get reconnect => 'Reconnect';

  @override
  String actualToolSteps(int count) {
    return 'Actual tool steps (last $count)';
  }

  @override
  String get chooseDirectory => 'Choose a directory to import';

  @override
  String get selectConversation => 'Choose a conversation first';

  @override
  String get noSupplementTask =>
      'There is no task to supplement. Choose save or queue.';

  @override
  String sendFailed(String error) {
    return 'Send failed: $error';
  }

  @override
  String get savedButNotEligible =>
      'Input saved, but generation eligibility was not confirmed. Continue from that message instead of sending it again.';

  @override
  String enteredTopic(String title) {
    return 'Opened topic: $title';
  }

  @override
  String get thinkingNext => 'AI is thinking. You can enter the next message…';

  @override
  String get inputMessage => 'Enter a message…';

  @override
  String get closeInputFormat => 'Close input format';

  @override
  String get addContent => 'Add content';

  @override
  String get selectDirectory => 'Choose directory';

  @override
  String get viewInputFormat => 'View input format';

  @override
  String get queueNext => 'Save and queue the next question';

  @override
  String get send => 'Send';

  @override
  String get recordOnlyHelp => 'Save as a record without requesting AI';

  @override
  String get formatDirectory => 'Import a project or file';

  @override
  String get formatDirectoryHelp =>
      'Inspect `/path/to/project` and import it into the knowledge base';

  @override
  String get formatConfirm => 'Confirm / cancel';

  @override
  String get formatConfirmHelp => 'Handle an AI draft';

  @override
  String get mentionPerson => '@person';

  @override
  String get tagTopic => '#topic';

  @override
  String get link => 'Link';

  @override
  String get directory => 'Directory';

  @override
  String get unverifiedDraft =>
      'Unverified draft · Does not confirm successful actions or valid citations';

  @override
  String get toolRunning => 'In progress';

  @override
  String get toolUncertainNoRetry =>
      'Result unverified; will not retry automatically';

  @override
  String get toolReadCompleted => 'Query completed';

  @override
  String get toolProposalReady => 'Draft ready for review; not executed';

  @override
  String get toolExecuted => 'Action executed (confirmed receipt)';

  @override
  String get toolFailed => 'Execution failed';

  @override
  String get toolUncertain => 'Result unverified';

  @override
  String get toolListRules => 'Find rules';

  @override
  String get toolListPages => 'Browse knowledge pages';

  @override
  String get toolReadPage => 'Read knowledge page';

  @override
  String get toolUsage => 'Check usage';

  @override
  String get toolSearchKnowledge => 'Search knowledge base';

  @override
  String get toolReadLink => 'Read link';

  @override
  String get toolReadWeb => 'Read web page';

  @override
  String get toolRecordEvent => 'Record event';

  @override
  String get toolDraftPage => 'Draft knowledge page';

  @override
  String get toolSkillScript => 'Run skill script';

  @override
  String get toolMcp => 'Call MCP tool';

  @override
  String get toolListTodos => 'Find to-dos';

  @override
  String get toolDraftTodo => 'Draft to-do';

  @override
  String get toolDraftRelations => 'Draft contact links';

  @override
  String get toolExtractRelations => 'Organize contact links';

  @override
  String get toolEventsByDate => 'Find events by date';

  @override
  String get toolArchiveChats => 'Prepare conversation archive';

  @override
  String get toolRenamePage => 'Prepare page rename';

  @override
  String get toolImportSource => 'Prepare source import';

  @override
  String get toolInspectDirectory => 'Inspect project directory';

  @override
  String get toolImportFiles => 'Prepare directory file import';

  @override
  String get toolRevisePage => 'Draft page revision';

  @override
  String get toolStep => 'Process tool step';

  @override
  String get gerundPreparing0 => 'Reviewing memories…';

  @override
  String get gerundPreparing1 => 'Organizing thoughts…';

  @override
  String get gerundPreparing2 => 'Reviewing the conversation…';

  @override
  String get gerundPreparing3 => 'Looking through knowledge…';

  @override
  String get gerundPreparing4 => 'Gathering context…';

  @override
  String get gerundPreparing5 => 'Putting context together…';

  @override
  String get gerundAwaiting0 => 'Thinking it through…';

  @override
  String get gerundAwaiting1 => 'Shaping a response…';

  @override
  String get gerundAwaiting2 => 'Considering the details…';

  @override
  String get gerundAwaiting3 => 'Weighing the options…';

  @override
  String get gerundAwaiting4 => 'Connecting ideas…';

  @override
  String get gerundAwaiting5 => 'Composing a reply…';

  @override
  String get gerundAwaiting6 => 'Refining the wording…';

  @override
  String get gerundAwaiting7 => 'Working through the details…';

  @override
  String get copyThisMessage => 'Copy this message';

  @override
  String get message => 'Message';

  @override
  String get me => 'Me';

  @override
  String get updateMessageStatus => 'Update message status';

  @override
  String get eventAnalyzing => 'This event is being analyzed. Try again later.';

  @override
  String get inputGuidePerson => 'Mention a contact';

  @override
  String get inputGuideTopic => 'Open a topic';

  @override
  String get inputGuideLink => 'Import content';

  @override
  String get aiProviderMissing =>
      'No AI provider is configured. Add one in Settings and try again.';

  @override
  String copiedItem(String label) {
    return 'Copied $label';
  }

  @override
  String importDirectoryPrompt(String path) {
    return 'Inspect the project in `$path` and import it into the knowledge base';
  }

  @override
  String messageLoadFailed(String error) {
    return 'Could not load messages\n$error';
  }

  @override
  String aiReplyFailed(String error) {
    return 'AI reply failed: $error';
  }

  @override
  String get goalCoAuthorReady =>
      'Goal context is ready in the Goals AI input. Review or edit it, then send to plan together.';

  @override
  String get goalStudio => 'AI planning studio';

  @override
  String get newItem => 'New';

  @override
  String get newGoal => 'New goal';

  @override
  String get goalManageInPage => 'Manage and discuss goals on the Goals page';

  @override
  String get goalStudioHelp =>
      'Describe an intention to AI. It checks feasibility and assumptions, then proposes a long-term plan with milestones and verifiable outcomes. You confirm before anything is saved.';

  @override
  String get goalTreeHelp =>
      'Confirmed plans appear in the goal tree. Select a goal to open its dedicated discussion.';

  @override
  String get noActiveGoals =>
      'No active goals yet. Create one or plan with AI on the Goals page.';

  @override
  String get planWithAi => 'Plan together with AI';

  @override
  String get reviewGoalsWithAi => 'Review or plan with AI';

  @override
  String get noGoalsInStatus => 'No goals with the selected status';

  @override
  String get noActiveGoalsCreate =>
      'No active goals. Select New or plan together with AI.';

  @override
  String get goalCapHint =>
      'Active limit reached. The new goal will start paused.';

  @override
  String get newGoalHint =>
      'Describe a new goal… (for example: Release Elsewhen 1.0)';

  @override
  String get collapseGoalContext => 'Hide goal context';

  @override
  String get addGoalContext => 'Add motivation, action or obstacle';

  @override
  String get goalWhyOptional => '💡 Why / motivation (optional)';

  @override
  String get goalWhyHint =>
      'For example: Validate the complete personal knowledge workflow';

  @override
  String get goalActionOptional =>
      '📍 Action for the next week or two (optional)';

  @override
  String get goalActionHint =>
      'For example: Resolve snapshot convergence across devices';

  @override
  String get goalObstacleOptional => '🚧 Current obstacle (optional)';

  @override
  String get goalObstacleHint =>
      'For example: Device certificate setup is complicated';

  @override
  String get goalDoneOptional => '🎯 Completion criteria (optional)';

  @override
  String get goalDoneHint => 'For example: All device tests pass';

  @override
  String get collapsePhase => 'Collapse phase';

  @override
  String get expandPhase => 'Expand phase';

  @override
  String get currentPhase => 'Current phase';

  @override
  String get moreActions => 'More actions';

  @override
  String get reactivateGoal => 'Reactivate goal';

  @override
  String get adjustGoalAction => 'Adjust action or obstacle';

  @override
  String get completeGoal => 'Complete goal (archive as achievement)';

  @override
  String get pauseGoal => 'Pause goal (resume anytime)';

  @override
  String get editGoalDetails => 'Edit details';

  @override
  String get currentActionLabel => 'Current action: ';

  @override
  String get currentObstacleLabel => 'Current obstacle: ';

  @override
  String get goalActionNotSet =>
      'Not set. Add an action for the next week or two.';

  @override
  String get nextPhaseHint =>
      'After completing a phase, focus on the next action to start within a week or two.';

  @override
  String get currentActionField => '📍 Current action';

  @override
  String get nextActionHint =>
      'For example: Implement and test the initial snapshot algorithm';

  @override
  String get clearObstacleField => '🚧 Current obstacle (leave blank to clear)';

  @override
  String get nextObstacleHint =>
      'For example: Waiting for network port confirmation';

  @override
  String get updateAction => 'Update action';

  @override
  String get restartGoalHint =>
      'To resume this goal, confirm or set the next concrete action:';

  @override
  String get restartActionField => '📍 Next concrete action';

  @override
  String get restartActionHint =>
      'For example: Map dependencies and plan the refactor';

  @override
  String get restartObstacleField => '🚧 Current obstacle (optional)';

  @override
  String get restartObstacleHint =>
      'For example: Required test devices are unavailable';

  @override
  String get confirmReactivate => 'Confirm reactivation';

  @override
  String get editFullGoal => 'Edit goal details';

  @override
  String get goalVision => 'Goal vision';

  @override
  String get goalWhy => '💡 Why / motivation';

  @override
  String get goalNextAction => '📍 Action for the next week or two';

  @override
  String get goalObstacle => '🚧 Current obstacle';

  @override
  String get goalDone => '🎯 Completion criteria';

  @override
  String get saveChanges => 'Save changes';

  @override
  String get goalActionSaved => 'Action and obstacle updated';

  @override
  String get goalDetailsSaved => 'Goal details saved';

  @override
  String get collapseDiscussion => 'Collapse discussion';

  @override
  String get goalStudioInputHint =>
      'Describe your long-term intention… (drafts are not sent automatically)';

  @override
  String get goalChatInputHint =>
      'Discuss this goal with AI… (drafts are not sent automatically)';

  @override
  String get generateReply => 'Generate reply';

  @override
  String get goalStudioWelcome =>
      '🎯 Describe your long-term intention. I will assess feasibility and assumptions, then propose a plan and milestones. You confirm before anything is saved.';

  @override
  String get goalChatWelcome =>
      '🎯 Let’s discuss this goal. I can help plan actions, examine obstacles and review completion criteria. Pausing, completing or resuming a goal requires your confirmation.';

  @override
  String goalPaused(String goal) {
    return 'Paused “$goal”';
  }

  @override
  String goalCompletedToast(String goal) {
    return '🎉 Completed goal “$goal”!';
  }

  @override
  String goalReactivated(String goal) {
    return 'Reactivated “$goal”';
  }

  @override
  String goalCapacity(int count) {
    return 'There are already $count active goals. Pause or complete one before reactivating this goal.';
  }

  @override
  String activeGoalCount(int count, int maximum) {
    return 'Active goals $count / $maximum';
  }

  @override
  String goalLoadFailed(String error) {
    return 'Could not load goals: $error';
  }

  @override
  String newGoalPaused(String goal) {
    return 'Active limit reached. “$goal” was created as paused.';
  }

  @override
  String newGoalCreated(String goal) {
    return 'Created goal “$goal”';
  }

  @override
  String newGoalCapacityHint(int count) {
    return 'The limit of $count active goals has been reached. New goals start paused and can be resumed anytime.';
  }

  @override
  String whyGoal(String why) {
    return 'Why: $why';
  }

  @override
  String doneGoal(String done) {
    return 'Completion criteria: $done';
  }

  @override
  String adjustActionTitle(String goal) {
    return 'Adjust current action · $goal';
  }

  @override
  String reactivateGoalTitle(String goal) {
    return 'Resume goal · $goal';
  }

  @override
  String motivationReview(String why) {
    return '💡 Motivation: $why';
  }

  @override
  String goalDiscussionTitle(String goal) {
    return 'Goal discussion · $goal';
  }

  @override
  String goalNewSeed(String brief) {
    return 'I want to plan a new long-term goal. Help me clarify why, what completion means, the first concrete action for the next week or two, and likely obstacles. $brief';
  }

  @override
  String goalReviewSeed(String brief) {
    return 'Help review these goals: which actions are ready for the next phase? Or help me plan a new long-term goal. $brief';
  }

  @override
  String goalBriefItem(String goal, String action) {
    return '“$goal” (action: $action)';
  }

  @override
  String goalBrief(String goals) {
    return 'Current active goals: $goals.';
  }

  @override
  String get noGoalsBrief => 'There are no active goals yet.';

  @override
  String get notSet => 'Not set';

  @override
  String get planNewGoal => 'Plan or create a goal with AI';

  @override
  String get nearTerm => 'Near term';

  @override
  String get midTerm => 'Mid term';

  @override
  String get longTerm => 'Long term';

  @override
  String get activeGoals => 'Active';

  @override
  String get pausedGoals => 'Paused';

  @override
  String get achievedGoals => 'Achieved';

  @override
  String get inProgressGoal => 'In progress';

  @override
  String get pausedGoalState => 'Paused';

  @override
  String get pausedShort => 'Paused';

  @override
  String get initializingConversation => 'Initialize conversation';

  @override
  String get loadMessages => 'Load messages';

  @override
  String get examplePerson => '@Alex ';

  @override
  String get exampleTopic => '#payment-process';

  @override
  String goalSummaryCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Goals ($count)',
      zero: 'Goals',
    );
    return '$_temp0';
  }

  @override
  String get supplementUnsupported =>
      'Not added: this material cannot be safely merged. It was retained; process it separately or withdraw it.';

  @override
  String get supplementInputChanged =>
      'Not added: the input version changed. It remains queued.';

  @override
  String get supplementContextChanged =>
      'Not added: material version or scope differs. Retained as the next question.';

  @override
  String get supplementBudget =>
      'Not added: request budget or protocol limits. It remains queued.';

  @override
  String get supplementClosed =>
      'The target task ended. Generation waits until the queue resumes.';

  @override
  String get supplementWaiting =>
      'Waiting for the tool turn to finish. The input remains eligible for the queue until added.';

  @override
  String get queueUnavailable =>
      'Queue status is unavailable. Inputs will not be saved again.';

  @override
  String get queueActionUnconfirmed =>
      'Queue action is unconfirmed. Refresh to check.';

  @override
  String get queuePaused => 'Generation queue paused';

  @override
  String get queueOrdered => 'Generation queue · Processed in saved order';

  @override
  String get resumeQueue => 'Resume queue';

  @override
  String get pauseQueue => 'Pause queue';

  @override
  String get queueInputDescription =>
      'Queued inputs have not been sent to the model. Withdrawing only cancels generation eligibility.';

  @override
  String get withdrawGeneration => 'Withdraw generation';

  @override
  String get moreQueuedInputs =>
      'More inputs are queued. They appear as earlier inputs are processed.';

  @override
  String get injectedSupplements => 'Supplements added to the task';

  @override
  String get injectedSupplementsHint =>
      'Added to the next request context; delivery and model completion are not guaranteed. These inputs will not be queued again.';

  @override
  String get wikiChat => 'AI chat';

  @override
  String get moveChatWindow => 'Drag to move the conversation window';

  @override
  String get shrinkChatWindow => 'Minimize conversation window';

  @override
  String get backToMainChat => 'Back to main chat';

  @override
  String get aiTasks => 'AI tasks';

  @override
  String get summarizePage => 'Summarize this page';

  @override
  String get extractKeyPoints => 'Extract key points';

  @override
  String get captureDecisions => 'Capture decisions and reasons';

  @override
  String get organizeNextSteps => 'Organize next steps';

  @override
  String get translateSelection => 'Translate selection';

  @override
  String get writeCopy => 'Write copy';

  @override
  String get rescanProject => 'Rescan project directory';

  @override
  String get reimportSource => 'Reimport source';

  @override
  String get taskResetDescription =>
      'Reselecting uses the current page revision and clears the previous selection and comparison materials.';

  @override
  String get reselectTaskMaterial => 'Reselect task and materials';

  @override
  String get askPageHint => 'Ask AI about this page…';

  @override
  String get pageChatWelcome =>
      '👋 I can summarize, extract key points, expand or rewrite this page. Changes to the knowledge base require your confirmation.\nFor a project imported from a local directory, ask to rescan it to import the latest files.';

  @override
  String get summarizePagePrompt =>
      'Summarize this page and highlight the most important information.';

  @override
  String get keyPointsPrompt =>
      'Extract the key facts, assessments and reusable information from this page.';

  @override
  String get decisionsPrompt =>
      'Organize the decisions already established on this page and their reasons.';

  @override
  String get nextStepsPrompt =>
      'Organize the next actions and unresolved questions from this page.';

  @override
  String get translateSelectionPrompt =>
      'Translate the selected excerpt into XX, preserving structure and technical meaning. Work only on the provided excerpt; state the scope and do not claim to translate the whole page.';

  @override
  String get writeCopyPrompt =>
      'Based on this page, draft copy for XX channel, aimed at XX audience, with a XX tone.';

  @override
  String get projectRescannedAssets =>
      'Project directory rescanned; asset knowledge page updated';

  @override
  String get replaceProjectPage => 'Replace project page content?';

  @override
  String get replaceProjectDescription =>
      'The current AI reply will replace the entire page body. The previous content remains in revision history.';

  @override
  String get replace => 'Replace';

  @override
  String get projectPageReplaced => 'Current project page content replaced';

  @override
  String get pageMissing => 'Page not found; it may have been deleted';

  @override
  String get projectRescanned =>
      'Project directory rescanned; knowledge page updated';

  @override
  String get sourceReimported => 'Source reimported; knowledge page updated';

  @override
  String get thinkingShort => 'Thinking…';

  @override
  String get copiedMessage => 'Message copied';

  @override
  String get replaceBody => 'Replace content';

  @override
  String get saveAsOutput => 'Save as output';

  @override
  String get aiReply => 'AI reply';

  @override
  String get selectReceipt => 'Select a record on the left to view its receipt';

  @override
  String get backToList => 'Back to list';

  @override
  String get selectProposal =>
      'Select a proposal on the left to view its details';

  @override
  String get pendingReview => 'Pending review';

  @override
  String get decisionHistory => 'Decision history';

  @override
  String get refresh => 'Refresh';

  @override
  String get noDecisions => 'No decision history';

  @override
  String get noProposals => 'No pending proposals';

  @override
  String get previousPage => 'Previous';

  @override
  String get nextPage => 'Next';

  @override
  String get parametersSources => 'Parameters and sources';

  @override
  String get midTermGoal => 'Mid-term goal';

  @override
  String get longTermGoal => 'Long-term goal';

  @override
  String get nearTermGoal => 'Near-term goal';

  @override
  String get motivationLabel => 'Motivation: ';

  @override
  String get completionCriteriaLabel => 'Completion criteria: ';

  @override
  String get feasibilityAssessment => 'Feasibility assessment';

  @override
  String get assumptions => 'Assumptions';

  @override
  String get assumption => 'Assumption';

  @override
  String get milestoneSteps =>
      'Milestones (the first starts after confirmation; the rest are queued)';

  @override
  String get proposalTitle => 'Title';

  @override
  String get proposalBody => 'Body';

  @override
  String get goalVisionShort => 'Goal vision';

  @override
  String get ruleContent => 'Rule content';

  @override
  String get recordContent => 'Record content';

  @override
  String get note => 'Note';

  @override
  String get dueIsoOptional => 'Due time (ISO 8601, optional)';

  @override
  String get priorityOptions => 'Priority (low/normal/high)';

  @override
  String get changeNote => 'Change note';

  @override
  String get newTitle => 'New title';

  @override
  String get phaseOptions => 'Phase (near/mid/long)';

  @override
  String get visionOptional => 'Vision (optional)';

  @override
  String get assessmentOptional => 'Feasibility assessment (optional)';

  @override
  String get whyMotivation => 'Why / motivation';

  @override
  String get skillsExperience => 'Skills and experience';

  @override
  String get availableResources => 'Available resources';

  @override
  String get timeConstraints => 'Time and practical constraints';

  @override
  String get preferencesBoundaries => 'Preferences and boundaries';

  @override
  String get actionWeeks => 'Concrete action for the next week or two';

  @override
  String get obstaclesExpected => 'Current or expected obstacles';

  @override
  String get completionCriteria => 'Completion criteria';

  @override
  String get reviewGoalBasis => 'Goal revision used for review';

  @override
  String get editDraftHint =>
      'Saving only updates the draft. Review it again and accept to execute the action.';

  @override
  String get rejectThis => 'Reject this';

  @override
  String get acceptThis => 'Accept this';

  @override
  String get processingEllipsis => 'Processing…';

  @override
  String get saveWithoutExecution => 'Save changes without executing';

  @override
  String get batchReviewDescription =>
      'Only the listed revisions are processed. Actions run individually and may partially succeed. Each action shows its result.';

  @override
  String get targetVersionBasis => 'Target revision basis';

  @override
  String get batchFinished => 'Processing finished. Check each result.';

  @override
  String get rejectThese => 'Reject these';

  @override
  String get acceptThese => 'Accept these';

  @override
  String get processingItems => 'Processing items…';

  @override
  String get reviewProposals => 'Review proposals';

  @override
  String get globalRuleReview => 'Legacy global rule review';

  @override
  String get conversationProposalReview => 'Conversation proposal review';

  @override
  String get actionActivateRule => 'Personal rule';

  @override
  String get actionCreateTodo => 'Create to-do';

  @override
  String get actionSavePage => 'Save knowledge page';

  @override
  String get actionRevisePage => 'Revise or derive page';

  @override
  String get actionRenamePage => 'Rename knowledge page';

  @override
  String get actionRecordEvent => 'Save record';

  @override
  String get actionPeopleRelations => 'Contacts and relations';

  @override
  String get actionArchiveChats => 'Archive conversations';

  @override
  String get actionImportSource => 'Import source';

  @override
  String get actionImportFiles => 'Import directory files';

  @override
  String get actionImportProject => 'Import project summary';

  @override
  String get actionPlanningProfile => 'Personal profile';

  @override
  String get actionCreateGoal => 'Create goal';

  @override
  String get actionUpdateGoal => 'Update goal context';

  @override
  String get actionCreatePlan => 'Long-term plan';

  @override
  String get actionAddSteps => 'Add plan steps';

  @override
  String get receiptWaiting => 'Waiting for execution receipt';

  @override
  String get receiptFailedRolledBack => 'Execution failed (rolled back)';

  @override
  String get declined => 'Rejected';

  @override
  String reviewSelectedCount(int count) {
    return 'Review selected ($count)';
  }

  @override
  String reviewSelectedTitle(int count) {
    return 'Review $count selected items';
  }

  @override
  String pageNumber(int page) {
    return 'Page $page';
  }

  @override
  String proposalVersion(String action, String version) {
    return '$action · Revision $version';
  }

  @override
  String updateGoalById(String id) {
    return 'Update goal context (ID: $id)';
  }

  @override
  String invalidProposal(String args) {
    return 'This proposal has an invalid format and cannot be accepted. Reject it and request a new draft.\n$args';
  }

  @override
  String goalId(String id) {
    return 'Goal ID: $id';
  }

  @override
  String planLabel(String plan) {
    return 'Plan: $plan';
  }

  @override
  String intentLabel(String intent) {
    return 'Intention: “$intent”';
  }

  @override
  String visionLabel(String vision) {
    return 'Vision: $vision';
  }

  @override
  String criterionLabel(String done) {
    return ' (criterion: $done)';
  }

  @override
  String proposalReopen(String error) {
    return '$error\nClose and reopen the review. A new revision will not be accepted automatically.';
  }

  @override
  String fullReceiptCount(int count) {
    return 'Show full receipt ($count characters)';
  }

  @override
  String batchUnconfirmed(String error) {
    return 'Completion is unconfirmed. Check decision history; this action will not retry automatically: $error';
  }

  @override
  String taskRestoreFailed(String error) {
    return 'The original task cannot be restored: $error';
  }

  @override
  String reloadMaterialFailed(String error) {
    return 'Could not reload materials: $error';
  }

  @override
  String savedAsDerivative(String title) {
    return 'Saved as derivative: $title';
  }

  @override
  String get rescan => 'Rescan';

  @override
  String get reimport => 'Reimport';

  @override
  String get readPage => 'Read page';

  @override
  String get aiSummary => 'AI summary';

  @override
  String get justNow => 'Just now';

  @override
  String minutesAgo(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count minutes ago',
      one: '1 minute ago',
    );
    return '$_temp0';
  }

  @override
  String hoursAgo(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count hours ago',
      one: '1 hour ago',
    );
    return '$_temp0';
  }

  @override
  String daysAgo(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count days ago',
      one: '1 day ago',
    );
    return '$_temp0';
  }

  @override
  String get taskLocalQuestion => 'Ask about a passage';

  @override
  String get taskFullSummary => 'Full-page summary';

  @override
  String get taskCompare => 'Compare sources';

  @override
  String get taskLocalEdit => 'Edit a passage';

  @override
  String get taskFullRevision => 'Revise the full page';

  @override
  String get taskIndependent => 'Independent question';

  @override
  String get taskMaterialChanged =>
      'The task material has changed. Reselect the task and material explicitly; the new revision will not be used automatically.';

  @override
  String get chooseMaterial => 'View / choose material';

  @override
  String get taskDisplayedRevision => 'Task and displayed revision';

  @override
  String get taskScopeHint =>
      'Follow-up questions use this task and revision. Reselect for a new task. Full-page revisions run in consecutive chunks and still require review.';

  @override
  String get refreshMaterialRevision => 'Refresh to current revision';

  @override
  String get comparisonSlug => 'Comparison material slug';

  @override
  String get readAndAdd => 'Read and add';

  @override
  String get selectionAutomatic =>
      'No selection; relevant passages will be chosen for questions';

  @override
  String get selectionTooLong =>
      'The selection exceeds 6,000 characters. Select a shorter passage.';

  @override
  String get useDisplayedRevision => 'Use the displayed revision';

  @override
  String get compassDismissHint =>
      'Handle urgent matters; pause these reminders for 7 days';

  @override
  String get compassFocusObstacle => 'Discuss this obstacle';

  @override
  String get compassDiscussProgress => 'Discuss the next step';

  @override
  String get compassUpdateAction => 'Update current action';

  @override
  String get compassSnooze => 'Pause for 7 days';

  @override
  String get compassRealigned => 'Goal action updated; direction realigned';

  @override
  String get compassDismissed => 'These reminders are paused for 7 days';

  @override
  String get compassTitle => 'Direction check-in';

  @override
  String get compassSnoozeHint => 'Pause for 7 days';

  @override
  String get compassRollAction => 'Update next action';

  @override
  String get habitDoneFallback => 'Done';

  @override
  String get habitMissedFallback => 'Not yet';

  @override
  String get habitDoneTextFallback => 'Completed today';

  @override
  String get habitMissedTextFallback => 'Not completed today';

  @override
  String get habitCheckin => 'Habit check-in';

  @override
  String get changeTime => 'Change time';

  @override
  String get pauseCheckins => 'Pause check-ins';

  @override
  String get checkinsPaused => 'Paused; resume in Settings';

  @override
  String get askLater => 'Ask later';

  @override
  String get discuss => 'Discuss';

  @override
  String get skipToday => 'Skip today';

  @override
  String get discussReason => 'Discuss why';

  @override
  String get checkinTimeUpdated => 'Check-ins will continue at the new time';

  @override
  String get checkinsUpdated => 'Check-ins updated';

  @override
  String get checkinDismissed => 'No more check-ins today';

  @override
  String get checkinSnoozed => 'Snoozed; will ask again later';

  @override
  String get checkinSuperseded => 'A newer reminder was sent';

  @override
  String get checkinExpired => 'This check-in has expired';

  @override
  String get checkinPlanClosed => 'This check-in plan is closed';

  @override
  String get checkinRecordedEvent => 'Recorded and saved as an event';

  @override
  String get askAfterHour => 'Will ask again in 1 hour';

  @override
  String get cognitiveInsight => 'Knowledge insight';

  @override
  String get adoptAsRule => 'Adopt as rule';

  @override
  String get enableCheckins => 'Enable check-ins';

  @override
  String get makeTodo => 'Make a to-do';

  @override
  String get acknowledge => 'Got it';

  @override
  String get insightRuleApplied => 'Adopted as a personal rule ✓';

  @override
  String get insightHabitEnabled => 'Habit check-ins enabled ✓';

  @override
  String get insightTodoAdded => 'Added to to-do list ✓';

  @override
  String get insightHandled => 'Handled ✓';

  @override
  String get insightDismissed => 'Acknowledged';

  @override
  String get insightExpired => 'This insight has expired';

  @override
  String get personalRuleAdopted => 'Adopted as a personal rule';

  @override
  String get addedToTodoList => 'Added to to-do list';

  @override
  String get recorded => 'Recorded';

  @override
  String get pageNotFoundDeleted => 'Page not found or deleted';

  @override
  String get habitCheckinsEnabled => 'Habit check-ins enabled';

  @override
  String get contactsSuggestionInvalid =>
      'A contact-link suggestion is available, but its data could not be read.';

  @override
  String get choose => 'Choose';

  @override
  String get contactArgsInvalid =>
      'Invalid action arguments; contacts cannot be selected';

  @override
  String get contactArgsUnreadable =>
      'Could not read the action arguments; contacts cannot be selected';

  @override
  String get selectionCancelledUnsaved =>
      'Selection cancelled; changes were not saved';

  @override
  String get wikiEmptyHelp =>
      'No knowledge pages yet.\n\nPaste a URL or text in Import,\nor ask in chat to import something into the knowledge base.';

  @override
  String get sortRecentlyUpdated => 'Recently updated';

  @override
  String get loadFailed => 'Could not load';

  @override
  String get reminderTenMinutes => '+10 min';

  @override
  String get done => 'Done';

  @override
  String get dismissReminderHint => 'Dismiss without marking done';

  @override
  String get bodyRequired => 'Enter some content';

  @override
  String get topicTitleHint =>
      'Title (optional; defaults to the first 60 characters)';

  @override
  String get topicTagsHint => 'Tags (optional; separate with spaces or commas)';

  @override
  String get markdownBodyHint => 'Content (Markdown supported)';

  @override
  String get saveToTopics => 'Save to topics';

  @override
  String materialScope(String task, String range) {
    return 'Material scope: $task$range';
  }

  @override
  String materialRange(int start, int end) {
    return ' · Characters $start–$end';
  }

  @override
  String materialTitleCount(String title, int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count characters',
      one: '1 character',
    );
    return '$title · $_temp0';
  }

  @override
  String selectionHint(String selection) {
    return 'Select up to 6,000 characters below. Current selection: $selection';
  }

  @override
  String compassPlanTitle(String plan) {
    return 'Direction check-in · Plan “$plan”';
  }

  @override
  String compassGoalTitle(String goal) {
    return 'Direction check-in · Goal “$goal”';
  }

  @override
  String compassStalled(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days days',
      one: '1 day',
    );
    return 'No progress for $_temp0';
  }

  @override
  String compassActionStalled(String action, int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days days',
      one: '1 day',
    );
    return 'Current action “$action” has stalled for $_temp0';
  }

  @override
  String compassObstacle(String obstacle) {
    return ', obstacle: $obstacle';
  }

  @override
  String compassRecentTopics(String topics) {
    return '. Recent attention has focused on “$topics”.';
  }

  @override
  String compassObstacleLine(String obstacle) {
    return 'Current obstacle: $obstacle';
  }

  @override
  String compassSeedObstacle(String goal, String obstacle, String action) {
    return 'For goal “$goal”, I am stuck on “$obstacle”. My current action is “$action”. Help me work through this.';
  }

  @override
  String compassSeedProgress(String goal, String action) {
    return 'For goal “$goal”, my current action is “$action” and progress has stalled. Help me work out the next step.';
  }

  @override
  String compassPlanStep(String plan) {
    return 'Plan “$plan” · Step';
  }

  @override
  String compassCardDetail(
    String prefix,
    String goal,
    String action,
    int days,
    String topics,
  ) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days days',
      one: '1 day',
    );
    return '$prefix “$goal”: current action “$action” has not progressed for $_temp0. Recent attention has focused on “$topics”.';
  }

  @override
  String recordedText(String text) {
    return 'Recorded: $text';
  }

  @override
  String checkinDiscussSeed(String text) {
    return '$text, because ';
  }

  @override
  String insightSuggestion(String action) {
    return 'Suggestion: $action';
  }

  @override
  String insightSource(String slug) {
    return 'Source: $slug';
  }

  @override
  String contactsSuggestion(String people, String targets) {
    return 'Contacts found: $people\nRelated items: $targets\nOpen Review proposals to accept or reject this suggestion.';
  }

  @override
  String chooseContactName(String name) {
    return 'Choose contact “$name”';
  }

  @override
  String chooseTargetName(String name) {
    return 'Choose item “$name”';
  }

  @override
  String recentPageCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count recent pages',
      one: '1 recent page',
    );
    return '$_temp0';
  }

  @override
  String incubatingCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count incubating items',
      one: '1 incubating item',
    );
    return '$_temp0 · Awaiting assessment';
  }

  @override
  String evidenceCount(int count) {
    return 'Evidence: $count';
  }

  @override
  String reminderTitle(String title, String time) {
    return 'Reminder: $title ($time)';
  }

  @override
  String topicSavedTitle(String title) {
    return 'Saved as topic: “$title”';
  }

  @override
  String get copyPageTitle => 'Copy title';

  @override
  String get pageTitleCopied => 'Page title copied';

  @override
  String get readPageFailed => 'Could not load knowledge page';

  @override
  String get pageBodyEmpty => '*(No content)*';

  @override
  String get knowledgePageMissing => 'Knowledge page not found';

  @override
  String get knowledgePageMissingHelp =>
      'This page may have been deleted or renamed. It may also be a suggested reference mentioned by AI that has not been saved to the knowledge base.';

  @override
  String get openInKnowledgeBase => 'Open in knowledge base';

  @override
  String get checkinQuestionRequired => 'Enter a check-in question';

  @override
  String get checkinTimeInvalid => 'Use HH:MM between 07:00 and 22:00';

  @override
  String get checkinDayRequired => 'Select at least one day';

  @override
  String get checkinResponsesRequired =>
      'Enter both button labels and record text';

  @override
  String get closeCheckinsQuestion => 'Turn off check-ins?';

  @override
  String get closeCheckinsHelp =>
      'Past check-ins and responses will be kept. No further check-ins will be sent.';

  @override
  String get askRegularly => 'Check in with me';

  @override
  String get adjustCheckins => 'Adjust check-ins';

  @override
  String get checkinHelp =>
      'At the scheduled time, a message appears in the main conversation. You confirm the content before it is saved; the habit page is not changed automatically.';

  @override
  String get checkinSyncHelp =>
      'When devices sync, only the owner device creates scheduled check-ins. Other devices display the same message without sending duplicates.';

  @override
  String get inferenceBasis => 'Based on';

  @override
  String get checkinQuestion => 'Question';

  @override
  String get checkinQuestionHint =>
      'For example: Did you go for a walk after lunch?';

  @override
  String get checkinTime => 'Time of day';

  @override
  String get repeat => 'Repeat';

  @override
  String get everyDay => 'Every day';

  @override
  String get checkinDoneFields => 'Done button and record';

  @override
  String get checkinMissedFields => 'Not done button and record';

  @override
  String get walkDoneHint => 'Went for a walk';

  @override
  String get walkDoneTextHint => 'I went for a walk after lunch today';

  @override
  String get walkMissedHint => 'Not yet';

  @override
  String get walkMissedTextHint => 'I have not gone for a walk today';

  @override
  String get checkinRecordHelp =>
      'Keep button labels short. The completed response creates an event using the record text. By default, a missed response stays in chat without creating an event.';

  @override
  String get closeCheckins => 'Turn off check-ins';

  @override
  String get feedbackSuggestion => 'Give feedback on this suggestion';

  @override
  String get feedbackHelp =>
      'Select the original suggestion from the reply. Accepting it records agreement; any action still needs separate confirmation.';

  @override
  String get feedbackOriginal =>
      'Original suggestion (you can select a passage)';

  @override
  String get feedbackRewrite => 'My rewrite (optional)';

  @override
  String get feedbackAccept => 'Accept';

  @override
  String get feedbackIgnore => 'Ignore';

  @override
  String get feedbackSaveRewrite => 'Save rewrite';

  @override
  String get feedbackWithdraw => 'Withdraw feedback';

  @override
  String get feedbackRewritten => 'Rewritten';

  @override
  String get feedbackWithdrawn => 'Withdrawn';

  @override
  String get pageKindProfile => 'Profile';

  @override
  String get pageKindPerson => 'Contact';

  @override
  String get pageKindRecurringCost => 'Recurring cost';

  @override
  String get pageKindCapability => 'Capability';

  @override
  String get pageKindAsset => 'Asset';

  @override
  String get pageKindProject => 'Project';

  @override
  String get pageKindRelationship => 'Relationship';

  @override
  String get pageKindDecision => 'Decision';

  @override
  String get pageKindHabit => 'Habit';

  @override
  String get pageKindConstraint => 'Constraint';

  @override
  String get pageKindInsight => 'Insight';

  @override
  String get pageKindTopic => 'Topic';

  @override
  String get pageKindMethod => 'Method';

  @override
  String get pageKindCase => 'Case';

  @override
  String get pageKindPrinciple => 'Principle';

  @override
  String get pageKindSeries => 'Series';

  @override
  String get pageKindSource => 'Source';

  @override
  String checkinDaysPerWeek(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count days per week',
      one: '1 day per week',
    );
    return '$_temp0';
  }

  @override
  String weekdayShort(String day) {
    String _temp0 = intl.Intl.selectLogic(day, {
      '1': 'Mon',
      '2': 'Tue',
      '3': 'Wed',
      '4': 'Thu',
      '5': 'Fri',
      '6': 'Sat',
      'other': 'Sun',
    });
    return '$_temp0';
  }

  @override
  String get load => 'Load';

  @override
  String get promptTemplateLanguage => 'Template language';

  @override
  String get promptIncompatibleFallback =>
      'This revision is incompatible; using the built-in default';

  @override
  String promptParentFallback(String requested, String resolved) {
    return 'Using parent language: $requested → $resolved';
  }

  @override
  String promptMissingTranslation(String requested, String resolved) {
    return 'Translation unavailable; using built-in default: $requested → $resolved';
  }

  @override
  String get aiResponseLanguage => 'AI reply language';

  @override
  String get aiInstructionLanguage => 'Instruction language';

  @override
  String get languageAutomatic => 'Automatic';

  @override
  String get instructionFollowContext => 'Use default policy';

  @override
  String get languagePolicyHelp =>
      'Automatic replies follow this message or the most recent reliable conversation language. Default instructions follow the interface for chat and the knowledge language for background work. Changes apply to new runs.';

  @override
  String knowledgeLanguageReadonly(String locale) {
    return 'Knowledge output language: $locale (read only)';
  }

  @override
  String runLanguageUi(String locale) {
    return 'Interface language: $locale';
  }

  @override
  String runLanguageInstructions(String locale) {
    return 'Instruction language: $locale';
  }

  @override
  String runLanguageResponse(String locale, String source) {
    return 'Reply language: $locale · $source';
  }

  @override
  String runLanguageKnowledge(String locale) {
    return 'Knowledge output language: $locale';
  }

  @override
  String runLanguageFallback(String reason) {
    return 'Fallback: $reason';
  }

  @override
  String get languageSourceExplicit => 'Explicit request';

  @override
  String get languageSourcePreference => 'Fixed preference';

  @override
  String get languageSourceCurrent => 'Current message';

  @override
  String get languageSourceConversation => 'Conversation language';

  @override
  String get languageSourceFallback => 'Default fallback';

  @override
  String get languageSourceDataset => 'Dataset policy';

  @override
  String get languageSourceLegacy => 'Legacy policy';

  @override
  String get promptContextContract =>
      'Conversation context and profile contract';

  @override
  String get promptToolDescriptionsContract => 'Tool descriptions contract';

  @override
  String get promptToolMetadataContract =>
      'Skill catalog and tool activation contract';

  @override
  String get promptExecutionContract =>
      'Runtime recovery and page processing contract';

  @override
  String get promptProcessingContract =>
      'Background processing and analysis contract';

  @override
  String get promptReplyContract => 'Conversation replies and review status';

  @override
  String get promptKnowledgeProcessingContract =>
      'Knowledge compilation and topic maintenance contract';

  @override
  String get promptLookupContract => 'Local query descriptions';

  @override
  String get promptSupplementContract => 'Task supplements and page references';

  @override
  String get knowledgeLanguage => 'Knowledge language';

  @override
  String get knowledgeLanguageHelp =>
      'New knowledge uses this language. Work already started keeps its original language; existing pages and source text are preserved. Changing this setting does not reprocess the library.';

  @override
  String get knowledgeLanguageUnsupported =>
      'Knowledge generation is paused because this version does not support the dataset’s language policy. Update the app to continue.';

  @override
  String get knowledgeLanguageOwnerOnly =>
      'Only the dataset owner can change this setting.';

  @override
  String get knowledgeLanguageCapabilityRequired =>
      'Connect and update all paired devices before changing the knowledge language.';

  @override
  String get title => 'Title';

  @override
  String get copy => 'Copy';

  @override
  String get todoInputHint => 'Add something to follow up… (Enter to add)';

  @override
  String get todoNoteHint => 'Additional notes (optional)';

  @override
  String get todoEmptyHelp =>
      'No to-dos yet\n\nAdd one here, or ask AI in a conversation to suggest\nfollow-ups for you to review and create.';

  @override
  String todoOpenGroup(int count) {
    return 'Open · $count';
  }

  @override
  String todoDoneGroup(int count) {
    return 'Completed · $count';
  }

  @override
  String get todoEdit => 'Edit to-do';

  @override
  String get todoTitleHint => 'Something to follow up…';

  @override
  String get todoDueHint => 'Due date (optional, YYYY-MM-DD; clear to remove)';

  @override
  String get todoReminderOption =>
      'Remind me when due (banner or notification)';

  @override
  String get todoPriority => 'Priority';

  @override
  String get priorityHigh => 'High';

  @override
  String get priorityNormal => 'Normal';

  @override
  String get priorityLow => 'Low';

  @override
  String todoDueDate(String date) {
    return 'Due $date';
  }

  @override
  String todoReminderDate(String date) {
    return 'Reminder $date';
  }

  @override
  String get todoStatusOpen => 'Open';

  @override
  String get todoStatusArchived => 'Archived';

  @override
  String errorFieldRequired(String field) {
    return '$field is required.';
  }

  @override
  String errorNotFound(String resource) {
    return '$resource could not be found. Refresh and try again.';
  }

  @override
  String get errorSettingsChanged =>
      'Settings have changed. Refresh before editing again.';

  @override
  String get errorSyncUnlockRequired =>
      'Unlock synchronization before changing this setting.';

  @override
  String get errorLanguageUnsupported =>
      'This app version does not support the selected language or policy. Update the app and try again.';

  @override
  String get errorReminderTimePast =>
      'The reminder time has passed. Choose a future time first.';

  @override
  String get errorReminderTimeRequired =>
      'Set a due time before enabling the reminder.';

  @override
  String get errorDetails => 'View details';

  @override
  String get open => 'Open';

  @override
  String get knowledgePage => 'Knowledge page';

  @override
  String get habitReminderSettings => 'Habit reminders';

  @override
  String get habitReminderSettingsHelp =>
      'Manage regular check-ins for active habit pages. Changes to the time, question and buttons are saved to the habit’s plan. Only the owner device sends scheduled check-ins to the main conversation; other devices display the synced message.';

  @override
  String get habitReminderEmpty =>
      'No active habit pages yet. Create a habit page in the knowledge base, then return here to set up check-ins.';

  @override
  String habitReminderCounts(int enabled, int paused, int unset) {
    return 'Active $enabled · Paused $paused · Not set $unset';
  }

  @override
  String get habitReminderActive => 'Active';

  @override
  String get habitReminderUnset => 'Check-ins have not been set up';

  @override
  String get habitReminderSetup => 'Set up';

  @override
  String get habitReminderAdjust => 'Adjust';

  @override
  String get habitReminderResumed => 'Check-ins resumed';

  @override
  String get habitReminderPaused => 'Check-ins paused';

  @override
  String habitWeekSchedule(String days) {
    return 'Weekly: $days';
  }

  @override
  String get habitReminderNotSet => 'Not set';

  @override
  String get trayShowWindow => 'Show main window';

  @override
  String get trayQuit => 'Quit Elsewhen';

  @override
  String get syncKeepAliveDefault =>
      'Syncing with another device. Keep the app running.';

  @override
  String get syncKeepAliveSetup => 'Preparing this device for synchronization';

  @override
  String get syncKeepAlivePairOwner => 'Pairing with the joining device';

  @override
  String get syncKeepAlivePairJoin => 'Pairing with an existing device';

  @override
  String get syncKeepAliveEnrollment =>
      'Completing device enrollment and initial sync';

  @override
  String get syncNotificationTitle => 'Elsewhen is syncing';

  @override
  String get syncNotificationChannel => 'Device synchronization';

  @override
  String get syncNotificationDescription =>
      'Keeps the app running during pairing and initial snapshot transfer';

  @override
  String get syncObjectConflictsTitle => 'To-do and goal sync conflicts';

  @override
  String get syncObjectConflictsHelp =>
      'Review conflicting to-dos, goals, plans, rules or knowledge language policies from another device. Keep the local version or accept the incoming version; fields are not combined.';

  @override
  String get syncObjectConflictsEmpty =>
      'No to-do, goal, plan, rule or knowledge language conflicts to review.';

  @override
  String syncConflictFrom(String kind, String device) {
    return '$kind · Changes from $device';
  }

  @override
  String get syncKindTodo => 'To-do';

  @override
  String get syncKindGoal => 'Goal';

  @override
  String get syncKindPlan => 'Plan';

  @override
  String get syncKindRule => 'Rule';

  @override
  String get syncDeletedReason =>
      'The other device deleted this item while further edits were still pending.';

  @override
  String get syncGoalDeletedReason =>
      'Edits arrived after this goal was deleted on the other device; review is required.';

  @override
  String get syncBaseChangedReason =>
      'Both devices edited this item from an older version.';

  @override
  String get syncGoalLimitReason =>
      'The incoming goal would exceed the active goal limit.';

  @override
  String get syncLanguageChangedReason =>
      'The knowledge language policies have diverged. Review the version signed by the dataset owner.';

  @override
  String get syncLocalDeleted => 'Local (deleted)';

  @override
  String get syncLocalCurrent => 'Current local version';

  @override
  String get syncIncomingVersion => 'Incoming version';

  @override
  String get syncKeepLocal => 'Keep local';

  @override
  String get syncAcceptIncoming => 'Accept incoming';

  @override
  String get syncDeleted => 'Deleted';

  @override
  String get syncEmptyValue => '(empty)';

  @override
  String syncStatusUpdated(String status, String updated) {
    return 'Status: $status · Updated: $updated';
  }

  @override
  String get syncRefresh => 'Reload';

  @override
  String get syncLoadingConflicts => 'Loading conflicts…';

  @override
  String get syncNoActionNeeded => 'No action needed';

  @override
  String syncReviewCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count to review',
      one: '1 to review',
    );
    return '$_temp0';
  }

  @override
  String get errorConflictChanged =>
      'The local item has changed. Refresh and review it again.';

  @override
  String get errorConflictResolved =>
      'This conflict has already been resolved. Refresh the list.';

  @override
  String get errorGoalLimit =>
      'The active goal limit has been reached. Archive a goal or keep the local version before continuing.';

  @override
  String get knowledgeReadingTitle => 'Reading and use';

  @override
  String get knowledgeReadingHelp =>
      'Track your reading and use. This does not change the original text or your endorsement.';

  @override
  String get readingUnread => 'Unread';

  @override
  String get readingRead => 'Read';

  @override
  String get readingValuable => 'Worth using';

  @override
  String get readingAdopted => 'Adopted';

  @override
  String get readingArchived => 'Archived';

  @override
  String libraryAllFilter(String filter) {
    return 'All $filter';
  }

  @override
  String get librarySearchHint => 'Search titles or full text';

  @override
  String get clearSearch => 'Clear search';

  @override
  String get libraryArea => 'Areas';

  @override
  String get libraryImported => 'Sources';

  @override
  String get libraryInsight => 'Knowledge';

  @override
  String get libraryNetwork => 'Contacts and projects';

  @override
  String get libraryKind => 'Types';

  @override
  String get libraryReadingState => 'Reading states';

  @override
  String get tags => 'Tags';

  @override
  String get librarySourceKind => 'Source';

  @override
  String get pageKindNote => 'Note';

  @override
  String get loadMore => 'Load more';

  @override
  String get libraryNoMatch => 'No matching knowledge pages';

  @override
  String get libraryAllShown => 'All results shown';

  @override
  String get knowledgeBatchTitle => 'Review revisions in a batch';

  @override
  String get knowledgeBatchHelp =>
      'Review the content and conditions first. Confirm up to 20 revisions at a time.';

  @override
  String get knowledgeViewRevision => 'View this revision';

  @override
  String knowledgeAcceptCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count revisions',
      one: '1 revision',
    );
    return 'Confirm $_temp0';
  }

  @override
  String get knowledgeRejectSelected => 'Reject selected';

  @override
  String get knowledgeAccepted => 'Accepted';

  @override
  String get knowledgeRejected => 'Rejected';

  @override
  String get priorRejectionTitle => 'You previously rejected this source';

  @override
  String get priorRejectionReason =>
      'You previously discarded this source. Your reason was:';

  @override
  String get priorRejectionHelp =>
      'Importing again reopens this source for review. Continue?';

  @override
  String get saveAnyway => 'Save anyway';

  @override
  String get sourceUpdateTitle => 'Source content has changed';

  @override
  String get sourceUpdateHelp =>
      'Save this as a new version while keeping the old original. Knowledge pages that cite it will be marked for review.';

  @override
  String get sourceSavedVersion => 'Saved version';

  @override
  String get sourceIncomingVersion => 'Fetched version';

  @override
  String get sourceKeepOld => 'Keep the old version';

  @override
  String get sourceSaveNew => 'Save the new version';

  @override
  String get wikiImportUrl => 'Import from URL';

  @override
  String get wikiImportUrlHelp =>
      'Fetch a webpage or post, then review and save it.';

  @override
  String get wikiCreateTopic => 'Create a topic';

  @override
  String get wikiCreateTopicHelp => 'Write content to create a knowledge page.';

  @override
  String get knowledgeReview => 'Knowledge review';

  @override
  String get noPendingWork => 'Nothing pending';

  @override
  String get wikiCloseConfirm => 'Confirm before closing';

  @override
  String get wikiUnsavedChanges =>
      'This page has unsaved edits. Closing it will discard those changes.';

  @override
  String get wikiDiscardClose => 'Discard changes and close';

  @override
  String get wikiSaveClose => 'Save and close';

  @override
  String get wikiCloseTab => 'Close tab';

  @override
  String get wikiPageUnavailable =>
      'This page does not exist or has been deleted.';

  @override
  String get wikiEditTags => 'Edit tags';

  @override
  String get wikiTagsHelp =>
      'Separate tags with spaces or commas. Leave blank to remove all tags.';

  @override
  String get wikiTagsExample => 'For example: work Rust investing';

  @override
  String get wikiEnterUrl => 'Paste a link first.';

  @override
  String get wikiEnterContent => 'Enter some content to save first.';

  @override
  String wikiSavedTitle(String title) {
    return 'Saved “$title” to the knowledge base';
  }

  @override
  String get wikiSave => 'Save to knowledge base';

  @override
  String get wikiUrlHelp =>
      'Paste a webpage or x.com/twitter.com link. Long posts and articles are supported.';

  @override
  String get wikiUrlHint => 'https://x.com/… or https://example.com/…';

  @override
  String get wikiFetching => 'Fetching…';

  @override
  String get wikiFetch => 'Fetch';

  @override
  String get wikiPreviewHelp =>
      'Fetched content opens in a preview tab. Review it, then choose “Save to knowledge base” to keep it.';

  @override
  String get wikiTitleHint =>
      'Title (optional; defaults to the first 60 characters)';

  @override
  String get wikiTagsHint =>
      'Tags (optional; separate with spaces or commas, e.g. work investing)';

  @override
  String get wikiSourceLoading => 'Loading source…';

  @override
  String get wikiSourceRetry => 'Could not load source. Tap to retry.';

  @override
  String get wikiSourceUnavailable => 'The source page is no longer available.';

  @override
  String wikiBasedOn(String title) {
    return 'Based on: $title';
  }

  @override
  String get wikiSaved => 'Saved to knowledge base';

  @override
  String get wikiContent => 'Content';

  @override
  String get wikiOriginal => 'Original';

  @override
  String get wikiOutputs => 'Outputs';

  @override
  String get wikiRelations => 'Related';

  @override
  String get wikiSources => 'Sources';

  @override
  String get wikiSourcesRevisions => 'Sources and revisions';

  @override
  String get wikiPageChangedTitle => 'This page has been updated';

  @override
  String get wikiPageChangedHelp =>
      'This page changed while you were editing.\nReload discards your edits and shows the latest content.\nOverwrite replaces the updated content with your edits. Both versions remain in revision history.';

  @override
  String get wikiReload => 'Reload';

  @override
  String get wikiOverwrite => 'Overwrite';

  @override
  String wikiCollectedOn(String date) {
    return 'Collected $date';
  }

  @override
  String get wikiCheckinHint =>
      'Receive a check-in in the main chat. You confirm its question and buttons first.';

  @override
  String wikiCheckinPaused(String time) {
    return 'Paused · $time';
  }

  @override
  String get wikiCheckins => 'Regular check-ins';

  @override
  String get wikiCopyTitle => 'Copy title';

  @override
  String get wikiTitleCopied => 'Title copied';

  @override
  String get wikiNoEventOrigins => 'No event origins yet';

  @override
  String wikiEventOrigins(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count events',
      one: '1 event',
    );
    return 'Based on $_temp0';
  }

  @override
  String get wikiAddTags => 'Add tags';

  @override
  String get wikiContactRelations => 'Contact relationships';

  @override
  String get wikiTagsUpdated => 'Tags updated';

  @override
  String get wikiImmersive => 'Immersive reading';

  @override
  String get wikiImmersiveHelp => 'Immersive reading (Esc to exit)';

  @override
  String get wikiExitImmersive => 'Exit immersive reading (Esc)';

  @override
  String get wikiReadingSettings => 'Reading settings';

  @override
  String get wikiReadingSettingsHelp =>
      'Reading settings (font, size and line spacing)';

  @override
  String get wikiEditContent => 'Edit content';

  @override
  String get wikiDeletePage => 'Delete page';

  @override
  String get wikiDeleteQuestion => 'Delete this page?';

  @override
  String wikiDeleteHelp(String title) {
    return '“$title” and its revision history will be permanently deleted. Paired devices will also delete this page after syncing. Conflicting offline edits will be kept for review.';
  }

  @override
  String get errorWikiPageChanged =>
      'This page has changed. Reload it or confirm an overwrite.';

  @override
  String get errorWikiMaterialReadonly => 'Source originals are read only.';

  @override
  String get errorWikiContentRequired => 'Content is required.';

  @override
  String get errorWikiContentTooLong =>
      'Content cannot exceed 65,536 characters.';

  @override
  String get wikiDiscussPage => 'Discuss this page with AI';

  @override
  String get wikiAskAi => 'Ask AI';

  @override
  String get wikiProjectPathCopied => 'Project path copied';

  @override
  String get wikiEditProjectPath => 'Change project path';

  @override
  String get wikiChooseProjectDirectory => 'Choose project directory';

  @override
  String get wikiOldPath => 'Previous path';

  @override
  String get wikiNewPath => 'New path';

  @override
  String get wikiProjectPathUpdated => 'Project path updated';

  @override
  String get wikiSourceLinkCopied => 'Source link copied';

  @override
  String get wikiRelatedPages => 'Related pages';

  @override
  String get wikiAddRelatedPage => 'Add a related page';

  @override
  String get wikiNoRelatedCandidates => 'No pages are available to add.';

  @override
  String get wikiTweet => 'Post';

  @override
  String get wikiWebpage => 'Webpage';

  @override
  String get wikiImportTagsHint =>
      'Tags (optional; separate with spaces, e.g. methods investing)';

  @override
  String get wikiHome => 'Home';

  @override
  String wikiPostTab(String id) {
    return 'Post $id';
  }

  @override
  String wikiRelationLabel(String relation) {
    String _temp0 = intl.Intl.selectLogic(relation, {
      'related': 'Related',
      'mentions': 'Mentions',
      'mentionedBy': 'Mentioned by',
      'part_of': 'Part of',
      'contains': 'Contains this page',
      'same_topic': 'Same topic',
      'supports': 'Supports',
      'supportedBy': 'Supported by',
      'contradicts': 'Contradicts',
      'contradictedBy': 'Contradicted by',
      'other': '$relation',
    });
    return '$_temp0';
  }

  @override
  String get tweetDiscuss => 'Discuss this post with AI';

  @override
  String get tweetDiscussHelp =>
      'Discuss it with AI first. Only “Save to knowledge base” saves the content.';

  @override
  String get tweetUnknownAuthor => 'Unknown author';

  @override
  String tweetByAuthor(String author) {
    return 'Post by $author';
  }

  @override
  String get tweetSaved => 'Saved';

  @override
  String get tweetOriginal => 'Fetched content (original)';

  @override
  String get tweetLinkCopied => 'Post link copied';

  @override
  String get tweetFormatting => 'AI is formatting…';

  @override
  String get tweetFormat => 'Format as Markdown with AI';

  @override
  String get tweetFormatAgain => 'Format again';

  @override
  String get tweetDiscardFormat => 'Discard formatting';

  @override
  String get tweetSeparateDraftHelp =>
      'The original is saved; the formatted draft is saved separately.';

  @override
  String get tweetFormatPreview => 'AI formatting (Markdown preview)';

  @override
  String get tweetChatHint =>
      'Ask about this post… (Enter to send, Shift+Enter for a new line)';

  @override
  String get artifactWorks => 'Derived works';

  @override
  String get artifactRefresh => 'Refresh derived works';

  @override
  String get artifactEmpty =>
      'No derived works yet. Ask AI in this page’s chat for a summary, opinion, idea or script. Saved works appear here as versions.';

  @override
  String get artifactNotAdopted => 'Not adopted';

  @override
  String artifactMetadata(String model, String strategy) {
    return 'Model: $model · Process version: $strategy';
  }

  @override
  String get artifactUnknownModel => 'Unknown (not recorded)';

  @override
  String get artifactUnknownStrategy => 'Unknown (legacy work)';

  @override
  String get artifactInstruction => 'Generation instructions';

  @override
  String get artifactNoInstruction =>
      'No generation instructions were recorded for this version.';

  @override
  String get artifactAdoptedRevision => 'View the adopted revision';

  @override
  String get artifactNewerRevisionHelp =>
      'The content has a newer revision. Adoption still refers to the revision you confirmed.';

  @override
  String get artifactOpenPage => 'Open knowledge page';

  @override
  String get artifactCopyBody => 'Copy full text';

  @override
  String get artifactBodyCopied => 'Full text copied';

  @override
  String get artifactCompareSelect => 'Add to comparison';

  @override
  String get artifactUnadopt => 'Undo adoption';

  @override
  String get artifactAdopt => 'Adopt this version';

  @override
  String get artifactMoreVersions => 'Load more versions';

  @override
  String get artifactCompare => 'Compare versions';

  @override
  String artifactType(String type) {
    String _temp0 = intl.Intl.selectLogic(type, {
      'summary': 'Summary',
      'opinion': 'Opinion',
      'idea': 'Idea',
      'script': 'Script',
      'ai_summary': 'AI summary',
      'other': '$type',
    });
    return '$_temp0';
  }

  @override
  String get workItemUpdateStatus => 'Change status';

  @override
  String get workItemUpdatePriority => 'Change priority';

  @override
  String workItemPriority(String priority) {
    String _temp0 = intl.Intl.selectLogic(priority, {
      'high': 'High priority',
      'low': 'Low priority',
      'other': 'Normal priority',
    });
    return '$_temp0';
  }

  @override
  String get workItemChooseDue => 'Choose a due date';

  @override
  String get workItemSetDue => 'Set a due date';

  @override
  String get entityMerged => 'This entity has been merged';

  @override
  String entityMergedInto(String target) {
    return 'This entity was merged into $target';
  }

  @override
  String get entityViewTarget => 'View target';

  @override
  String get entityUndoMerge => 'Undo merge';

  @override
  String get entityMerge => 'Merge entities';

  @override
  String get entityNoMergeCandidates =>
      'No entities of the same type are available to merge.';

  @override
  String get entityChooseTarget => 'Choose the target entity';

  @override
  String get entityMergeInto => 'Merge into';

  @override
  String get entityContinue => 'Continue';

  @override
  String entityMergeQuestion(String title) {
    return 'Merge into “$title”?';
  }

  @override
  String get entityMergeHelp =>
      'Facts, aliases and relationships move to the target entity, with duplicates combined. Original events are preserved. You can undo this from the old entity page if the moved data has not changed.';

  @override
  String get entityConfirmMerge => 'Confirm merge';

  @override
  String entityMergeSaved(String title) {
    return 'Merged into “$title”. Original events are preserved.';
  }

  @override
  String get entityUndoMergeQuestion => 'Undo this merge?';

  @override
  String get entityUndoMergeHelp =>
      'Restore only the facts, aliases and relationships moved by this merge. If they have since changed, the operation is refused to protect newer data. Original events are preserved.';

  @override
  String get entityMergeUndone => 'Merge undone; entity content restored';

  @override
  String get entityRelatedTodos => 'Related to-dos';

  @override
  String get entityOpenWorkItem => 'Open work item';

  @override
  String get entityFacts => 'Structured facts';

  @override
  String get entityFactConflictHelp =>
      'Some facts may conflict. Historical sources are preserved. Review the sources before removing an incorrect derived fact.';

  @override
  String entityFactSource(String date, String confidence, String source) {
    return '$date · Confidence $confidence/5 · View source $source';
  }

  @override
  String get entityRemoveFact => 'Correct by removing this fact';

  @override
  String get entityRemoveFactQuestion => 'Remove this fact?';

  @override
  String get entityRemoveFactHelp =>
      'Only the derived fact on the knowledge page is removed. The original event is preserved.';

  @override
  String get entityRemove => 'Remove';

  @override
  String get entityFactRemoved => 'Fact removed; original event preserved';

  @override
  String get entitySourceMissing => 'The source event no longer exists.';

  @override
  String get entitySourceEvent => 'Source event';

  @override
  String get entityAiSummary => 'AI summary';

  @override
  String get entityEmotionAnnotations =>
      'Emotion annotations (text, no scores)';

  @override
  String entityEmotionAboutEvent(String feeling) {
    return 'About this event: $feeling';
  }

  @override
  String entityEmotionAboutTarget(String target, String feeling) {
    return 'About $target: $feeling';
  }

  @override
  String get entityAiGuidance => 'AI guidance (derived)';

  @override
  String entityRelatedReference(String slug, String note) {
    return 'Related: $slug — $note';
  }

  @override
  String entityAnalysisMetadata(String date, String status) {
    return 'Recorded $date · Analysis status: $status';
  }

  @override
  String entityAnalysisStatus(String status) {
    String _temp0 = intl.Intl.selectLogic(status, {
      'pending': 'Pending',
      'running': 'Running',
      'succeeded': 'Completed',
      'failed': 'Failed',
      'skipped': 'Skipped',
      'other': '$status',
    });
    return '$_temp0';
  }

  @override
  String get entityGuidance => 'Related guidance';

  @override
  String get entityViewGuidance => 'View guidance';

  @override
  String entityGuidanceDetails(String date) {
    return '$date · View key points';
  }

  @override
  String get entityGuidanceHelpful => 'Helpful (tap again to clear)';

  @override
  String get entityGuidanceNotRelevant => 'Not relevant (tap again to clear)';

  @override
  String entityRecordedOn(String date) {
    return 'Recorded $date';
  }

  @override
  String get entityReusableLessons => 'Reusable lessons';

  @override
  String get entityRefinedTodos => 'Refined follow-ups';

  @override
  String get runLanguagePreserveSource =>
      'Output policy: preserve the source language (formatting only)';

  @override
  String get l6ReviewHistoryIsNotAPersonaldee4d =>
      'Review history is not a personal rule';

  @override
  String get l6All778fc => 'All';

  @override
  String get l6RunLoga8ce4 => 'Run log';

  @override
  String get l6NoRunsYet022d0 => 'No runs yet';

  @override
  String get l6ProtectedOnlyEvidenceCanBeAddedba2bf =>
      'Protected (only evidence can be added)';

  @override
  String get l6Savinga032e => 'Saving';

  @override
  String get l6ScanInvitationQrCode4882e => 'Scan invitation QR code';

  @override
  String get l6CameraAccessIsUnavailableCopyAnd95be5 =>
      'Camera access is unavailable. Copy and paste the invitation instead.';

  @override
  String get l6DeviceKeysAreHeldInSystem9ad4e =>
      'Device keys are held in system secure storage on this device and excluded from cloud backup and sync.';

  @override
  String get l6DeviceKeyRecordsAreMissingRecover91b1e =>
      'Device key records are missing. Recover through the original device; replacement keys will not be generated.';

  @override
  String get l6DeviceKeyRecordsAreDamagedUnlocking1284a =>
      'Device key records are damaged. Unlocking has stopped; existing records will not be overwritten.';

  @override
  String get l6SystemSecureStorageIsUnavailableOr5cc06 =>
      'System secure storage is unavailable or locked. Unlock the system key store, then refresh.';

  @override
  String get l6CheckingDeviceKeyStorage3d187 => 'Checking device key storage…';

  @override
  String get l6AuthorizedDevicesb4736 => 'Authorized devices';

  @override
  String get l6RefreshDevices7b112 => 'Refresh devices';

  @override
  String get l6OwnerDevicede929 => 'Owner device';

  @override
  String get l6RunsAiAnalysis6697e => 'Runs AI analysis';

  @override
  String get l6ThisDevice8b4c1 => 'This device';

  @override
  String get l6Revoke9fcef => 'Revoke';

  @override
  String get l6TransferAiAnalysisResponsibilityBeforeRevoking455d7 =>
      'Transfer AI analysis responsibility before revoking access';

  @override
  String get l6RevokeThisDeviceSSyncAccess2a8d0 =>
      'Revoke this device’s sync access?';

  @override
  String get l6KnowledgeSyncConflicts67265 => 'Knowledge sync conflicts';

  @override
  String get l6KnowledgeChangesFromAnotherDeviceConflict5a183 =>
      'Knowledge changes from another device conflict with local changes. Keep the local version or accept the incoming version as a whole.';

  @override
  String get l6NoKnowledgeSyncConflictsNeedReview2246e =>
      'No knowledge sync conflicts need review.';

  @override
  String get l6TheIncomingVersionDeletesThisPagec27d8 =>
      'The incoming version deletes this page, while local changes still need delivery';

  @override
  String get l6BothDevicesRenamedThisPageDifferently200f1 =>
      'Both devices renamed this page differently';

  @override
  String get l6TheIncomingPathConflictsWithAnother49464 =>
      'The incoming path conflicts with another local page';

  @override
  String get l6TheIncomingEditIsBasedOnf0ddf =>
      'The incoming edit is based on an older version of this page';

  @override
  String get l6LocalVersionDeleted25fe7 => 'Local version (deleted)';

  @override
  String get l6ThisPageDoesNotExistLocallya6853 =>
      '(This page does not exist locally)';

  @override
  String get l6Untitledd8f9b => '(Untitled)';

  @override
  String get l6OrganizeTopicscabf2 => 'Organize topics';

  @override
  String get l6MergeSimilarTopicsOrSplitA6542f =>
      'Merge similar topics or split a large topic. Changes take effect after preview and confirmation.';

  @override
  String get l6OldTopicsAreArchivedAfterConfirmationf5e56 =>
      'Old topics are archived after confirmation. Their sources and history remain available. New topics are reference knowledge, not personal rules.';

  @override
  String get l6SearchTopicsToMergef60aa => 'Search topics to merge…';

  @override
  String get l6FindTopicsa638d => 'Find topics';

  @override
  String get l6PreviewMerge6d8f5 => 'Preview merge';

  @override
  String get l6PreviewSplit6f910 => 'Preview split';

  @override
  String get l6AwaitingConfirmation27b58 => 'Awaiting confirmation';

  @override
  String get l6Revoked61063 => 'Revoked';

  @override
  String get l6UndoThisBatch7aa0a => 'Undo this batch';

  @override
  String get l6ConfirmAndArchiveOldTopics37635 =>
      'Confirm and archive old topics';

  @override
  String get l6RejectPlan8b441 => 'Reject plan';

  @override
  String get l6LoadEarlierPlans65e31 => 'Load earlier plans';

  @override
  String get l6RestartTheAppAfterSwitchingToea99d =>
      'Restart the app after switching to view all synced data.';

  @override
  String get l6EnterDatasete3665 => 'Enter dataset';

  @override
  String get l6CouldNotLoadSyncStatusSyncb8a20 =>
      'Could not load sync status\n(Sync may not be enabled yet)';

  @override
  String get l6Processorb4366 => 'Processor';

  @override
  String get l6NoSyncYet8ce44 => 'No sync yet';

  @override
  String get l6HeartbeatOnStandbyIdleda794 => 'Heartbeat on standby (idle)';

  @override
  String get l6Status62e95 => 'Status';

  @override
  String get l6LoadLinkableKnowledgea71cb => 'Load linkable knowledge';

  @override
  String get l6LoadResolutionRevision2bf89 => 'Load resolution revision';

  @override
  String get l6ResolutionSavedf80e5 => 'Resolution saved';

  @override
  String get l6LinkedResolutionsb26bd => 'Linked resolutions';

  @override
  String get l6LinkTheIssueToAKnowledge08e6c =>
      'Link the issue to a knowledge revision covering all evidence';

  @override
  String get l6NoKnowledgePageCoversTheseSources10eed =>
      'No knowledge page covers these sources yet. Organize the disagreement and conditions in a related topic, then return to link it.';

  @override
  String get l6KnowledgePageThatResolvesTheIssueb3d20 =>
      'Knowledge page that resolves the issue';

  @override
  String get l6ReviewRevisionContent73b70 => 'Review revision content';

  @override
  String get l6HowThisRevisionResolvesTheDisagreemented5c3 =>
      'How this revision resolves the disagreement';

  @override
  String get l6ConfirmLinkedResolution5427f => 'Confirm linked resolution';

  @override
  String get l6RefreshLinkableKnowledge5a5b4 => 'Refresh linkable knowledge';

  @override
  String get l6LoadAvailableSourcesefee2 => 'Load available sources';

  @override
  String get l6RepairSourceEvidence7c494 => 'Repair source evidence';

  @override
  String get l6KeepValidSourcesOrChooseReplacementse7069 =>
      'Keep valid sources or choose replacements, then review the knowledge update.';

  @override
  String get l6RejectedOrArchivedSourcesCannotSupport758ff =>
      'Rejected or archived sources cannot support new conclusions. Valid sources stay selected below. Existing knowledge is preserved until you confirm a revision.';

  @override
  String get l6FindReplacementSources05a6a => 'Find replacement sources';

  @override
  String get l6TheFirst50SourcesAreShown7e891 =>
      'The first 50 sources are shown. Search by title for others. Selected sources are preserved.';

  @override
  String get l6UnavailableExcludedFromEvidenceForThe84882 =>
      'Unavailable; excluded from evidence for the new revision';

  @override
  String get l6SourcesForThisKnowledgec088b => 'Sources for this knowledge';

  @override
  String get l6AvailableAsAReplacementSourceda692 =>
      'Available as a replacement source';

  @override
  String get l6ThereIsNotEnoughEvidenceSelect1b56c =>
      'There is not enough evidence. Select or import valid sources. Current knowledge still needs supporting evidence.';

  @override
  String get l6ChooseUpTo8SourcesNarrow5655d =>
      'Choose up to 8 sources. Narrow your selection.';

  @override
  String get l6PrepareRepairProposale29a1 => 'Prepare repair proposal';

  @override
  String get l6RefreshSources59684 => 'Refresh sources';

  @override
  String get l6ReloadSources59500 => 'Reload sources';

  @override
  String get l6LoadReviewHistory6b7da => 'Load review history';

  @override
  String get l6KnowledgeHistory37e16 => 'Knowledge history';

  @override
  String get l6HistoryOfProposalsAndRevisionsAccepted1535c =>
      'History of proposals and revisions (accepted, rejected or linked resolutions)';

  @override
  String get l6ThisPageHasNoRevisionsYet1b848 =>
      'This page has no revisions yet.';

  @override
  String get l6ResolutionNotesca3c9 => 'Resolution notes';

  @override
  String get l6AcceptedChanges114f5 => 'Accepted changes';

  @override
  String get l6SuggestedConditions5bf71 => 'Suggested conditions';

  @override
  String get l6SavedConditions0ae9a => 'Saved conditions';

  @override
  String get l6LinkedRevision4f2da => 'Linked revision';

  @override
  String get l6PreviousContent5b344 => 'Previous content';

  @override
  String get l6SavedContentc80ef => 'Saved content';

  @override
  String get l6Reload77849 => 'Reload';

  @override
  String get l6LoadMoreRecords37269 => 'Load more records';

  @override
  String get l6Waiting25d23 => 'Waiting';

  @override
  String get l6Skipped9f38a => 'Skipped';

  @override
  String get l6ProcessingQueueb0388 => 'Processing queue';

  @override
  String get l6ViewPendingWorkReadingProgressAnd0ed68 =>
      'View pending work, reading progress and wait reasons';

  @override
  String get l6NoTasksInThisState7922e => 'No tasks in this state.';

  @override
  String get l6ReloadQueueb9f40 => 'Reload queue';

  @override
  String get l6LoadMoreTasks1db39 => 'Load more tasks';

  @override
  String get l6PrepareSourceRepair2b5a2 => 'Prepare source repair';

  @override
  String get l6ThereIsNotEnoughEvidenceAddfdb09 =>
      'There is not enough evidence. Add valid sources first. Current knowledge is preserved while awaiting evidence.';

  @override
  String get l6TheSelectedSourcesAreBeingReadb3e01 =>
      'The selected sources are being read in the background. Prepare a repair proposal once reading finishes.';

  @override
  String get l6UpstreamKnowledgeae238 => 'Upstream knowledge';

  @override
  String get l6UpstreamKnowledgeNeedsReviewOpenItsb9c0d =>
      'Upstream knowledge needs review. Open its page to check the changes first.';

  @override
  String get l6Actionf3ea6 => 'Action';

  @override
  String get l6PageDoesNotExist55c9e => 'Page does not exist';

  @override
  String get l6SourceRecords5bc8d => 'Source records';

  @override
  String get l6SourceRecordDoesNotExist25551 => 'Source record does not exist';

  @override
  String get l6CouldNotLoadd9f60 => 'Could not load';

  @override
  String get l6LoadSourceRecordse26e1 => 'Load source records';

  @override
  String get l6DecideOnThisSource4e2fd => 'Decide on this source';

  @override
  String get l6DiscardAsRejectedKnowledge14322 =>
      'Discard as rejected knowledge';

  @override
  String get l6RequiredReasonForDiscarding0dc5c =>
      'Required: reason for discarding';

  @override
  String get l6OrganizeKnowledge210a3 => 'Organize knowledge';

  @override
  String get l6ExtractOnDemandSaveNewReference28c44 =>
      'Extract on demand; save new reference knowledge directly';

  @override
  String get l6ExtractAMethod9c1f5 => 'Extract a method';

  @override
  String get l6ReusableStepsAndConditions4c52e =>
      'Reusable steps and conditions';

  @override
  String get l6OrganizeACaseac694 => 'Organize a case';

  @override
  String get l6ContextApproachAndOutcomeee83d =>
      'Context, approach and outcome';

  @override
  String get l6ExtractAPrinciple14a19 => 'Extract a principle';

  @override
  String get l6PrinciplesEvidenceAndLimits440fe =>
      'Principles, evidence and limits';

  @override
  String get l6ExtractKnowledge196cf => 'Extract knowledge';

  @override
  String get l6IncludeInKnowledge29c9c => 'Include in knowledge';

  @override
  String get l6IncludedInKnowledge9a862 => 'Included in knowledge';

  @override
  String get l6DiscardedRejectedKnowledge57399 =>
      'Discarded (rejected knowledge)';

  @override
  String get l6ReferenceKnowledgeIsOrganizedAutomaticallyYoucefad =>
      'Reference knowledge is organized automatically. You confirm human edits and personal rules.';

  @override
  String get l6LoadProposal021f2 => 'Load proposal';

  @override
  String get l6NoChangesNeedYourConfirmationNew743fb =>
      'No changes need your confirmation. New reference knowledge is saved directly in outputs.';

  @override
  String get l6SourceAndContentChecks5517e => 'Source and content checks';

  @override
  String get l6LoadCheckse2e22 => 'Load checks';

  @override
  String get l6NoChecksNeedReview4053b => 'No checks need review.';

  @override
  String get l6FailedAutomaticRetryScheduledc9e00 =>
      'Failed; automatic retry scheduled';

  @override
  String get l6SupportingEvidenceabba3 => 'Supporting evidence';

  @override
  String get l6KnowledgeCitedInThisAnswer4ad6e =>
      'Knowledge cited in this answer';

  @override
  String get l6ThisAnswerHasNoSystemVerified0ba58 =>
      'This answer has no system-verified knowledge citations.';

  @override
  String get l6SourceVersionDoesNotExist8a671 =>
      'Source version does not exist';

  @override
  String get l6ViewSourceEvent3c3f9 => 'View source event';

  @override
  String get l6BackgroundProcessingOrganizesReferenceKnowledgeAnd36787 =>
      'Background processing organizes reference knowledge and shared topics. New extracted outputs are saved directly. Changes to edited or confirmed content need review. Originals are preserved; you confirm personal rules.';

  @override
  String get l6SuggestionsToReview822f0 => 'Suggestions to review';

  @override
  String get l6CompiledReferenceKnowledge9efb1 =>
      'Compiled reference knowledge';

  @override
  String get l6NoKnowledgeOutputsYetYouCan01967 =>
      'No knowledge outputs yet. You can use the original text. Background processing decides whether it supports new knowledge.';

  @override
  String get l6LoadChanges6cd73 => 'Load changes';

  @override
  String get l6ReviewProposal8e384 => 'Review proposal';

  @override
  String get l6Handled219bd => 'Handled';

  @override
  String get l6CompareWithCurrentContenta3c20 => 'Compare with current content';

  @override
  String get l6CurrentContent47be3 => 'Current content';

  @override
  String get l6LoadPageContent3f16e => 'Load page content';

  @override
  String get l6ReviewParagraphByParagraph072ad =>
      'Review paragraph by paragraph';

  @override
  String get l6OriginalParagraphe62c3 => 'Original paragraph';

  @override
  String get l6SuggestedParagraphcd73c => 'Suggested paragraph';

  @override
  String get l6AlsoAcceptTheSuggestedConditionsbd478 =>
      'Also accept the suggested conditions';

  @override
  String get l6WhichIssuesThisRevisionResolvesYou7c3a0 =>
      'Which issues this revision resolves (you confirm):';

  @override
  String get l6ChangesToSourceEvidenceRequireFull6139c =>
      'Changes to source evidence require full confirmation. Partial acceptance is available when sources stay the same.';

  @override
  String get l6ViewSourceMaterialdf8d0 => 'View source material';

  @override
  String get l6LoadSourceSnapshot1cba8 => 'Load source snapshot';

  @override
  String get l6ViewEventfdcb3 => 'View event';

  @override
  String get l6HideComparisond3329 => 'Hide comparison';

  @override
  String get l6CompareWithTheCurrentContentConfirming9fbf7 =>
      'Compare with the current content. Confirming applies these changes. For a personal rule, it also confirms the revised rule and its conditions.';

  @override
  String get l6ConfirmRevision97031 => 'Confirm revision';

  @override
  String get l6RejectSuggestionb7fe0 => 'Reject suggestion';

  @override
  String get l6HandledOriginalTextPreserved3bc8e =>
      'Handled; original text preserved';

  @override
  String get l6RejectedDecisionPreserved97f65 => 'Rejected; decision preserved';

  @override
  String get l6DismissHint0e8a6 => 'Dismiss hint';

  @override
  String get l6OperationFailed09e42 => 'Operation failed';

  @override
  String get l6DismissCheck617f7 => 'Dismiss check';

  @override
  String get l6LoadRevisions0cbe1 => 'Load revisions';

  @override
  String get l6GenerateRestorationProposal29825 =>
      'Generate restoration proposal';

  @override
  String get l6TheRestorationProposalIsInOutputsde198 =>
      'The restoration proposal is in Outputs. It takes effect after confirmation. Current sources and rule settings are preserved.';

  @override
  String get l6PrepareRestoration94c4c => 'Prepare restoration';

  @override
  String get l6SourceAssessment7740d => 'Source assessment';

  @override
  String get l6Endorse8a231 => 'Endorse';

  @override
  String get l6Reject802d6 => 'Reject';

  @override
  String get l6UseThisAsAPersonalRulefc28c => 'Use this as a personal rule?';

  @override
  String get l6AiWillConsultThisRuleWhenf912f =>
      'AI will consult this rule when its conditions apply. Confirm that the content is reliable and the conditions are accurate.';

  @override
  String get l6ConfirmUpgrade8c3fd => 'Confirm upgrade';

  @override
  String get l6SaveMetadata757dd => 'Save metadata';

  @override
  String get l6Conditions24481 => 'Conditions';

  @override
  String get l6ReferenceMaterialc3d33 => 'Reference material';

  @override
  String get l6ReusableMethod12837 => 'Reusable method';

  @override
  String get l6ConfirmedRuleba4b1 => 'Confirmed rule';

  @override
  String get l6SaveConditionsAndStrength46f33 => 'Save conditions and strength';

  @override
  String get l6SourcesLetYouVerifyEvidenceRevisionsccae6 =>
      'Sources let you verify evidence. Revisions show how knowledge changed. Normal reading and AI search need no action on each entry.';

  @override
  String get l6UpstreamBasis0fd54 => 'Upstream basis';

  @override
  String get l6NoVerifiableSourcesYet8b47f => 'No verifiable sources yet.';

  @override
  String get l6SourceEvents2c4b3 => 'Source events';

  @override
  String get l6OriginalSourceVersions24704 => 'Original source versions';

  @override
  String get l6KnowledgeRevisions42307 => 'Knowledge revisions';

  @override
  String get l6ViewAndRestorePastRevisionsOfd9312 =>
      'View and restore past revisions of this page';

  @override
  String get l6SourceIsTemporarilyUnavailablecb78e =>
      'Source is temporarily unavailable';

  @override
  String get l6DeviceInformation67298 => 'Device information';

  @override
  String get l6Rename1cd80 => 'Rename';

  @override
  String get l6DeviceId464c0 => 'Device ID:';

  @override
  String get l6RenameThisDevice304fd => 'Rename this device';

  @override
  String get l6DeviceNamed889e => 'Device name';

  @override
  String get l6RelayServiceUnlockDeviceKeysTo0103f =>
      'Relay service: unlock device keys to view';

  @override
  String get l6RelayServiceda32a => 'Relay service:';

  @override
  String get l6CopyRelayAddress61683 => 'Copy relay address';

  @override
  String get l6RelayAddressCopied7db6a => 'Relay address copied';

  @override
  String get l6Restore79748 => 'Restore';

  @override
  String get l6TransferAiAnalysisResponsibilityfe457 =>
      'Transfer AI analysis responsibility';

  @override
  String get l6ThisDeviceIsNotInvolvedInaf62a =>
      'This device is not involved in a handoff.';

  @override
  String get l6ResponsibilityHasBeenHandedOffWaitingeac50 =>
      'Responsibility has been handed off. Waiting for the target device to accept.';

  @override
  String get l6AHandoffAwaitsThisDeviceSdd081 =>
      'A handoff awaits this device’s confirmation; sync is up to date.';

  @override
  String get l6AHandoffAwaitsConfirmationButThis0f114 =>
      'A handoff awaits confirmation, but this device has not reached the handoff boundary.';

  @override
  String get l6ThisDeviceRunsAiAnalysisThe54d4b =>
      'This device runs AI analysis (the previous handoff completed).';

  @override
  String get l6AcceptResponsibility6c220 => 'Accept responsibility';

  @override
  String get l6NoEligibleTargetDevicesAuthorizeAaba52 =>
      'No eligible target devices. Authorize a device to join first.';

  @override
  String get l6TargetDevice1b73f => 'Target device';

  @override
  String get l6TransferAiAnalysisResponsibility2e5ba =>
      'Transfer AI analysis responsibility';

  @override
  String get l6WaitingForTheOwnerToApprovebf73f =>
      'Waiting for the owner to approve handoff proof.';

  @override
  String get l6ProcessingHandoffed58d => 'Processing handoff…';

  @override
  String get l6TransferAiAnalysisResponsibility5d2ad =>
      'Transfer AI analysis responsibility?';

  @override
  String get l6Transferc79cc => 'Transfer';

  @override
  String get l6JoinWithAnInvitation78bca => 'Join with an invitation';

  @override
  String get l6InvitationFromATrustedDeviceade9c =>
      'Invitation from a trusted device';

  @override
  String get l6PreviewInvitationOfflinee39bc => 'Preview invitation (offline)';

  @override
  String get l6JoinAnExistingDatasetb3b50 => 'Join an existing dataset';

  @override
  String get l6PreviewAndVerifyInvitation1afde =>
      'Preview and verify invitation';

  @override
  String get l6SendJoinRequestd5d20 => 'Send join request';

  @override
  String get l6WaitingForOwnerApproval9e900 => 'Waiting for owner approval';

  @override
  String get l6ConfirmInstallation555cc => 'Confirm installation';

  @override
  String get l6WaitingForAuthorizationAndInitialSync6cedf =>
      'Waiting for authorization and initial sync';

  @override
  String get l6ReceivingTheInitialSnapshot032cf =>
      'Receiving the initial snapshot';

  @override
  String get l6RelayMailboxFullDeliveryResumesAutomatically4a933 =>
      'Relay mailbox full. Delivery resumes automatically as the peer collects messages.';

  @override
  String get l6ScanQrCodeb8a8f => 'Scan QR code';

  @override
  String get l6RelayToConnectTo06ef4 => 'Relay to connect to';

  @override
  String get l6InvitingDeviceSPublicKeyFingerprint283a5 =>
      'Inviting device’s public key fingerprint';

  @override
  String get l6ThisDeviceSPublicKeyFingerprintb7e3a =>
      'This device’s public key fingerprint (verify on an existing device)';

  @override
  String get l6ReviewTheInvitationThenExplicitlyChoosece487 =>
      'Review the invitation, then explicitly choose to join.';

  @override
  String get l6PreparingAndVerifyingProtectedDeviceKeys60dc8 =>
      'Preparing and verifying protected device keys.';

  @override
  String get l6DeviceKeysAreReadyRequestDeliverye8427 =>
      'Device keys are ready. Request delivery is not confirmed yet.';

  @override
  String get l6WaitingForAnExistingDeviceTo330c1 =>
      'Waiting for an existing device to verify the fingerprint and approve.';

  @override
  String get l6WaitingForSystemConfirmationNoProof188b0 =>
      'Waiting for system confirmation. No proof is sent before confirmation.';

  @override
  String get l6InstallationIsSavedProofDeliveryIsf28de =>
      'Installation is saved. Proof delivery is not confirmed yet.';

  @override
  String get l6InstallationProofDeliveredWaitingForAuthorization5d65b =>
      'Installation proof delivered; waiting for authorization and initial sync.';

  @override
  String get l6CoordinatingTheApprovedHandoffAutomaticallyCheck9b863 =>
      'Coordinating the approved handoff automatically. Check its progress below.';

  @override
  String get l6ConfirmingHandoffRevisionsSendingResumesAftera5068 =>
      'Confirming handoff revisions. Sending resumes after confirmation.';

  @override
  String get l6TheInvitationExpiredRequestANewf2a42 =>
      'The invitation expired. Request a new one.';

  @override
  String get l6JoiningRequiresANewInvitationOr2a99d =>
      'Joining requires a new invitation or recovery.';

  @override
  String get l6NewDeviceNamed42db => 'New device name';

  @override
  String get l6IHaveCheckedTheInvitationAnd15ccd =>
      'I have checked the invitation and relay, and agree to connect and receive all Provider settings and API keys.';

  @override
  String get l6RequestToJoinUsingASeparate835e5 =>
      'Request to join using a separate staging database';

  @override
  String get l6ResendOriginalJoinRequest97725 => 'Resend original join request';

  @override
  String get l6CheckForApproval58c9a => 'Check for approval';

  @override
  String get l6ResendSavedInstallationProof3d80a =>
      'Resend saved installation proof';

  @override
  String get l6StopThisDeviceSEnrollmentb3a98 =>
      'Stop this device’s enrollment';

  @override
  String get l6ApprovalCleanupIsNotConfirmedResending9fd47 =>
      'Approval cleanup is not confirmed. Resending proof checks cleanup again without reinstalling.';

  @override
  String get l6ViewRecoverableEnrollmentStagingEntriesf095c =>
      'View recoverable enrollment staging entries';

  @override
  String get l6UnlockAndRecovere9f7c => 'Unlock and recover';

  @override
  String get l6DeleteThisStagingEntrya06bf => 'Delete this staging entry?';

  @override
  String get l6TheCurrentStorageBackendDoesNot2cb78 =>
      'The current storage backend does not support device sync.';

  @override
  String get l6SyncIsEnabled210fe => 'Sync is enabled';

  @override
  String get l6SyncIsPausedThisDeviceS7fecc =>
      'Sync is paused (this device’s key is locked)';

  @override
  String get l6SyncIsDisabledThisDeviceIs6aad5 =>
      'Sync is disabled (this device is not authorized)';

  @override
  String get l6SyncNeedsRepair8e4fa => 'Sync needs repair';

  @override
  String get l6SyncIsNotEnabledbccba => 'Sync is not enabled';

  @override
  String get l6LoadingSyncStatus861b0 => 'Loading sync status';

  @override
  String get l6SyncStatusIsUnavailable0971a => 'Sync status is unavailable';

  @override
  String get l6OwnerDeviceRunsBackgroundAiAnalysis73dc6 =>
      'Owner device · runs background AI analysis';

  @override
  String get l6MemberDeviceRunsBackgroundAiAnalysisf055d =>
      'Member device · runs background AI analysis';

  @override
  String get l6MemberDevicee071b => 'Member device';

  @override
  String get l6Pairing6aed7 => 'Pairing';

  @override
  String get l6Manage4989b => 'Manage';

  @override
  String get l6Clearingf54a6 => 'Clearing…';

  @override
  String get l6ClearThisDeviceSSyncIdentityab953 =>
      'Clear this device’s sync identity';

  @override
  String get l6SavedProviderApiKeysArePreservedb2ce9 =>
      '• Saved Provider API keys are preserved and migrated after pairing again.';

  @override
  String get l6SavedProviderApiKeysWillStop714a6 =>
      '• Saved Provider API keys will stop working and must be entered again.';

  @override
  String get l6IUnderstandThatThisDeviceMust3caf2 =>
      'I understand that this device must pair again to sync.';

  @override
  String get l6IUnderstandThatProviderApiKeys4b028 =>
      'I understand that Provider API keys must be entered again.';

  @override
  String get l6ClearThisDeviceSSyncIdentityf7c83 =>
      'Clear this device’s sync identity?';

  @override
  String get l6AfterClearing12369 => 'After clearing:';

  @override
  String get l6ThisDeviceLeavesTheDatasetAnd73b97 =>
      '• This device leaves the dataset and must pair or join again to sync.';

  @override
  String get l6ChangesNotYetSyncedToOther8c274 =>
      '• Changes not yet synced to other devices are not sent automatically.';

  @override
  String get l6NotesConversationsAndKnowledgeArePreserved9b3e9 =>
      'Notes, conversations and knowledge are preserved.';

  @override
  String get l6ClearAndPairAgain845d1 => 'Clear and pair again';

  @override
  String get l6DeviceName4433e => 'Device name';

  @override
  String get l6PrepareThisDevice9d671 => 'Prepare this device';

  @override
  String get l6PrepareThisDeviceForSyncee24c => 'Prepare this device for sync?';

  @override
  String get l6IUnderstandThatAuthorizedDevicesWill1e411 =>
      'I understand that authorized devices will receive all Provider settings and API keys.';

  @override
  String get l6IUnderstandPrepareThisDevice9598b =>
      'I understand; prepare this device';

  @override
  String get l6DeviceSyncIsDisabledInThise1b42 =>
      'Device sync is disabled in this build (ELSEWHEN_SYNC_PREVIEW).';

  @override
  String get l6AfterPairingDeviceListsAndSync6d4f5 =>
      'After pairing, device lists and sync management appear here.';

  @override
  String get l6DeviceIdIsUnavailableRefreshAnd762d2 =>
      'Device ID is unavailable. Refresh and try again.';

  @override
  String get l6AuthorizedDevicesAppearHereAfterThey67f15 =>
      'Authorized devices appear here after they join.';

  @override
  String get l6LockDeviceKeys6ac4e => 'Lock device keys';

  @override
  String get l6UnlockDeviceKeysa84f8 => 'Unlock device keys';

  @override
  String get l6RefreshStatus7cc7f => 'Refresh status';

  @override
  String get l6Diagnostics0b673 => 'Diagnostics';

  @override
  String get l6DatasetId395bc => 'Dataset ID';

  @override
  String get l6ConfirmDeviceEnrollment159c2 => 'Confirm device enrollment';

  @override
  String get l6CompareThePublicKeyFingerprintWitha360c =>
      'Compare the public key fingerprint with the new device:';

  @override
  String get l6ConfirmThatYouTrustThisDevice20292 =>
      'Confirm that you trust this device and allow it to receive all Provider settings and API keys.';

  @override
  String get l6Back11d02 => 'Back';

  @override
  String get l6ApproveThisDeviced11af => 'Approve this device';

  @override
  String get l6InviteADevice9f3fa => 'Invite a device';

  @override
  String get l6NewDevicesCanRequestToJoinbe25a =>
      'New devices can request to join an existing dataset. Handoff proceeds automatically after approval.';

  @override
  String get l6YourRelayUrl9ff3a => 'Your relay URL';

  @override
  String get l6IUnderstandThatCreatingAnInvitation5f3db =>
      'I understand that creating an invitation connects to this relay, and approving a device shares Provider settings and API keys.';

  @override
  String get l6CreateATemporaryInvitationa2809 =>
      'Create a temporary invitation';

  @override
  String get l6InvitationCreatedShowTheQrCode15986 =>
      'Invitation created (show the QR code or copy it)';

  @override
  String get l6ANewDeviceHasRequestedTo3129b =>
      'A new device has requested to join';

  @override
  String get l6ApprovedDeliveringApprovalc4bcf =>
      'Approved; delivering approval';

  @override
  String get l6ApprovalDeliveredWaitingForTheNewaeaa7 =>
      'Approval delivered; waiting for the new device to finish installation';

  @override
  String get l6NewDeviceJoinsCheckTheDevice29220 =>
      'New device joins (check the device list in Manage)';

  @override
  String get l6PreparingTheInitialSnapshotb1613 =>
      'Preparing the initial snapshot';

  @override
  String get l6WaitingForInitialSnapshotSelectione32c8 =>
      'Waiting for initial snapshot selection';

  @override
  String get l6SendingTheInitialSnapshotfa3fd => 'Sending the initial snapshot';

  @override
  String get l6ShareInvitationsOnlyWithTrustedDevices47059 =>
      'Share invitations only with trusted devices.';

  @override
  String get l6CopyInvitationf7293 => 'Copy invitation';

  @override
  String get l6CancelInvitation17e4c => 'Cancel invitation';

  @override
  String get l6VerifyAndApprove5c161 => 'Verify and approve';

  @override
  String get l6ResendSavedApproval48def => 'Resend saved approval';

  @override
  String get l6ProviderSyncConflicts31e02 => 'Provider sync conflicts';

  @override
  String get l6KeysAreMaskedKeepTheLocal2e55f =>
      'Keys are masked. Keep the local configuration or accept the remote configuration as a whole. Address hints do not confirm connectivity.';

  @override
  String get l6NoProviderSyncConflictsNeedReview4dc86 =>
      'No Provider sync conflicts need review.';

  @override
  String get l6CurrentLocalConfiguration683a0 => 'Current local configuration';

  @override
  String get l6RemoteConfiguration04e8c => 'Remote configuration';

  @override
  String get l6KeepLocal6d6a5 => 'Keep local';

  @override
  String get l6AcceptRemote4546a => 'Accept remote';

  @override
  String get l6NoConfigurationa33dc => '(No configuration)';

  @override
  String get l6KeyNotSetefd96 => 'Key: not set';

  @override
  String get l6LoopbackAddressReachableOnlyFromThis5a7ca =>
      'Loopback address: reachable only from this device';

  @override
  String get l6PrivateNetworkAddressReachableOnlyOnf9ec8 =>
      'Private network address: reachable only on the same network';

  @override
  String get l6MissingHttpSPrefixMostClients85171 =>
      'Missing http(s):// prefix; most clients cannot use this address';

  @override
  String get l6UnusualProtocolMayNotBeSupportedf9ae0 =>
      'Unusual protocol; may not be supported';

  @override
  String get l6MissingHostNameb705c => 'Missing host name';

  @override
  String get l6LastSyncSkippedLockedOrBackground113af =>
      'Last sync: skipped (locked or background busy)';

  @override
  String get l6RelayMailboxFullWaitingForThec480e =>
      'Relay mailbox full; waiting for the peer to collect messages';

  @override
  String get l6RelayMessageFormatMismatchUpdateThe1d3c3 =>
      'Relay message format mismatch; update the app';

  @override
  String get l6RepeatedContentValidationFailuresDeliveryPausedb04cc =>
      'Repeated content validation failures; delivery paused';

  @override
  String get l6NetworkIsUnstableRetrying2aaa3 =>
      'Network is unstable; retrying';

  @override
  String get l6Resyncing0f8da => 'Resyncing';

  @override
  String get l6DeliveredToRelayWaitingForPeer2bf3e =>
      'Delivered to relay; waiting for peer confirmation';

  @override
  String get l6Idle837e7 => 'Idle';

  @override
  String get l6RetryingSoone5ae3 => 'Retrying soon';

  @override
  String get l6DeleteEmotionAnnotationa51b5 => 'Delete emotion annotation';

  @override
  String get l6DeleteRelationship16bbd => 'Delete relationship';

  @override
  String get l6NoRelationshipsOrEmotionAnnotationsYetbecb5 =>
      'No relationships or emotion annotations yet. People and projects mentioned in conversations or quick capture appear after confirmation or explicit @/# tagging. Emotion annotations are brief text without scores. Deleting an entry preserves the original record.';

  @override
  String l6LastArg0187f4(String arg0) {
    return 'Last $arg0';
  }

  @override
  String get l6DeleteThisRelationshipPreserveOriginalRecord6cc81 =>
      'Delete this relationship (preserve original record)';

  @override
  String get l6EmotionAnnotationsab8ab => 'Emotion annotations';

  @override
  String get l6AboutThisEventbc042 => 'About this event';

  @override
  String l6AboutArg0171b0(String arg0) {
    return 'About $arg0';
  }

  @override
  String get l6DeleteThisEmotionAnnotationPreserveOriginal8d148 =>
      'Delete this emotion annotation (preserve original record)';

  @override
  String get l6OtherDimensionsCommitmentsReflectionPaceBoundaries873e8 =>
      'Other dimensions—commitments, reflection, pace, boundaries, health, finances, location, identity and creative work—live on knowledge pages that you can view and edit.';

  @override
  String get l6EventRemindersaa957 => 'Event reminders';

  @override
  String get l6ImmediateAndInReviewsd066b => 'Immediate and in reviews';

  @override
  String get l6ReviewsOnly71ead => 'Reviews only';

  @override
  String get l6SaveReminderPreferencese0188 => 'Save reminder preferences';

  @override
  String get l6EmotionalCheckIns74925 => 'Emotional check-ins';

  @override
  String get l63Daysb30a9 => '3 days';

  @override
  String get l67Daysecfe0 => '7 days';

  @override
  String get l614Days1e982 => '14 days';

  @override
  String get l630Daysff788 => '30 days';

  @override
  String get l6SaveRevisitPreferences8123a => 'Save revisit preferences';

  @override
  String get l6RecordReminderFeedbackcfbf1 => 'Record reminder feedback';

  @override
  String get l6Assessingd7b92 => 'Assessing';

  @override
  String get l6FollowedInConversationa3d02 => 'Followed in conversation';

  @override
  String get l6NotApplicableeec46 => 'Not applicable';

  @override
  String get l6RuleContextsInConversationsEvaluatedSilently95c0c =>
      'Rule contexts in conversations (evaluated silently in the background)';

  @override
  String get l6RecentReminderse3fba => 'Recent reminders';

  @override
  String l6HelpfulArg0NotRelevantArg18e65c(String arg0, String arg1) {
    return 'Helpful $arg0 · Not relevant $arg1';
  }

  @override
  String get l6TriggerConditionsf5c95 => 'Trigger conditions';

  @override
  String get l6TriggerKeywordsCommaSeparatedBlankDisablesdae9a =>
      'Trigger keywords (comma separated; blank disables prefiltering)';

  @override
  String get l6ForExampleProjectXDeadlinef216a =>
      'For example: project X, deadline';

  @override
  String get l6TriggerContext02570 => 'Trigger context';

  @override
  String get l6AllContexts39fdd => 'All contexts';

  @override
  String get l6QuickCapturecfe6c => 'Quick capture';

  @override
  String get l6DailyReview6bea3 => 'Daily review';

  @override
  String get l6SaveTriggerConditions8e04c => 'Save trigger conditions';

  @override
  String get l6DeleteRulef9ad3 => 'Delete rule';

  @override
  String get l6PersonalRules03b44 => 'Personal rules';

  @override
  String get l6UserModel88c56 => 'User model';

  @override
  String get l6NoRulesYetWhenYouDiscuss96049 =>
      'No rules yet. When you discuss lessons or experiences, AI may suggest a rule. Accepted rules appear here and can be consulted in similar situations.';

  @override
  String get l6TriggerConditionsConfigureda3704 =>
      'Trigger conditions (configured)';

  @override
  String get l6ConfigureTriggerConditions168ff =>
      'Configure trigger conditions';

  @override
  String get l6PlaceEachSkillFolderWithIts94f37 =>
      'Place each skill folder with its SKILL.md in <data directory>/skills. AI loads skills as needed and asks before running scripts. Marking a skill as trusted lets its scripts run without approval. Scripts can perform arbitrary local operations; enable trust only when you understand and accept this behavior.';

  @override
  String get l6InstallFromFolder22a87 => 'Install from folder';

  @override
  String get l6InstallFromZip906f1 => 'Install from ZIP';

  @override
  String get l6InstallFromGit0882e => 'Install from Git';

  @override
  String get l6OpenSkillsFoldera4437 => 'Open skills folder';

  @override
  String l6SelectedArg0Arg1fd75c(String arg0, String arg1) {
    return 'Selected $arg0 / $arg1';
  }

  @override
  String l6TotalArg01e668(String arg0) {
    return 'Total: $arg0';
  }

  @override
  String get l6ExitSelectione23ed => 'Exit selection';

  @override
  String get l6DeleteMultiple362ae => 'Delete multiple';

  @override
  String get l6DeselectAllba20e => 'Deselect all';

  @override
  String get l6SelectAll3e44b => 'Select all';

  @override
  String l6DeleteSelectedArg0def95(String arg0) {
    return 'Delete selected ($arg0)';
  }

  @override
  String get l6LoadingSkills8d716 => 'Loading skills…';

  @override
  String get l6NoUserSkillsInstalledInstallAe8aea =>
      'No user skills installed\nInstall a skill from a folder, ZIP or Git repository';

  @override
  String get l6LoadUserSkillsfb79f => 'Load user skills';

  @override
  String get l6ChooseASkillZipFile0e485 => 'Choose a skill ZIP file';

  @override
  String get l6ChooseASkillFolderContainingSkill8789f =>
      'Choose a skill folder containing SKILL.md';

  @override
  String get l6InstallFromZip37d9a => 'Install from ZIP';

  @override
  String get l6InstallFromFolder71cd7 => 'Install from folder';

  @override
  String get l6InstallASkillFromGitfa921 => 'Install a skill from Git';

  @override
  String get l6RepositoryUrlHttpsOrLocalPath3ae69 =>
      'Repository URL (HTTPS or local path)';

  @override
  String get l6Install087db => 'Install';

  @override
  String get l6InstallFromGit6d266 => 'Install from Git';

  @override
  String l6InstalledArg0EnabledByDefaultdd2c2(String arg0) {
    return 'Installed $arg0; enabled by default';
  }

  @override
  String get l6UninstallSkill053a6 => 'Uninstall skill';

  @override
  String l6Arg0WillBeRemovedFromThis1a7c6(String arg0) {
    return '$arg0 will be removed from this device and, if synced, other devices.';
  }

  @override
  String get l6Uninstall200cf => 'Uninstall';

  @override
  String get l6UninstallMultipleSkills13309 => 'Uninstall multiple skills';

  @override
  String l6SkillsToRemoveArg0SyncedCopiesaa41a(String arg0) {
    return 'Skills to remove: $arg0. Synced copies will also be removed from other devices.';
  }

  @override
  String l6UninstallSkillArg0a3819(String arg0) {
    return 'Uninstall skill $arg0';
  }

  @override
  String l6SkillsUninstalledArg08db74(String arg0) {
    return 'Skills uninstalled: $arg0';
  }

  @override
  String l6SucceededArg0FailedArg1Arg2Arg3a2cde(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return 'Succeeded: $arg0; failed: $arg1: $arg2$arg3';
  }

  @override
  String get l6ToggleUserSkill2de88 => 'Toggle user skill';

  @override
  String get l6MarkedAsTrustedThisSkillS1cf36 =>
      'Marked as trusted: this skill’s scripts no longer require approval';

  @override
  String get l6ChangeTrustSetting7ce93 => 'Change trust setting';

  @override
  String get l6KnowledgeExport26466 => 'Knowledge export';

  @override
  String get l6DatabaseMaintenance60286 => 'Database maintenance';

  @override
  String get l6DailyTokenUsage5a651 => 'Daily token usage';

  @override
  String get l6TrustedNoApproval69302 => 'Trusted · no approval';

  @override
  String get l6RequireApprovalEachTime673dd => 'Require approval each time';

  @override
  String get l6TrustNoApproval12269 => 'Trust (no approval)';

  @override
  String get l6EnabledAPasswordIsRequiredWhen1aee3 =>
      'Enabled: a password is required when the app starts';

  @override
  String get l6DisabledTheAppOpensDirectlycbfe4 =>
      'Disabled: the app opens directly';

  @override
  String get l6SetAnAppPassword72f45 => 'Set an app password';

  @override
  String get l6ChangePassword0ae20 => 'Change password';

  @override
  String get l6TurnOffPassword7f7cb => 'Turn off password';

  @override
  String get l6OnlyASaltedPasswordHashIsd5644 =>
      'Only a salted password hash is stored in this device’s secure storage. It is never uploaded or synced. A forgotten password cannot be recovered; reset requires clearing app data.';

  @override
  String get l6FingerprintOrFaceUnlock1b96c => 'Fingerprint or face unlock';

  @override
  String get l6UseSystemBiometricsForQuickUnlocking22994 =>
      'Use system biometrics for quick unlocking. The password remains available as a fallback.';

  @override
  String get l6ThisDeviceDoesNotSupportBiometrics40ff0 =>
      'This device does not support biometrics or has none enrolled.';

  @override
  String get l6EnableFingerprintOrFaceUnlockb2a8f =>
      'Enable fingerprint or face unlock';

  @override
  String get l6TheLockScreenWillFirstOffer49b2b =>
      'The lock screen will first offer system biometrics. Confirm with your current app password.';

  @override
  String get l6CurrentPassword33abf => 'Current password';

  @override
  String get l6ConfirmEnable263d4 => 'Confirm enable';

  @override
  String get l6IncorrectCurrentPasswordNotEnabled691f1 =>
      'Incorrect current password; not enabled';

  @override
  String get l6BiometricUnlockEnablede15eb => 'Biometric unlock enabled';

  @override
  String get l6BiometricsAreCurrentlyUnavailableNotEnabled78c56 =>
      'Biometrics are currently unavailable; not enabled';

  @override
  String get l6AppPasswordSeta5390 => 'App password set';

  @override
  String get l6ChangeAppPassword25e9a => 'Change app password';

  @override
  String get l6AppPasswordChanged0493a => 'App password changed';

  @override
  String get l6TurnOffAppPassworda80d7 => 'Turn off app password';

  @override
  String get l6TheAppWillNoLongerRequire00cc0 =>
      'The app will no longer require a password on startup. Confirm with your current password.';

  @override
  String get l6ConfirmDisable059b4 => 'Confirm disable';

  @override
  String get l6AppPasswordTurnedOff9f4c1 => 'App password turned off';

  @override
  String get l6IncorrectCurrentPasswordNotDisabled0660c =>
      'Incorrect current password; not disabled';

  @override
  String get l6EnterTheCurrentPassword4b0b0 => 'Enter the current password';

  @override
  String get l6EnterANewPassword689e5 => 'Enter a new password';

  @override
  String get l6TheAppPasswordMustHave41cb25 =>
      'The app password must have 4 digits';

  @override
  String get l6TheNewPasswordsDoNotMatchd0b9c =>
      'The new passwords do not match';

  @override
  String get l6TheCurrentPasswordIsIncorrectb0dab =>
      'The current password is incorrect';

  @override
  String get l6CouldNotSaveTryAgainae0b2 => 'Could not save. Try again.';

  @override
  String get l6NewPassword4Digits51018 => 'New password (4 digits)';

  @override
  String get l6ConfirmNewPassworde0327 => 'Confirm new password';

  @override
  String get l6Confirmf526c => 'Confirm';

  @override
  String get l6ExportKnowledge863fb => 'Export knowledge';

  @override
  String get l6LoadDatabaseSize69850 => 'Load database size';

  @override
  String get l6OptimizeDatabase12519 => 'Optimize database';

  @override
  String get l6FetchingServicebfb3a => 'Fetching service';

  @override
  String get l6PostFetchingServiceUpdated1c8a1 =>
      'Post fetching service updated';

  @override
  String get l6SavePostFetchingService23d23 => 'Save post fetching service';

  @override
  String get l6ChooseASecretReference6a79c => 'Choose a secret reference';

  @override
  String get l6AddASecretInSecuritySecretb6b03 =>
      'Add a secret in Security → Secret management first.';

  @override
  String get l6PostFetching22fc8 => 'Post fetching';

  @override
  String get l6WebSearch678ed => 'Web search';

  @override
  String get l6OutboundProxyabb5e => 'Outbound proxy';

  @override
  String get l6EncryptedDnsDohc262a => 'Encrypted DNS (DoH)';

  @override
  String get l6UserSkills748e7 => 'User skills';

  @override
  String get l6McpServersad57a => 'MCP servers';

  @override
  String get l6ConnectModelContextProtocolServersTo6e5c7 =>
      'Connect Model Context Protocol servers to add external tools to conversations. AI calls them as needed. Local stdio servers start as subprocesses using the configured command.';

  @override
  String get l6EachToolCallRequiresApprovalBycbb3e =>
      'Each tool call requires approval by default. Trusting a server lets its calls execute directly. MCP servers are third-party code and can perform destructive operations. Enable trust only when you accept that behavior. For read-only queries, the separate exemption applies only to tools marked readOnlyHint; other calls still require approval.';

  @override
  String l6EnabledArg0Arg1d02a1(String arg0, String arg1) {
    return 'Enabled $arg0 / $arg1';
  }

  @override
  String get l6AddServer35085 => 'Add server';

  @override
  String get l6LoadingServers3023e => 'Loading servers…';

  @override
  String get l6NoMcpServersAddedChooseAddac3f0 =>
      'No MCP servers added\nChoose Add server to connect an external tool source';

  @override
  String get l6LoadMcpServers1900c => 'Load MCP servers';

  @override
  String get l6MarkedAsTrustedThisServerS74676 =>
      'Marked as trusted: this server’s tool calls no longer require approval';

  @override
  String get l6ReadOnlyExemptionEnabledForTools0d43a =>
      'Read-only exemption enabled for tools marked readOnlyHint by the server. Write tools still require approval.';

  @override
  String get l6ChangeReadOnlyApprovalExemptione8168 =>
      'Change read-only approval exemption';

  @override
  String get l6ToggleMcpServer14dd0 => 'Toggle MCP server';

  @override
  String get l6DeleteMultipleServers13274 => 'Delete multiple servers';

  @override
  String l6ServersToRemoveArg0SyncedCopiesccae5(String arg0) {
    return 'Servers to remove: $arg0. Synced copies will also be removed from other devices.';
  }

  @override
  String l6DeleteMcpServerArg0f17d5(String arg0) {
    return 'Delete MCP server $arg0';
  }

  @override
  String l6ServersDeletedArg05fc2a(String arg0) {
    return 'Servers deleted: $arg0';
  }

  @override
  String l6SucceededArg0FailedArg1Arg2Arg312d85(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return 'Succeeded: $arg0; failed: $arg1: $arg2$arg3';
  }

  @override
  String get l6DeleteMcpServer34624 => 'Delete MCP server';

  @override
  String get l6AddMcpServerc9d22 => 'Add MCP server';

  @override
  String get l6EditMcpServer4c8df => 'Edit MCP server';

  @override
  String get l6NameLettersDigitsHyphensOrUnderscores0dc18 =>
      'Name (letters, digits, hyphens or underscores)';

  @override
  String get l6RelatedIntentKeywordsOptionalb6b1f =>
      'Related intent keywords (optional)';

  @override
  String get l6ForExampleProductReviewsRecommendationsSales606d3 =>
      'For example: product reviews, recommendations, sales';

  @override
  String get l6SeparateKeywordsWithCommasToHelp6c10b =>
      'Separate keywords with commas to help AI select this tool for related tasks (up to 256 characters).';

  @override
  String get l6LocalCommandaa556 => 'Local command';

  @override
  String get l6RemoteServer1b481 => 'Remote server';

  @override
  String get l6Commandb114b => 'Command';

  @override
  String get l6ArgumentsJsonArray5ffa9 => 'Arguments (JSON array)';

  @override
  String get l6InsertASecretReference6edc7 => 'Insert a secret reference';

  @override
  String get l6EnvironmentVariablesJsonObject364cf =>
      'Environment variables (JSON object)';

  @override
  String get l6EndpointUrl28548 => 'Endpoint URL';

  @override
  String get l6CustomHeadersJsonObjectAuthenticationEtcd9233 =>
      'Custom headers (JSON object, authentication, etc.)';

  @override
  String get l6TrustedCallsSkipApproval6254b => 'Trusted (calls skip approval)';

  @override
  String get l6ToolsFromThisServerExecuteWithoutf61a4 =>
      'Tools from this server execute without individual approvals. MCP servers are third-party code; enable this only for servers you trust.';

  @override
  String get l6TestConnection10b7d => 'Test connection';

  @override
  String get l6SaveMcpServer25c8a => 'Save MCP server';

  @override
  String get l6EnterACommandBeforeTesting41aae =>
      'Enter a command before testing.';

  @override
  String get l6EnterAnHttpSEndpointBeforec28cd =>
      'Enter an HTTP(S) endpoint before testing.';

  @override
  String get l6ConnectedButTheServerProvidedNod0d33 =>
      'Connected, but the server provided no tools';

  @override
  String l6ConnectedArg0Arg1c4ea4(String arg0, String arg1) {
    return 'Connected: $arg0$arg1';
  }

  @override
  String get l6TestMcpConnection1eed1 => 'Test MCP connection';

  @override
  String get l6Remote80194 => 'Remote';

  @override
  String get l6AllPlatforms52e2a => 'All platforms';

  @override
  String l6OnlyArg064396(String arg0) {
    return 'Only $arg0';
  }

  @override
  String get l6ReadOnlyCallsSkipApproval63678 =>
      'Read-only calls skip approval';

  @override
  String l6RelatedIntentsArg0dff5f(String arg0) {
    return 'Related intents: $arg0';
  }

  @override
  String get l6DisableReadOnlyApprovalExemptioned6aa =>
      'Disable read-only approval exemption';

  @override
  String get l6EnableReadOnlyApprovalExemption95d1c =>
      'Enable read-only approval exemption';

  @override
  String get l6AiAnalysisec6c3 => 'AI analysis';

  @override
  String l6BranchesArg079379(String arg0) {
    return 'Branches: $arg0';
  }

  @override
  String l6Arg0MinutesAgo6b0c6(String arg0) {
    return '$arg0 minutes ago';
  }

  @override
  String l6Arg0HoursAgoaf702(String arg0) {
    return '$arg0 hours ago';
  }

  @override
  String l6Arg0DaysAgo70636(String arg0) {
    return '$arg0 days ago';
  }

  @override
  String get l6ReadingSettings9033d => 'Reading settings';

  @override
  String get l6TheThreeSettingsAreIndependentFollow172b0 =>
      'The three settings are independent. Follow global restores inheritance for each setting without changing the others.';

  @override
  String get l6Fontb50d4 => 'Font';

  @override
  String get l6InheritGlobal2a499 => 'Inherit global';

  @override
  String get l6OverrideGlobal93d3e => 'Override global';

  @override
  String get l6FontSize576cc => 'Font size';

  @override
  String get l6LineSpacing514b5 => 'Line spacing';

  @override
  String get l6ChooseAReadingFont8189e => 'Choose a reading font';

  @override
  String get l6SetUpAnAiProviderToc8ad4 =>
      'Set up an AI Provider to enable AI chat and automatic analysis.';

  @override
  String get l6Configurea44df => 'Configure';

  @override
  String get l6Queuecd354 => 'Queue';

  @override
  String get l6NoRecords09f60 => 'No records';

  @override
  String get l6FailedCoolingDown4a20e => 'Failed (cooling down)';

  @override
  String l6RecordedArg085217(String arg0) {
    return 'Recorded $arg0';
  }

  @override
  String l6AttemptsArg0fcc38(String arg0) {
    return 'Attempts: $arg0';
  }

  @override
  String l6AutomaticallyRequeuedArg0Times3363a(String arg0) {
    return 'Automatically requeued $arg0 times';
  }

  @override
  String l6NextAttemptArg0272da(String arg0) {
    return 'Next attempt: $arg0';
  }

  @override
  String l6EventsArg045727(String arg0) {
    return 'Events: $arg0';
  }

  @override
  String get l6Updated9db0 => 'Update';

  @override
  String get l6NoNewKnowledgeExtractedc38ab => 'No new knowledge extracted';

  @override
  String get l6DohSettingsSavedeaeea => 'DoH settings saved';

  @override
  String get l6EnterADohServiceUrlBeforeff97f =>
      'Enter a DoH service URL before testing.';

  @override
  String get l6ActivationPolicyfc94d => 'Activation policy';

  @override
  String get l6DohServiceUrlae8b4 => 'DoH service URL';

  @override
  String get l6OffTheUrlIsSavedButa536d =>
      'Off: the URL is saved but not used. Web search connects directly.';

  @override
  String get l6RequiredADohFailureReturnsAnaabc1 =>
      'Required: a DoH failure returns an error without falling back to direct access.';

  @override
  String get l6AutomaticPreferDohAndEchFallingfc42f =>
      'Automatic: prefer DoH and ECH, falling back to a direct connection on failure.';

  @override
  String get l6Testingcf7e9 => 'Testing';

  @override
  String get l6RecordWhatIsHappeningUseName925ca =>
      'Record what is happening… (use @name for contacts and #topic for projects)';

  @override
  String get l6ConversationFormat9d903 => 'Conversation format';

  @override
  String get l6HowToRecord2bcbc => 'How to record';

  @override
  String get l6UseNameToIdentifyAContact6d759 =>
      'Use @name to identify a contact and #topic for a project, task or topic.\n\nFor example: @Alex is handling #payment-flow.\n\n“Remember this” saves an experience or update.\n“Save to knowledge base” keeps a conclusion.\n“Create a to-do” drafts a follow-up.\n\nRelationships, knowledge pages, to-dos and rules are shown as drafts first. Open Review proposals and accept or reject the specific draft. Ordinary chat replies do not execute drafts.';

  @override
  String l6Arg0Revoked9671e(String arg0) {
    return '$arg0 (revoked)';
  }

  @override
  String l6DeviceArg0WillImmediatelyLoseSyncadbd3(String arg0) {
    return 'Device “$arg0” will immediately lose sync access.\n\nKeys already held by the device are not removed. Revoke or rotate them in the Provider console. Remaining devices will receive the new version.';
  }

  @override
  String l6ChangesFromArg0710df(String arg0) {
    return 'Changes from $arg0';
  }

  @override
  String l6RenamedFromArg0e3178(String arg0) {
    return 'Renamed from “$arg0”';
  }

  @override
  String l6ContentLengthArg0CharactersUpdatedArg1f8788(
    String arg0,
    String arg1,
  ) {
    return 'Content length: $arg0 characters · Updated $arg1';
  }

  @override
  String l6ConditionsArg006edc(String arg0) {
    return 'Conditions: $arg0';
  }

  @override
  String get l6WebSearchSettingsSaved30dee => 'Web search settings saved';

  @override
  String get l6SearchEnginee5837 => 'Search engine';

  @override
  String get l6SearxngInstanceUrla3267 => 'SearxNG instance URL';

  @override
  String get l6ApiUrlacd8a => 'API URL';

  @override
  String get l6HttpLocalhost8080OrYourSearxngba754 =>
      'http://localhost:8080 or your SearxNG URL';

  @override
  String get l6SearxngIsSelfHostedMetasearchNoe6c57 =>
      'SearxNG is self-hosted metasearch. No API key is needed. Enable JSON output on your instance.';

  @override
  String get l6EnableWebSearch9ee9a => 'Enable web search';

  @override
  String get l6AllowAiToSearchTheWeb1ea6a =>
      'Allow AI to search the web for current information and news.';

  @override
  String get l6TvlyFreeAllowanceAtTavilyCom279ec =>
      'tvly-... (free allowance at tavily.com)';

  @override
  String get l6BsaApplyAtBraveComSearch6c59c =>
      'BSA... (apply at brave.com/search/api)';

  @override
  String get l6ConfiguredEnterAValueToReplace1d9d7 =>
      'Configured (enter a value to replace; leave blank to keep)';

  @override
  String get l6ProxySettingsSavedOutboundRequestsUseac39b =>
      'Proxy settings saved; outbound requests use them immediately';

  @override
  String get l6EnterAProxyUrlBeforeTesting89cfb =>
      'Enter a proxy URL before testing.';

  @override
  String get l6UseHttpsProxyOrAllProxy33ed0 =>
      'Use HTTPS_PROXY or ALL_PROXY from the environment; connect directly if unset';

  @override
  String get l6ConfigureAProxyea2d0 => 'Configure a proxy';

  @override
  String get l6UseTheUrlBelowInsteadOf96914 =>
      'Use the URL below instead of environment variables. The DoH tunnel yields while the proxy is active.';

  @override
  String get l6NoProxy4f187 => 'No proxy';

  @override
  String get l6ConnectDirectlyIgnoringProxyEnvironmentVariablesa4dbf =>
      'Connect directly, ignoring proxy environment variables';

  @override
  String get l6Http1270017890a9d59 => 'http://127.0.0.1:7890 or socks5://…';

  @override
  String get l6SelectAnEventToViewItsd5e60 =>
      'Select an event to view its details';

  @override
  String get l6EventDoesNotExistf5542 => 'Event does not exist';

  @override
  String get l6EventRecordsbabe0 => 'Event records';

  @override
  String get l6RecordedAt650b3 => 'Recorded at';

  @override
  String get l6NoAnalysisYetPendingOrNo90172 =>
      'No analysis yet (pending or no analyzer configured).';

  @override
  String get l6AnalysisSummary83a0f => 'Analysis summary';

  @override
  String get l6Peopleb755b => 'People';

  @override
  String get l6NoRelatedRuleReminders24596 => 'No related rule reminders';

  @override
  String get l6CouldNotLoadEvent7ba1f => 'Could not load event';

  @override
  String get l6ConversationsWithAiAreRecordedAs6eb67 =>
      'Conversations with AI are recorded as events for reviews and knowledge processing.';

  @override
  String get l6TodaySRecordse0105 => 'Today’s records';

  @override
  String get l6Last7Days2a0a0 => 'Last 7 days';

  @override
  String get l6NoEventsRecordedYetConversationsWithb4ac9 =>
      'No events recorded yet\nConversations with AI are recorded here automatically';

  @override
  String get l6NoRecentlyRecordedEventsf0c62 => 'No recently recorded events';

  @override
  String get l6TryAgainLaterb8e0d => 'Try again later';

  @override
  String l6Arg0CompletedArg1Items30653(String arg0, String arg1) {
    return '$arg0 completed · $arg1 items';
  }

  @override
  String l6AutomaticRetryArg06d99e(String arg0) {
    return 'Automatic retry: $arg0';
  }

  @override
  String l6Arg0FromThisPageSSource92ed3(String arg0) {
    return '$arg0 · From this page’s source material';
  }

  @override
  String l6AcceptChangesArg0a09f4(String arg0) {
    return 'Accept changes $arg0';
  }

  @override
  String get l6RemoveThisParagraphe26cb => 'Remove this paragraph';

  @override
  String l6HistoricalSnapshotsArg0cbbd3(String arg0) {
    return 'Historical snapshots: $arg0';
  }

  @override
  String l6LatestArg0Arg1aa89a(String arg0, String arg1) {
    return 'Latest: $arg0 · $arg1';
  }

  @override
  String l6DeviceNameArg0a02e4(String arg0) {
    return 'Device name: $arg0';
  }

  @override
  String l6RoleArg0f3803(String arg0) {
    return 'Role: $arg0';
  }

  @override
  String get l6NoRelayConfiguredSetItWhenfc098 =>
      'No relay configured (set it when inviting the first device)';

  @override
  String get l6StartANewChapterHereEarliere67c8 =>
      'Start a new chapter here; earlier history moves into a chapter summary';

  @override
  String get l6ThisChapterMarkerNoLongerExistsec974 =>
      'This chapter marker no longer exists; it may have been undone.';

  @override
  String get l6NewChapterSummarycf20d => 'New chapter · Summary';

  @override
  String get l6GeneratingChapterSummary277ba => 'Generating chapter summary…';

  @override
  String get l6DiscardChanges1cead => 'Discard changes';

  @override
  String get l6ConfirmNewChapter12069 => 'Confirm new chapter';

  @override
  String get l6RemoveThisEntry52142 => 'Remove this entry';

  @override
  String l6AddArg03e9a8(String arg0) {
    return 'Add $arg0';
  }

  @override
  String get l6AfterStartingANewChapterThis0bfa3 =>
      'After starting a new chapter, this summary replaces the earlier text in requests. AI can reread original messages for exact values or quotations. Edit or remove entries before confirming.';

  @override
  String get l6NoChapterHistoryInThisConversation36eaa =>
      'No chapter history in this conversation yet';

  @override
  String get l6Day15917 => 'Day';

  @override
  String get l6Week451b8 => 'Week';

  @override
  String get l6Monthd9b59 => 'Month';

  @override
  String get l6PreviousDaydede7 => 'Previous day';

  @override
  String get l6NextDaya4161 => 'Next day';

  @override
  String get l6CouldNotLoadReview55475 => 'Could not load review';

  @override
  String get l6AiReviewbb899 => 'AI review';

  @override
  String get l6NoAiReviewForThisDay286ea => 'No AI review for this day yet';

  @override
  String get l6EventsForThisDayfbec1 => 'Events for this day';

  @override
  String get l6NoEventsRecordedOnThisDay6a37a =>
      'No events recorded on this day';

  @override
  String get l6ThisDayLastWeekfc29d => 'This day last week';

  @override
  String l6EarlierReviewsArg0f4cc0(String arg0) {
    return 'Earlier reviews ($arg0)';
  }

  @override
  String l6PeriodReviewArg0ToArg1Arg269c4d(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return 'Period review · $arg0 to $arg1$arg2';
  }

  @override
  String get l6Events550e3 => 'Events';

  @override
  String get l6ToDosCompletede1bfa => 'To-dos completed';

  @override
  String get l6DeviationChecksPending648aa => 'Deviation checks pending';

  @override
  String get l6NoDeviationsToReport28ee2 => 'No deviations to report';

  @override
  String get l6MomentsThatDivergedFromYourGoals39f82 =>
      'Moments that diverged from your goals—observations without judgment:';

  @override
  String l6Arg0Dismissede66fc(String arg0) {
    return '$arg0 · Dismissed';
  }

  @override
  String l6GoalArg06fb2b(String arg0) {
    return 'Goal: $arg0';
  }

  @override
  String get l6CouldNotLoadStatistics88ae7 => 'Could not load statistics';

  @override
  String get l6NoRecordsThisWeeka75a5 => 'No records this week';

  @override
  String get l6NoRecordsThisMonthd57d9 => 'No records this month';

  @override
  String get l6EventTypes40b68 => 'Event types';

  @override
  String get l6Activitiesb2548 => 'Activities';

  @override
  String l6KnowledgeProcessingArg0ActivePagesArg179c73(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return 'Knowledge processing · $arg0 active pages$arg1$arg2 · $arg3 summaries in 30 days$arg4';
  }

  @override
  String get l6KnowledgeProcessingHealthLast30Days3bf5c =>
      'Knowledge processing health (last 30 days)';

  @override
  String get l6Compilation8669e => 'Compilation';

  @override
  String l6PendingProposalsArg030DaysAcceptedf2e76(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return 'Pending proposals $arg0 (30 days: accepted $arg1, rejected $arg2) · Failed background jobs $arg3/$arg4';
  }

  @override
  String get l6Librarydfd6b => 'Library';

  @override
  String l6ActiveArg0ArchivedArg1NoSummaryb4539(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  ) {
    return 'Active $arg0 · Archived $arg1 · No summary $arg2 · Stale $arg3 · Never shown $arg4';
  }

  @override
  String get l6Delivery412a9 => 'Delivery';

  @override
  String l6AttachedArg0TimesArg1PagesUsageabc07(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return 'Attached $arg0 times ($arg1 pages) · Usage records $arg2 · Cited pages $arg3';
  }

  @override
  String get l6Retrieval1c037 => 'Retrieval';

  @override
  String l6AiSearchesArg0PageReadsArg15216f(String arg0, String arg1) {
    return 'AI searches $arg0 · Page reads $arg1';
  }

  @override
  String get l6SummaryPipeline00b38 => 'Summary pipeline';

  @override
  String l6CompletedArg0UnavailableArg1Arg2ee56d(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return 'Completed $arg0 · Unavailable $arg1$arg2';
  }

  @override
  String get l6SourceReviewGate877bd => 'Source review gate';

  @override
  String l6SourcesAwaitingADecisionArg0Rejected9c019(String arg0, String arg1) {
    return 'Sources awaiting a decision $arg0 · Rejected sources $arg1';
  }

  @override
  String get l6SummaryPreparationHistoryProcessedInBackground2648e =>
      'Summary preparation history (processed in background batches)';

  @override
  String get l6NoPreparationHistoryYetBackgroundBatchescc092 =>
      'No preparation history yet. Background batches prepare large summary trees as needed.';

  @override
  String get l6LoadingHandoffStatus19264 => 'Loading handoff status…';

  @override
  String l6TransferAiAnalysisResponsibilityToArg05bc00(
    String arg0,
    String arg1,
  ) {
    return 'Transfer AI analysis responsibility to “$arg0” ($arg1).\n\nThis device stops claiming background tasks immediately. The target starts after syncing to the handoff boundary and explicitly accepting. This device cannot resume unilaterally.';
  }

  @override
  String get l6EnrollmentUsesASeparateStagingDatabase0ddf7 =>
      'Enrollment uses a separate staging database without overwriting current records or Providers. After approval and installation, the channel, roster and initial snapshot are coordinated automatically. The active dataset stays selected.';

  @override
  String get l6UsingKey99422 => 'Using Key';

  @override
  String l6InvitingDeviceArg0d440c(String arg0) {
    return 'Inviting device: $arg0';
  }

  @override
  String l6DatasetArg085f96(String arg0) {
    return 'Dataset: $arg0';
  }

  @override
  String l6InvitationExpiresArg0abd8a(String arg0) {
    return 'Invitation expires: $arg0';
  }

  @override
  String l6DatasetArg0OnlyApprovedCheckpointsCand6435(String arg0) {
    return 'Dataset: $arg0\nOnly approved checkpoints can be resumed. Unapproved enrollment needs a new invitation after restart.';
  }

  @override
  String l6StagedDataAndDeviceKeysFor0f456(String arg0) {
    return 'Staged data and device keys for “$arg0” will be permanently deleted. Joining later requires scanning a new invitation and obtaining approval.';
  }

  @override
  String l6ThisDeviceArg0Arg1ab420(String arg0, String arg1) {
    return 'This device: $arg0 · $arg1';
  }

  @override
  String get l6ThisDeviceSSyncIdentityIs46884 =>
      'This device’s sync identity is incomplete, which can follow interrupted pairing, power loss or restoring a backup. Clear the identity and pair again. Notes, conversations and knowledge are preserved.';

  @override
  String l6SyncOperationsAwaitingConfirmationArg0224a8(String arg0) {
    return 'Sync operations awaiting confirmation: $arg0';
  }

  @override
  String l6DeviceArg0d7706(String arg0) {
    return 'Device: $arg0';
  }

  @override
  String l6PlatformArg0e4ee7(String arg0) {
    return 'Platform: $arg0';
  }

  @override
  String l6ExpiresArg0a7daf(String arg0) {
    return 'Expires: $arg0';
  }

  @override
  String l6AwaitingReviewArg0Arg17fa78(String arg0, String arg1) {
    return 'Awaiting review: $arg0 ($arg1)';
  }

  @override
  String l6RevisionFromArg0beef3(String arg0) {
    return 'Revision from $arg0';
  }

  @override
  String l6AddressHintArg03f1aa(String arg0) {
    return 'Address hint: $arg0';
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
    return 'Last sync: uploaded $arg0 · received $arg1$arg2$arg3$arg4$arg5';
  }

  @override
  String get l6Sendingd948e => 'Sending';

  @override
  String l6RetryingLaterArg0baf89(String arg0) {
    return 'Retrying later: $arg0';
  }

  @override
  String l6UnknownStateArg0c8bbc(String arg0) {
    return 'Unknown state: $arg0';
  }

  @override
  String l6RetryInArg0Secondsccd79(String arg0) {
    return 'Retry in $arg0 seconds';
  }

  @override
  String l6RetryInArg0Minutesef5e1(String arg0) {
    return 'Retry in $arg0 minutes';
  }

  @override
  String l6DeliveryArg0QueuedArg1AwaitingConfirmationa5864(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  ) {
    return 'Delivery $arg0 (queued $arg1 · awaiting confirmation $arg2): $arg3';
  }

  @override
  String l6PeerHasNotConfirmedArg0Itemsc4ccb(String arg0, String arg1) {
    return 'Peer has not confirmed $arg0 items · $arg1';
  }

  @override
  String get l6EnterTheTargetDeviceIdb0da4 => 'Enter the target device ID.';

  @override
  String get l6ThisDeviceIsNotTheProcessor82939 =>
      'This device is not the processor; no handoff is needed.';

  @override
  String get l6OnlyTheDatasetOwnerCanApprovec5d36 =>
      'Only the dataset owner can approve handoff proof.';

  @override
  String get l6WaitForThisDeviceSActive1e382 =>
      'Wait for this device’s active background jobs to finish before transferring AI analysis.';

  @override
  String get l6WaitForSyncToReachThe58a6b =>
      'Wait for sync to reach the handoff boundary before accepting responsibility.';

  @override
  String get l6NoProcessorHandoffAwaitsThisDevice12c2e =>
      'No processor handoff awaits this device’s acceptance.';

  @override
  String get l6TheHandoffProofSignatureIsInvalid82d09 =>
      'The handoff proof signature is invalid; it may have been altered.';

  @override
  String get l6HandoffProofInformationIsOutsideIts3c0a0 =>
      'Handoff proof information is outside its valid range.';

  @override
  String get l6TheProofDoesNotMatchThe90664 =>
      'The proof does not match the current roster version. Start the handoff again on the original processor.';

  @override
  String get l6TheProofDoesNotMatchThe7ec43 =>
      'The proof does not match the current processor. Start the handoff again there.';

  @override
  String get l6TheTargetIsNotAnActivea00bf =>
      'The target is not an active roster member.';

  @override
  String get l6TheOriginalProcessorIsNoLongerd44b7 =>
      'The original processor is no longer in the roster; this proof cannot be approved.';

  @override
  String get l6TheHandoffProofBelongsToA7b34b =>
      'The handoff proof belongs to a different dataset.';

  @override
  String get l6AiAnalysisCannotBeTransferredTod5ffa =>
      'AI analysis cannot be transferred to the same device.';

  @override
  String get l6TheHandoffProofDoesNotMatch31e8f =>
      'The handoff proof does not match this device’s state.';

  @override
  String get l6HandoffProofIsMissingTheDevice668d5 =>
      'Handoff proof is missing; the device change cannot be applied.';

  @override
  String get l6TheProofSSourceDoesNot02b44 =>
      'The proof’s source does not match this device’s state.';

  @override
  String get l6TheHandoffProofHasNoSource9502a =>
      'The handoff proof has no source device.';

  @override
  String get l6TheHandoffProofTargetsADifferent2a815 =>
      'The handoff proof targets a different device.';

  @override
  String get l6ProcessorHandoffProofIsMissingThis0e9df =>
      'Processor handoff proof is missing; this device cannot accept the change.';

  @override
  String get l6TheRosterIncludesHandoffProofWithout363f4 =>
      'The roster includes handoff proof without changing the processor.';

  @override
  String get l6UnlockThisDeviceSSyncKeyse19a6 =>
      'Unlock this device’s sync keys before continuing.';

  @override
  String get l6ThisDeviceSSyncIdentityIs5c640 =>
      'This device’s sync identity is not ready.';

  @override
  String get l6TransferThisDeviceSAiAnalysis9e01d =>
      'Transfer this device’s AI analysis responsibility before revoking access.';

  @override
  String get l6AnotherDeviceUpdatedTheRosterRefreshb7381 =>
      'Another device updated the roster. Refresh and review the revocation again.';

  @override
  String get l6OnlyTheDatasetOwnerCanRevoke2ce42 =>
      'Only the dataset owner can revoke a member.';

  @override
  String get l6ThisDeviceSAccessHasAlreadyb76c9 =>
      'This device’s access has already been revoked.';

  @override
  String get l6ThisDeviceIsNotInThe3f9c7 => 'This device is not in the roster.';

  @override
  String get l6InvalidRosterRevisione21c2 => 'Invalid roster revision.';

  @override
  String get l6TheRosterHasReachedItsMemberee48e =>
      'The roster has reached its member limit.';

  @override
  String get l6DeviceIdentitiesConflictInTheRosterea7bc =>
      'Device identities conflict in the roster (duplicate key or stream).';

  @override
  String get l6TheLocalPageChangedRefreshAnd4e980 =>
      'The local page changed. Refresh and review again.';

  @override
  String get l6ChannelAuthorizationOrRevisionChangedAutomatic779c0 =>
      'Channel authorization or revision changed; automatic handoff stopped. Check the original device and relay permissions. New revisions will not overwrite authorization.';

  @override
  String get l6TheHandoffRevisionOrIdentityCould284a1 =>
      'The handoff revision or identity could not be confirmed. Unlock this device or resume the original staging entry. Keys will not be reset.';

  @override
  String get l6TheRevisionAndReleaseOfThebd873 =>
      'The revision and release of the local session are unconfirmed. Preserve the data, restart the app, then resume.';

  @override
  String get l6TheHandoffStepIsIncompleteThe70ae7 =>
      'The handoff step is incomplete. The original request will be retried without new approval or keys.';

  @override
  String l6Arg0Secondse612a(String arg0) {
    return '$arg0 seconds';
  }

  @override
  String l6Arg0Milliseconds5f522(String arg0) {
    return '$arg0 milliseconds';
  }

  @override
  String l6TheSyncStepDidNotRespond28e88(String arg0) {
    return 'The sync step did not respond within $arg0. It has been queued for retry; data is preserved.';
  }

  @override
  String get l6TheApprovedHandoffIsBeingCoordinated7e9e2 =>
      'The approved handoff is being coordinated. Initial snapshot, receipts and dataset switching are separate stages.';

  @override
  String get l6TheNativeHandoffRevisionCouldNot775d0 =>
      'The native handoff revision could not be confirmed. The local session stopped. Unlock and resume the original staging entry instead of joining again.';

  @override
  String get l6PreviewDidNotConnectToThed4c19 =>
      'Preview did not connect to the relay. Verify the invitation’s source, address and device fingerprint.';

  @override
  String get l6EnterADeviceNameAndExplicitly802c8 =>
      'Enter a device name and explicitly agree to connect to this relay and receive Provider settings and API keys.';

  @override
  String get l6JoinRequestDeliveredVerifyThisDevice8e98f =>
      'Join request delivered. Verify this device’s fingerprint on an existing device and explicitly approve it.';

  @override
  String get l6NoUnfinishedEnrollmentStagingEntriesaa853 =>
      'No unfinished enrollment staging entries.';

  @override
  String get l6RecoveryReadsTheOriginalInstallationS770c9 =>
      'Recovery reads the original installation’s keys without creating replacements.';

  @override
  String get l6StagingEntryDeletedButKeyCleanup3ce20 =>
      'Staging entry deleted, but key cleanup failed. Check device key health later.';

  @override
  String l6StagingEntryArg0Deleted0f067(String arg0) {
    return 'Staging entry “$arg0” deleted.';
  }

  @override
  String get l6OriginalInstallationProofResentWaitingForee8e7 =>
      'Original installation proof resent. Waiting for authorization and initial sync; no new approval or keys were created.';

  @override
  String get l6OriginalRequestResentWaitingForExplicitb70c0 =>
      'Original request resent. Waiting for explicit approval on an existing device.';

  @override
  String get l6InstallationProofDeliveredWaitingForAuthorization69cc4 =>
      'Installation proof delivered. Waiting for authorization and initial sync. The active dataset has not changed.';

  @override
  String get l6NoUsableApprovalYetVerifyAnd0b6c4 =>
      'No usable approval yet. Verify and approve on an existing device.';

  @override
  String get l6TheSameInstallationProofWasResent1abc4 =>
      'The same installation proof was resent. Delivery does not mean enrollment or sync is complete.';

  @override
  String get l6EnrollmentStoppedStagedDataAndKeys9b648 =>
      'Enrollment stopped. Staged data and keys are preserved. Temporary delivery permission will expire. Sent requests or already approved membership are not revoked.';

  @override
  String get l6DeviceKeysOrProtectedRevisionsAre5de96 =>
      'Device keys or protected revisions are missing. Recovery stopped; replacement keys will not be generated.';

  @override
  String get l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35 =>
      'Protected revisions rolled back. Enrollment stopped; original and staged data are preserved.';

  @override
  String get l6DeviceKeyRecordsAreDamagedEnrollment76c86 =>
      'Device key records are damaged. Enrollment stopped without overwriting keys.';

  @override
  String get l6SecureKeyStorageCouldNotSave6f761 =>
      'Secure key storage could not save or read back safely. Enrollment stopped. Unlock the key store and resume the staging entry.';

  @override
  String get l6ThisWorkspaceHasAnActiveSync69142 =>
      'This workspace has an active sync session or background jobs. Finish them or lock current sync before joining.';

  @override
  String get l6TheInvitationExpiredOrHasNo6c654 =>
      'The invitation expired or has no approved checkpoint to resume. Stop this enrollment and request a new invitation. Existing data is preserved.';

  @override
  String get l6TheRelayStepIsUnconfirmedCheck92aea =>
      'The relay step is unconfirmed. Check approval or resend the original request or proof; do not generate new keys.';

  @override
  String get l6EnrollmentIsIncompleteAndHasStopped1c448 =>
      'Enrollment is incomplete and has stopped. Check the original invitation, unlock state and staging entry. The active dataset has not changed.';

  @override
  String get l6ConfirmThatTheJoiningDeviceMay8297c =>
      'Confirm that the joining device may receive all Provider settings and API keys.';

  @override
  String get l6TheInvitationIsValidFor245be17 =>
      'The invitation is valid for 24 hours. A scanned join request appears here automatically. Verify the device and fingerprint, then approve.';

  @override
  String get l6NoJoinRequestReceivedYet10542 => 'No join request received yet.';

  @override
  String get l6VerifyTheDeviceAndFingerprintBefore5e3e6 =>
      'Verify the device and fingerprint before explicitly approving.';

  @override
  String get l6ExplicitlyConfirmSharingProviderApiKeysd0cdd =>
      'Explicitly confirm sharing Provider API keys with this device.';

  @override
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8 =>
      'Approval delivered; waiting for installation confirmation. Enrollment and sync are not yet complete.';

  @override
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation9b218 =>
      'Approval delivered; waiting for installation confirmation.';

  @override
  String get l6InvitationCanceled35e1b => 'Invitation canceled.';

  @override
  String get l6ApprovalDeliveredWaitingForTheNew361e9 =>
      'Approval delivered; waiting for the new device to confirm installation.';

  @override
  String get l6TheInvitationOrReviewVersionExpireda53ec =>
      'The invitation or review version expired. Create a new invitation and verify the device again.';

  @override
  String get l6TheInvitationWasCanceledLocallyRelay360d8 =>
      'The invitation was canceled locally; relay revocation is unconfirmed. Temporary sending permission expires with the invitation.';

  @override
  String get l6ApprovalIsSavedButHasNot99508 =>
      'Approval is saved but has not reached the new device because the network or relay is unavailable. Delivery retries automatically; no new approval is needed.';

  @override
  String get l6BackgroundSyncIsUsingTheRuntime0a21c =>
      'Background sync is using the runtime. This pairing write did not execute. Try again shortly. No new device was authorized automatically.';

  @override
  String get l6PairingIsIncompleteCheckDeviceUnlock85108 =>
      'Pairing is incomplete. Check device unlock state and relay address. No new device was authorized automatically.';

  @override
  String get l6UnlockThisDeviceSSyncKeysfc734 =>
      'Unlock this device’s sync keys before reviewing conflicts.';

  @override
  String get l6LocalSettingsChangedReviewTheUpdated3cd59 =>
      'Local settings changed. Review the updated list below.';

  @override
  String get l6TheConflictChangedReviewTheUpdated936b5 =>
      'The conflict changed. Review the updated list below.';

  @override
  String get l6CouldNotReadDeviceSyncState278eb =>
      'Could not read device sync state. Existing records are preserved.';

  @override
  String get l6DeviceSyncIsNotReady3fc30 => 'Device sync is not ready.';

  @override
  String get l6DeviceNamesMustContain12561d9b2 =>
      'Device names must contain 1–256 characters.';

  @override
  String get l6AnotherOperationUpdatedTheStateRefresh39e75 =>
      'Another operation updated the state. Refresh and try again.';

  @override
  String get l6UnlockDeviceKeysBeforeRenaming10735 =>
      'Unlock device keys before renaming.';

  @override
  String get l6TheDeviceNameContainsInvalidCharacters438ed =>
      'The device name contains invalid characters. Choose another name.';

  @override
  String get l6EnterADeviceNameAndConfirm02f54 =>
      'Enter a device name and confirm that pairing shares Provider API keys.';

  @override
  String get l6StopEnrollmentBeforePreparingOrUnlocking4420c =>
      'Stop enrollment before preparing or unlocking the current dataset.';

  @override
  String get l6LockingIsUnconfirmedTryAgainKeys6bf44 =>
      'Locking is unconfirmed. Try again; keys will not be regenerated.';

  @override
  String get l6DeviceKeysOrProtectedRevisionsAre845e9 =>
      'Device keys or protected revisions are missing. Recover through the original device; replacement keys will not be generated.';

  @override
  String get l6KeyRevisionsRolledBackUnlockingStoppedc58f3 =>
      'Key revisions rolled back. Unlocking stopped. Check the restored data; original records are preserved.';

  @override
  String get l6SystemKeyRecordsAreDamagedUnlocking23c95 =>
      'System key records are damaged. Unlocking stopped without overwriting data or keys.';

  @override
  String get l6TheSystemKeyStoreIsUnavailable8d23d =>
      'The system key store is unavailable or locked. Unlock it and retry. You can still save local records.';

  @override
  String get l6AnotherWindowOrBackgroundTaskIs610c2 =>
      'Another window or background task is using this workspace. Wait for it to finish, then retry.';

  @override
  String get l6BackgroundJobsAreStillRunningPrepare56569 =>
      'Background jobs are still running. Prepare sync after they finish. Processor ownership has not changed.';

  @override
  String get l6TheDeviceSyncIdentityNeedsRecoveryd2a55 =>
      'The device sync identity needs recovery. Automatic initialization stopped. Preserve the original database and rejoin through an authorized device.';

  @override
  String get l6SyncPreparationDidNotFinishAnd1c512 =>
      'Sync preparation did not finish and remains locked. Try again; original records are preserved.';

  @override
  String get l6AndMore93ac7 => ' and more';

  @override
  String get l6SupportedServiceFxtwitterConvertsXCom0989a =>
      'Supported service: fxTwitter converts x.com links into full articles.\nEndpoint: https://api.fxtwitter.com/status/<post-id>';

  @override
  String l6AndArg0MoreTools52a1a(String arg0) {
    return ' and $arg0 more tools';
  }

  @override
  String l6Arg0YouRecordedArg1Arg2Ac3209(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '${arg0}you recorded: “$arg1”$arg2 — a moment to remember.';
  }

  @override
  String l6SupplyYourOwnEndpointNoneIsd855e(String arg0) {
    return 'Supply your own endpoint; none is built in. Web searches use DoH+ECH to reach services affected by DNS or SNI blocking (such as x.com). This applies only to web search.$arg0';
  }

  @override
  String get l6Check40ffb => 'Check';

  @override
  String get l6LoadingDevices5c6b5 => 'Loading devices…';

  @override
  String l6Arg0ProposalArg1PagesArg2Pages2ee4f(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return '${arg0}proposal · $arg1 pages → $arg2 pages';
  }

  @override
  String l6SourcesArg0EventsArg10f288(String arg0, String arg1) {
    return 'Sources: $arg0 · Events: $arg1';
  }

  @override
  String l6SwitchedToArg0QuitAndReopenc2285(String arg0) {
    return 'Switched to “$arg0”. Quit and reopen the app to load its synced data.';
  }

  @override
  String l6JoinedDatasetArg0ItsDataIs7a081(String arg0) {
    return 'Joined dataset “$arg0”. Its data is not yet loaded in this window.';
  }

  @override
  String l6DatasetsReadyToOpenArg0c5aa4(String arg0) {
    return 'Datasets ready to open: $arg0';
  }

  @override
  String l6DevicesOnlineArg0Arg1c5eda(String arg0, String arg1) {
    return 'Devices online: $arg0/$arg1';
  }

  @override
  String l6HeartbeatHealthyPendingArg05c133(String arg0) {
    return 'Heartbeat healthy · Pending: $arg0';
  }

  @override
  String get l6FollowUpSuggestions99ccc => 'Follow-up suggestions';

  @override
  String get l6Guidance79851 => 'Guidance';

  @override
  String l6RelatedRuleRemindersArg05615f(String arg0) {
    return 'Related rule reminders ($arg0)';
  }

  @override
  String get l6AwaitingAnalysis605dc => 'Awaiting analysis';

  @override
  String get l6RecordedAwaitingAnalysis8e5e7 => 'Recorded; awaiting analysis';

  @override
  String l6CurrentRevisionArg0beb34(String arg0) {
    return 'Current revision: $arg0';
  }

  @override
  String get l6ResolvedWithALinkedPagef44a7 => 'Resolved with a linked page';

  @override
  String get l6Dismissedbd172 => 'Dismissed';

  @override
  String get l6OriginalSuggestiondb86c => 'Original suggestion';

  @override
  String get l6OpenLinkedKnowledgePage88f20 => 'Open linked knowledge page';

  @override
  String get l6LoadProcessingQueue079ca => 'Load processing queue';

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
    return 'Total: $arg0 · Pending: $arg1 · Running: $arg2 · Waiting: $arg3 · Retry: $arg4 · Skipped: $arg5 · Completed: $arg6';
  }

  @override
  String get l6RefreshListbd82e => 'Refresh list';

  @override
  String get l6FullDocumentReadingc1da8 => 'Full-document reading';

  @override
  String get l6SourceProcessingfbebb => 'Source processing';

  @override
  String get l6CrossSourceProcessing5240c => 'Cross-source processing';

  @override
  String get l6KnowledgeReview4f5bb => 'Knowledge review';

  @override
  String get l6SourceRefresh65f83 => 'Source refresh';

  @override
  String l6ScheduledArg020b73(String arg0) {
    return '\nScheduled: $arg0';
  }

  @override
  String l6LastProcessedArg0NewEvidenceArg124fb9(String arg0, String arg1) {
    return 'Last processed: $arg0 · New evidence: $arg1';
  }

  @override
  String get l6RecentProcessingAndInsights23cc8 =>
      'Recent processing and insights';

  @override
  String get l6LoadInsights25b9e => 'Load insights';

  @override
  String get l6CheckedAutomaticallyInTheBackgroundAs34c09 =>
      'Checked automatically in the background as events accumulate.';

  @override
  String get l6SourceUpdateCheckc6668 => 'Source update check';

  @override
  String get l6TopicProcessingAndChecks872c6 => 'Topic processing and checks';

  @override
  String get l6LoadSourceEventsaaf09 => 'Load source events';

  @override
  String get l6LoadKnowledgeOutputs54f00 => 'Load knowledge outputs';

  @override
  String get l6CrossSourceTopicIncludesThisPageae2b4 =>
      'Cross-source topic · Includes this page and related sources';

  @override
  String get l6AwaitingReview28cc1 => 'Awaiting review';

  @override
  String get l6ConfirmAndSave706df => 'Confirm and save';

  @override
  String get l6ReferenceKnowledgeSaved962eb => 'Reference knowledge saved';

  @override
  String get l6RecordAnOpinionb7461 => 'Record an opinion';

  @override
  String get l6LoadSourcesAndRevisionsd1d70 => 'Load sources and revisions';

  @override
  String get l6UnnamedDevice008d5 => 'Unnamed device';

  @override
  String get l6Memberc1ee9 => 'Member';

  @override
  String get l6RuleReminders79233 => 'Rule reminders';

  @override
  String get l6Latest9d501 => '(latest)';

  @override
  String l6IncubatingArg07051b(String arg0) {
    return ' · Incubating: $arg0';
  }

  @override
  String l6RejectedArg02115d(String arg0) {
    return ' · Rejected: $arg0';
  }

  @override
  String l6Arg0Unavailable85bdd(String arg0) {
    return ' $arg0 unavailable';
  }

  @override
  String get l6UnlockToVerifycd78c => 'Unlock to verify';

  @override
  String l6WaitingForDependenciesArg08ff9b(String arg0) {
    return ' · Waiting for dependencies: $arg0';
  }

  @override
  String get l6RosterResentdba01 => ' · Roster resent';

  @override
  String l6VersionMismatchesArg02d8cc(String arg0) {
    return ' · Version mismatches: $arg0';
  }

  @override
  String get l6ReceiptFailedf7d93 => ' · Receipt failed';

  @override
  String get l6OffDirectConnection371c7 => 'Off (direct connection)';

  @override
  String get l6AutomaticPreferDohFallBackTo02ddb =>
      'Automatic (prefer DoH, fall back to direct)';

  @override
  String get l6RequiredDohOnly763c2 => 'Required (DoH only)';

  @override
  String get l6HostedSearchOptimizedForAiWith46b67 =>
      'Hosted search optimized for AI, with a free allowance';

  @override
  String get l6HostedSearchWithAnIndependentPrivacy51ea0 =>
      'Hosted search with an independent, privacy-focused index';

  @override
  String get l6SelfHostedMetasearchNoApiKey0926f =>
      'Self-hosted metasearch; no API key required';

  @override
  String get l6Factsb27b7 => 'Facts';

  @override
  String get l6Decisions13750 => 'Decisions';

  @override
  String get l6Unfinishedb61b0 => 'Unfinished';

  @override
  String get l6IdeasAndDecisionsa2d17 => 'Ideas and decisions';

  @override
  String get l6PeopleAndProjectse73dc => 'People and projects';

  @override
  String get l6FollowUps068c6 => 'Follow-ups';

  @override
  String get l6Helpful2f55f => 'Helpful';

  @override
  String get l6NotRelevanta8361 => 'Not relevant';

  @override
  String get l6ContentContainingProtocolMarkersIsHidden50ad8 =>
      'Content containing protocol markers is hidden from progress. See the original record.';

  @override
  String get l6Excerptfba71 => '… (excerpt)';

  @override
  String get l6ThisStepFailedCheckTheFinal1ac2e =>
      'This step failed. Check the final status and review receipt; it will not run again automatically.';

  @override
  String get l6NoCompletionReceiptCouldBeVerifiede00f1 =>
      'No completion receipt could be verified. Check the actual result; this step will not repeat automatically.';

  @override
  String get l6SeeTheReviewCardBelowFord5a60 =>
      'See the review card below for the proposal, target revision, and accept or reject actions.';

  @override
  String l6EvidenceAtRetrievalArg0cb747(String arg0) {
    return 'Evidence at retrieval · $arg0';
  }

  @override
  String l6SourceArg0VersionArg1SnapshotArg2d0032(
    String arg0,
    String arg1,
    String arg2,
  ) {
    return 'Source: $arg0 · Version: $arg1 · Snapshot: $arg2';
  }

  @override
  String l6PageHashAtRetrievalArg05808a(String arg0) {
    return 'Page hash at retrieval: $arg0';
  }

  @override
  String l6UserRecordArg0Arg1902d4(String arg0, String arg1) {
    return 'User record $arg0: $arg1';
  }

  @override
  String l6UnverifiedPageArg01a6ef(String arg0) {
    return 'Unverified page · $arg0';
  }

  @override
  String get l6NoEvidenceExcerptsWereReturned013a1 =>
      'No evidence excerpts were returned.';

  @override
  String get l6TheseAreTheResultsReturnedBy46276 =>
      'These are the results returned by this query. Sources may change later; final-answer citations are verified separately.';

  @override
  String get l6GeneratingAReplyItContinuesWhence0b9 =>
      'Generating a reply. It continues when you switch away and stops when finished.';

  @override
  String get l6CouldNotLoadGenerationStatusReconnect2eabb =>
      'Could not load generation status. Reconnect to check; generation will not restart automatically.';

  @override
  String get l6TheStartResultIsUnconfirmedReconnecting3cc5d =>
      'The start result is unconfirmed. Reconnecting checks the same request without saving your input again.';

  @override
  String get l6TheStopRequestIsUnconfirmedReconnectdaf8d =>
      'The stop request is unconfirmed. Reconnect to check its status.';

  @override
  String get l6GenerationStatusIsTemporarilyUnavailableSaved71c5d =>
      'Generation status is temporarily unavailable. Saved input will not be resent.';

  @override
  String get biometricReason =>
      'Verify your fingerprint or face to unlock Elsewhen';

  @override
  String get l6Code33246 => 'Code';

  @override
  String get l6CodeCopied8df90 => 'Code copied';

  @override
  String l6HeadingArg01576c(String arg0) {
    return 'Heading $arg0';
  }

  @override
  String get l6Heading186e3d => 'Heading 1';

  @override
  String get l6Heading222dfe => 'Heading 2';

  @override
  String get l6Heading3516f9 => 'Heading 3';

  @override
  String get l6Boldf670b => 'Bold';

  @override
  String get l6Italicaf5a2 => 'Italic';

  @override
  String get l6Underline9bc18 => 'Underline';

  @override
  String get l6Strikethroughc85de => 'Strikethrough';

  @override
  String get l6InlineCode1a813 => 'Inline code';

  @override
  String get l6BulletedListb9e9a => 'Bulleted list';

  @override
  String get l6NumberedList0deb8 => 'Numbered list';

  @override
  String get l6Quote23c0e => 'Quote';

  @override
  String get l6Mergee5bad => 'Merge';

  @override
  String get l6Split726ca => 'Split';

  @override
  String get l6Reprocess796dd => 'Reprocess';

  @override
  String get l6WhyIsThisExcerptUnreliableOra83f2 =>
      'Why is this excerpt unreliable or unusable? Your reason becomes negative knowledge and will be shown before importing the same source again.';

  @override
  String get l6Discardea489 => 'Discard';

  @override
  String get l6Processing1a5c2 => 'Processing…';

  @override
  String get l6CheckForUpdatesa6df3 => 'Check for updates';

  @override
  String get l6ReprocessThisPaged752f => 'Reprocess this page';

  @override
  String get l6Unknownd9c32 => 'Unknown';

  @override
  String get l6RetryProcessingf8833 => 'Retry processing';

  @override
  String get deviationNoProgress => 'No progress';

  @override
  String get deviationStalled => 'Stalled';

  @override
  String get deviationConflict => 'Conflicting';

  @override
  String get deviationScopeDrift => 'Goal drift';

  @override
  String get initialSnapshotTransfer => 'Initial snapshot transfer';

  @override
  String get legacyPeopleProjects => 'People / projects';

  @override
  String get legacyArtifacts => 'Artifacts';

  @override
  String get legacyPlanPending => 'Pending';

  @override
  String get legacyPlanActive => 'In progress';

  @override
  String get legacyNewConversation => 'New conversation';

  @override
  String get reprocessingResult =>
      'Processing finished. View the result for details.';

  @override
  String get processingDetails => 'Processing details';

  @override
  String get errorKnowledgeSourceChanged =>
      'The source changed or was rejected. Generate a new proposal.';

  @override
  String get connectionSucceeded => 'Connection successful';

  @override
  String syncLockedBacklog(int count, String size) {
    return 'Sync paused: $count queued · $size. Unlock this device’s key to resume.';
  }

  @override
  String syncAwaitingPeer(int count, String size) {
    return 'Sync delayed: $count unconfirmed · $size. Retrying.';
  }

  @override
  String get syncGoUnlock => 'Unlock';

  @override
  String legacyOperationFailed(String action) {
    return '$action failed. Please try again.';
  }

  @override
  String pageReprocessed(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count source events',
      one: '$count source event',
    );
    return 'Reprocessed the page using $_temp0.';
  }

  @override
  String get pageHumanProtected =>
      'Human edits are protected. To revise this page with AI, create and review a proposal.';

  @override
  String get processingNotes => 'Processing notes';

  @override
  String get queueDetailPending => 'Waiting for background processing';

  @override
  String get queueDetailRunning => 'Processing';

  @override
  String get queueDetailWaiting =>
      'Waiting for source reading or upstream review';

  @override
  String get queueDetailRetry =>
      'Processing failed; waiting for automatic retry';

  @override
  String get queueDetailSkipped =>
      'Skipped; view the processing notes for the reason';

  @override
  String get queueDetailCompleted => 'Processing completed';

  @override
  String get aiReplyFailedGeneric =>
      'AI reply failed. Try again or view the error details.';

  @override
  String get manualLabel => 'Manual';

  @override
  String get automaticLabel => 'Automatic';

  @override
  String get activeProviderSuffix => ' (active)';

  @override
  String nodesAdvanced(int count) {
    return 'Advanced $count nodes';
  }

  @override
  String get errorKnowledgeReviewChanged =>
      'The page, sources or rule settings changed. Review the proposal again.';

  @override
  String get digestReadingPhase => 'Reading long event';

  @override
  String get digestMergingPhase => 'Merging completed segments';

  @override
  String get digestReadyPhase => 'Full event read · awaiting final processing';

  @override
  String get digestCancelledPhase =>
      'Reading paused after classification changed';

  @override
  String digestReadingProgress(
    String read,
    String total,
    String leaves,
    String merges,
  ) {
    return '$read / $total characters read · segments: $leaves · merges: $merges';
  }

  @override
  String get digestReadingBudgetHelp =>
      'Waiting for a provider with enough context capacity. Check the context window, output reserve and custom instructions. Saved reading progress is retained.';

  @override
  String get digestReadingRetryHelp =>
      'This step did not finish. It will retry automatically; saved reading progress is retained.';

  @override
  String get digestReadingLimitHelp =>
      'The event or reading cache reached the processing limit. Saved text and reading progress are retained.';

  @override
  String get promptCompassContract => 'Goal compass assessment contract';

  @override
  String get compassEvidence => 'View evidence';

  @override
  String get compassStillProgressing => 'Still progressing';

  @override
  String get compassDirectionChanged => 'Direction changed';

  @override
  String get compassFeedbackSaved =>
      'Feedback saved; future checks will respect your correction';

  @override
  String get compassReview => 'Quick review';

  @override
  String get compassReviewSeed =>
      'Let’s briefly review whether this plan still fits. The check-in was based on: ';

  @override
  String get compassEvidenceNotice =>
      'These are the user record excerpts used for this assessment. Original records remain unchanged.';

  @override
  String get compassCorrectionNote =>
      'Add progress or a change of plans (optional)';

  @override
  String get compassGentleCheck => 'Does the current plan still fit?';

  @override
  String get wikiAddedEvidence => 'New evidence in this update';

  @override
  String get wikiRetainedEvidence =>
      'Existing evidence will be kept. The new evidence is checked again when you confirm.';

  @override
  String get wikiApplicabilityCheck =>
      'Check whether the conditions and limits still fit.';

  @override
  String get datasetCleanupTitle => 'Clean up old dataset copies';

  @override
  String get datasetCleanupHelp =>
      'Only completed, unused and unselected copies are listed. Lock sync before cleanup. The current dataset, default database and join staging are protected.';

  @override
  String get datasetCleanupRefresh => 'Refresh copies';

  @override
  String get datasetCleanupEmpty => 'No old copies to clean up.';

  @override
  String get datasetCleanupReview => 'Review cleanup';

  @override
  String get datasetCleanupConfirmTitle => 'Delete this local dataset copy?';

  @override
  String get datasetCleanupWarning =>
      'This permanently deletes all local data in this copy and its local sync key. This cannot be undone. Other devices, remote data and the device roster are unchanged. Back up any unsynced content first. Interrupted cleanup can be retried.';

  @override
  String get datasetCleanupDelete => 'Permanently delete local copy';

  @override
  String datasetCleanupSize(String bytes) {
    return 'Local files: $bytes bytes';
  }

  @override
  String get datasetCleanupPending =>
      'Cleanup incomplete (files or local key). Retry to finish.';

  @override
  String get datasetCleanupRetry => 'Resume cleanup';

  @override
  String get datasetCleanupFailed =>
      'Cleanup is incomplete. Lock sync, close other app processes using this copy, then refresh and retry. Unfinished file or key cleanup remains pending.';
}
