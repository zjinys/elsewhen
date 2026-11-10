import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_zh.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('zh'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In zh, this message translates to:
  /// **'Elsewhen'**
  String get appTitle;

  /// No description provided for @initializationFailed.
  ///
  /// In zh, this message translates to:
  /// **'初始化失败'**
  String get initializationFailed;

  /// No description provided for @initializationError.
  ///
  /// In zh, this message translates to:
  /// **'初始化失败：{error}'**
  String initializationError(String error);

  /// No description provided for @startingApp.
  ///
  /// In zh, this message translates to:
  /// **'正在启动…'**
  String get startingApp;

  /// No description provided for @language.
  ///
  /// In zh, this message translates to:
  /// **'语言'**
  String get language;

  /// No description provided for @followSystem.
  ///
  /// In zh, this message translates to:
  /// **'跟随系统'**
  String get followSystem;

  /// No description provided for @chinese.
  ///
  /// In zh, this message translates to:
  /// **'简体中文'**
  String get chinese;

  /// No description provided for @english.
  ///
  /// In zh, this message translates to:
  /// **'English'**
  String get english;

  /// No description provided for @save.
  ///
  /// In zh, this message translates to:
  /// **'保存'**
  String get save;

  /// No description provided for @saving.
  ///
  /// In zh, this message translates to:
  /// **'保存中…'**
  String get saving;

  /// No description provided for @cancel.
  ///
  /// In zh, this message translates to:
  /// **'取消'**
  String get cancel;

  /// No description provided for @delete.
  ///
  /// In zh, this message translates to:
  /// **'删除'**
  String get delete;

  /// No description provided for @edit.
  ///
  /// In zh, this message translates to:
  /// **'编辑'**
  String get edit;

  /// No description provided for @add.
  ///
  /// In zh, this message translates to:
  /// **'添加'**
  String get add;

  /// No description provided for @close.
  ///
  /// In zh, this message translates to:
  /// **'关闭'**
  String get close;

  /// No description provided for @confirm.
  ///
  /// In zh, this message translates to:
  /// **'确认'**
  String get confirm;

  /// No description provided for @retry.
  ///
  /// In zh, this message translates to:
  /// **'重试'**
  String get retry;

  /// No description provided for @settings.
  ///
  /// In zh, this message translates to:
  /// **'设置'**
  String get settings;

  /// No description provided for @backToMain.
  ///
  /// In zh, this message translates to:
  /// **'返回主界面'**
  String get backToMain;

  /// No description provided for @conversation.
  ///
  /// In zh, this message translates to:
  /// **'对话'**
  String get conversation;

  /// No description provided for @knowledge.
  ///
  /// In zh, this message translates to:
  /// **'知识'**
  String get knowledge;

  /// No description provided for @goals.
  ///
  /// In zh, this message translates to:
  /// **'目标'**
  String get goals;

  /// No description provided for @review.
  ///
  /// In zh, this message translates to:
  /// **'回顾'**
  String get review;

  /// No description provided for @sync.
  ///
  /// In zh, this message translates to:
  /// **'同步'**
  String get sync;

  /// No description provided for @today.
  ///
  /// In zh, this message translates to:
  /// **'今天'**
  String get today;

  /// No description provided for @recentlyUsed.
  ///
  /// In zh, this message translates to:
  /// **'最近使用'**
  String get recentlyUsed;

  /// No description provided for @noRecentPages.
  ///
  /// In zh, this message translates to:
  /// **'还没有最近使用的页面'**
  String get noRecentPages;

  /// No description provided for @conversationLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'对话加载失败'**
  String get conversationLoadFailed;

  /// No description provided for @lastSevenDays.
  ///
  /// In zh, this message translates to:
  /// **'近 7 天'**
  String get lastSevenDays;

  /// No description provided for @rejectedSources.
  ///
  /// In zh, this message translates to:
  /// **'已否定（负知识）'**
  String get rejectedSources;

  /// No description provided for @noRejectedSources.
  ///
  /// In zh, this message translates to:
  /// **'还没有否定的来源。'**
  String get noRejectedSources;

  /// No description provided for @models.
  ///
  /// In zh, this message translates to:
  /// **'模型'**
  String get models;

  /// No description provided for @appearance.
  ///
  /// In zh, this message translates to:
  /// **'外观'**
  String get appearance;

  /// No description provided for @integrations.
  ///
  /// In zh, this message translates to:
  /// **'集成'**
  String get integrations;

  /// No description provided for @network.
  ///
  /// In zh, this message translates to:
  /// **'网络'**
  String get network;

  /// No description provided for @skills.
  ///
  /// In zh, this message translates to:
  /// **'技能'**
  String get skills;

  /// No description provided for @habits.
  ///
  /// In zh, this message translates to:
  /// **'习惯'**
  String get habits;

  /// No description provided for @data.
  ///
  /// In zh, this message translates to:
  /// **'数据'**
  String get data;

  /// No description provided for @rules.
  ///
  /// In zh, this message translates to:
  /// **'规则'**
  String get rules;

  /// No description provided for @security.
  ///
  /// In zh, this message translates to:
  /// **'安全'**
  String get security;

  /// No description provided for @aiProviders.
  ///
  /// In zh, this message translates to:
  /// **'AI 提供商'**
  String get aiProviders;

  /// No description provided for @memoryStrategy.
  ///
  /// In zh, this message translates to:
  /// **'记忆策略'**
  String get memoryStrategy;

  /// No description provided for @prompts.
  ///
  /// In zh, this message translates to:
  /// **'提示词'**
  String get prompts;

  /// No description provided for @memorySaved.
  ///
  /// In zh, this message translates to:
  /// **'记忆策略已保存'**
  String get memorySaved;

  /// No description provided for @strategyType.
  ///
  /// In zh, this message translates to:
  /// **'策略类型'**
  String get strategyType;

  /// No description provided for @simpleMemory.
  ///
  /// In zh, this message translates to:
  /// **'简单记忆 (最近 N 条)'**
  String get simpleMemory;

  /// No description provided for @slidingWindowMemory.
  ///
  /// In zh, this message translates to:
  /// **'滑动窗口 (Token 限制)'**
  String get slidingWindowMemory;

  /// No description provided for @maximumMessages.
  ///
  /// In zh, this message translates to:
  /// **'最大消息数'**
  String get maximumMessages;

  /// No description provided for @memoryBudget.
  ///
  /// In zh, this message translates to:
  /// **'对话记忆预算（可选）'**
  String get memoryBudget;

  /// No description provided for @memoryBudgetHint.
  ///
  /// In zh, this message translates to:
  /// **'留空按 Provider 上下文窗口推算'**
  String get memoryBudgetHint;

  /// No description provided for @themeMode.
  ///
  /// In zh, this message translates to:
  /// **'主题模式'**
  String get themeMode;

  /// No description provided for @themeColors.
  ///
  /// In zh, this message translates to:
  /// **'主题配色'**
  String get themeColors;

  /// No description provided for @lightTheme.
  ///
  /// In zh, this message translates to:
  /// **'浅色'**
  String get lightTheme;

  /// No description provided for @darkTheme.
  ///
  /// In zh, this message translates to:
  /// **'深色'**
  String get darkTheme;

  /// No description provided for @amberTheme.
  ///
  /// In zh, this message translates to:
  /// **'琥珀'**
  String get amberTheme;

  /// No description provided for @indigoTheme.
  ///
  /// In zh, this message translates to:
  /// **'靛蓝'**
  String get indigoTheme;

  /// No description provided for @aquaTheme.
  ///
  /// In zh, this message translates to:
  /// **'青碧'**
  String get aquaTheme;

  /// No description provided for @violetTheme.
  ///
  /// In zh, this message translates to:
  /// **'郁金'**
  String get violetTheme;

  /// No description provided for @bodyFontSize.
  ///
  /// In zh, this message translates to:
  /// **'正文字号'**
  String get bodyFontSize;

  /// No description provided for @contentFont.
  ///
  /// In zh, this message translates to:
  /// **'内容字体'**
  String get contentFont;

  /// No description provided for @contentFontDescription.
  ///
  /// In zh, this message translates to:
  /// **'作用于知识库正文、对话消息等内容区；界面文字使用固定字体'**
  String get contentFontDescription;

  /// No description provided for @systemDefault.
  ///
  /// In zh, this message translates to:
  /// **'系统默认'**
  String get systemDefault;

  /// No description provided for @desktopWindow.
  ///
  /// In zh, this message translates to:
  /// **'桌面窗口'**
  String get desktopWindow;

  /// No description provided for @noTrayAvailable.
  ///
  /// In zh, this message translates to:
  /// **'当前环境没有可用的系统托盘，关闭窗口将直接退出应用。'**
  String get noTrayAvailable;

  /// No description provided for @onWindowClose.
  ///
  /// In zh, this message translates to:
  /// **'关闭窗口时'**
  String get onWindowClose;

  /// No description provided for @minimizeToTray.
  ///
  /// In zh, this message translates to:
  /// **'最小化到托盘'**
  String get minimizeToTray;

  /// No description provided for @quitApp.
  ///
  /// In zh, this message translates to:
  /// **'退出应用'**
  String get quitApp;

  /// No description provided for @stayInTrayDescription.
  ///
  /// In zh, this message translates to:
  /// **'应用留在后台托盘，可随时从托盘菜单唤回'**
  String get stayInTrayDescription;

  /// No description provided for @quitDescription.
  ///
  /// In zh, this message translates to:
  /// **'彻底退出应用，托盘图标一并移除'**
  String get quitDescription;

  /// No description provided for @pending.
  ///
  /// In zh, this message translates to:
  /// **'待处理'**
  String get pending;

  /// No description provided for @processing.
  ///
  /// In zh, this message translates to:
  /// **'处理中'**
  String get processing;

  /// No description provided for @waitingRetry.
  ///
  /// In zh, this message translates to:
  /// **'等待重试'**
  String get waitingRetry;

  /// No description provided for @completed.
  ///
  /// In zh, this message translates to:
  /// **'已完成'**
  String get completed;

  /// No description provided for @failed.
  ///
  /// In zh, this message translates to:
  /// **'失败'**
  String get failed;

  /// No description provided for @skipped.
  ///
  /// In zh, this message translates to:
  /// **'跳过'**
  String get skipped;

  /// No description provided for @lastSuccess.
  ///
  /// In zh, this message translates to:
  /// **'最近成功'**
  String get lastSuccess;

  /// No description provided for @analysisStatusFailed.
  ///
  /// In zh, this message translates to:
  /// **'分析队列状态读取失败'**
  String get analysisStatusFailed;

  /// No description provided for @digestStatusFailed.
  ///
  /// In zh, this message translates to:
  /// **'知识消化状态读取失败'**
  String get digestStatusFailed;

  /// No description provided for @digestDescription.
  ///
  /// In zh, this message translates to:
  /// **'记录的事件会在后台自动整理进知识库；失败项冷却后会自动重试。'**
  String get digestDescription;

  /// No description provided for @viewQueueLogs.
  ///
  /// In zh, this message translates to:
  /// **'查看队列与运行日志'**
  String get viewQueueLogs;

  /// No description provided for @exporting.
  ///
  /// In zh, this message translates to:
  /// **'导出中…'**
  String get exporting;

  /// No description provided for @chooseExportFolder.
  ///
  /// In zh, this message translates to:
  /// **'选择目录并导出'**
  String get chooseExportFolder;

  /// No description provided for @exportFolder.
  ///
  /// In zh, this message translates to:
  /// **'选择导出目录'**
  String get exportFolder;

  /// No description provided for @storageExportUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'当前存储后端不支持导出'**
  String get storageExportUnsupported;

  /// No description provided for @loading.
  ///
  /// In zh, this message translates to:
  /// **'读取中…'**
  String get loading;

  /// No description provided for @compacting.
  ///
  /// In zh, this message translates to:
  /// **'整理中…'**
  String get compacting;

  /// No description provided for @compactNow.
  ///
  /// In zh, this message translates to:
  /// **'立即整理'**
  String get compactNow;

  /// No description provided for @databaseAlreadyCompact.
  ///
  /// In zh, this message translates to:
  /// **'整理完成，库已经很紧凑'**
  String get databaseAlreadyCompact;

  /// No description provided for @selectSecret.
  ///
  /// In zh, this message translates to:
  /// **'选择密钥'**
  String get selectSecret;

  /// No description provided for @noSecretsHint.
  ///
  /// In zh, this message translates to:
  /// **'请先在设置 → 安全 → 密钥管理中添加密钥'**
  String get noSecretsHint;

  /// No description provided for @secretLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'密钥库读取失败，请重试'**
  String get secretLoadFailed;

  /// No description provided for @duplicateProvider.
  ///
  /// In zh, this message translates to:
  /// **'复制 AI Provider'**
  String get duplicateProvider;

  /// No description provided for @addProvider.
  ///
  /// In zh, this message translates to:
  /// **'添加 AI Provider'**
  String get addProvider;

  /// No description provided for @editProvider.
  ///
  /// In zh, this message translates to:
  /// **'编辑 Provider'**
  String get editProvider;

  /// No description provided for @configurationNameRequired.
  ///
  /// In zh, this message translates to:
  /// **'配置名称 *'**
  String get configurationNameRequired;

  /// No description provided for @configurationNameHint.
  ///
  /// In zh, this message translates to:
  /// **'如：主用 GPT'**
  String get configurationNameHint;

  /// No description provided for @providerType.
  ///
  /// In zh, this message translates to:
  /// **'提供商类型'**
  String get providerType;

  /// No description provided for @openAiCompatible.
  ///
  /// In zh, this message translates to:
  /// **'OpenAI 兼容'**
  String get openAiCompatible;

  /// No description provided for @modelRequired.
  ///
  /// In zh, this message translates to:
  /// **'模型 *'**
  String get modelRequired;

  /// No description provided for @secretVault.
  ///
  /// In zh, this message translates to:
  /// **'密钥库'**
  String get secretVault;

  /// No description provided for @manualInput.
  ///
  /// In zh, this message translates to:
  /// **'手动输入'**
  String get manualInput;

  /// No description provided for @apiKeyHintNew.
  ///
  /// In zh, this message translates to:
  /// **'输入 API Key，或点击钥匙选择'**
  String get apiKeyHintNew;

  /// No description provided for @apiKeyHintExisting.
  ///
  /// In zh, this message translates to:
  /// **'留空保持当前密钥不变'**
  String get apiKeyHintExisting;

  /// No description provided for @chooseFromVault.
  ///
  /// In zh, this message translates to:
  /// **'从密钥库选择'**
  String get chooseFromVault;

  /// No description provided for @contextWindow.
  ///
  /// In zh, this message translates to:
  /// **'上下文窗口（token）'**
  String get contextWindow;

  /// No description provided for @contextWindowHint.
  ///
  /// In zh, this message translates to:
  /// **'留空使用保守默认值；中转可填写实测上限'**
  String get contextWindowHint;

  /// No description provided for @maximumOutput.
  ///
  /// In zh, this message translates to:
  /// **'输出上限（token）'**
  String get maximumOutput;

  /// No description provided for @optional.
  ///
  /// In zh, this message translates to:
  /// **'可选'**
  String get optional;

  /// No description provided for @selectSecretFirst.
  ///
  /// In zh, this message translates to:
  /// **'请先选择一个密钥'**
  String get selectSecretFirst;

  /// No description provided for @saveProvider.
  ///
  /// In zh, this message translates to:
  /// **'保存 Provider'**
  String get saveProvider;

  /// No description provided for @noProviders.
  ///
  /// In zh, this message translates to:
  /// **'尚未配置任何 AI Provider，点击右上角「添加」开始。'**
  String get noProviders;

  /// No description provided for @active.
  ///
  /// In zh, this message translates to:
  /// **'激活中'**
  String get active;

  /// No description provided for @deleteConfiguration.
  ///
  /// In zh, this message translates to:
  /// **'删除配置'**
  String get deleteConfiguration;

  /// No description provided for @addSecret.
  ///
  /// In zh, this message translates to:
  /// **'添加密钥'**
  String get addSecret;

  /// No description provided for @editSecret.
  ///
  /// In zh, this message translates to:
  /// **'编辑密钥'**
  String get editSecret;

  /// No description provided for @secretCodeRequired.
  ///
  /// In zh, this message translates to:
  /// **'唯一代号 *'**
  String get secretCodeRequired;

  /// No description provided for @secretCodeCreateHint.
  ///
  /// In zh, this message translates to:
  /// **'字母、数字、下划线或短横线，创建后固定'**
  String get secretCodeCreateHint;

  /// No description provided for @secretCodeEditHint.
  ///
  /// In zh, this message translates to:
  /// **'引用标识不可修改，名称可随时调整'**
  String get secretCodeEditHint;

  /// No description provided for @nameRequired.
  ///
  /// In zh, this message translates to:
  /// **'名称 *'**
  String get nameRequired;

  /// No description provided for @secretNameHint.
  ///
  /// In zh, this message translates to:
  /// **'例如：个人 GitHub Token'**
  String get secretNameHint;

  /// No description provided for @secretValueRequired.
  ///
  /// In zh, this message translates to:
  /// **'密钥值 *'**
  String get secretValueRequired;

  /// No description provided for @newSecretValueOptional.
  ///
  /// In zh, this message translates to:
  /// **'新密钥值（可选）'**
  String get newSecretValueOptional;

  /// No description provided for @keepSecretValueHint.
  ///
  /// In zh, this message translates to:
  /// **'留空保留原密钥，仅更新名称'**
  String get keepSecretValueHint;

  /// No description provided for @replaceMatchingCredentials.
  ///
  /// In zh, this message translates to:
  /// **'将相同的现有凭据替换为引用'**
  String get replaceMatchingCredentials;

  /// No description provided for @matchingCredentialsHint.
  ///
  /// In zh, this message translates to:
  /// **'精确匹配 MCP 环境变量、Header 和搜索 Key'**
  String get matchingCredentialsHint;

  /// No description provided for @notReferenced.
  ///
  /// In zh, this message translates to:
  /// **'尚未引用'**
  String get notReferenced;

  /// No description provided for @nameRequiredError.
  ///
  /// In zh, this message translates to:
  /// **'请填写名称'**
  String get nameRequiredError;

  /// No description provided for @secretFieldsRequired.
  ///
  /// In zh, this message translates to:
  /// **'请填写唯一代号和密钥值'**
  String get secretFieldsRequired;

  /// No description provided for @deleteSecretTitle.
  ///
  /// In zh, this message translates to:
  /// **'删除密钥？'**
  String get deleteSecretTitle;

  /// No description provided for @secretVaultDescription.
  ///
  /// In zh, this message translates to:
  /// **'密钥仅保存在本机。在 MCP 环境变量、Header 或搜索 API Key 中填写 \'{{secret:代号}}\' 即可引用。'**
  String get secretVaultDescription;

  /// No description provided for @promptBody.
  ///
  /// In zh, this message translates to:
  /// **'提示词正文'**
  String get promptBody;

  /// No description provided for @promptEditHint.
  ///
  /// In zh, this message translates to:
  /// **'保存只产生草稿，启用影响下一次运行。协议与执行权限由程序约束。'**
  String get promptEditHint;

  /// No description provided for @promptReadOnlyHint.
  ///
  /// In zh, this message translates to:
  /// **'输出协议与执行纪律只读。'**
  String get promptReadOnlyHint;

  /// No description provided for @promptDraftSaved.
  ///
  /// In zh, this message translates to:
  /// **'草稿已保存，可点击启用。'**
  String get promptDraftSaved;

  /// No description provided for @viewDefault.
  ///
  /// In zh, this message translates to:
  /// **'查看默认'**
  String get viewDefault;

  /// No description provided for @settingsEditNote.
  ///
  /// In zh, this message translates to:
  /// **'设置页编辑'**
  String get settingsEditNote;

  /// No description provided for @saveDraft.
  ///
  /// In zh, this message translates to:
  /// **'保存草稿'**
  String get saveDraft;

  /// No description provided for @activateDraft.
  ///
  /// In zh, this message translates to:
  /// **'启用草稿'**
  String get activateDraft;

  /// No description provided for @builtin.
  ///
  /// In zh, this message translates to:
  /// **'内置'**
  String get builtin;

  /// No description provided for @custom.
  ///
  /// In zh, this message translates to:
  /// **'自定义'**
  String get custom;

  /// No description provided for @currentlyActive.
  ///
  /// In zh, this message translates to:
  /// **'当前生效'**
  String get currentlyActive;

  /// No description provided for @activateRevision.
  ///
  /// In zh, this message translates to:
  /// **'启用此版本'**
  String get activateRevision;

  /// No description provided for @recentRunVersions.
  ///
  /// In zh, this message translates to:
  /// **'最近20次运行版本'**
  String get recentRunVersions;

  /// No description provided for @promptLocalDescription.
  ///
  /// In zh, this message translates to:
  /// **'提示词保存在本机。角色和处理方法支持修订与回滚；固定协议只读。当前使用中文资源。'**
  String get promptLocalDescription;

  /// No description provided for @loadPrompts.
  ///
  /// In zh, this message translates to:
  /// **'加载提示词'**
  String get loadPrompts;

  /// No description provided for @viewRunVersions.
  ///
  /// In zh, this message translates to:
  /// **'查看运行版本'**
  String get viewRunVersions;

  /// No description provided for @editable.
  ///
  /// In zh, this message translates to:
  /// **'可编辑'**
  String get editable;

  /// No description provided for @readOnlyContract.
  ///
  /// In zh, this message translates to:
  /// **'只读协议'**
  String get readOnlyContract;

  /// No description provided for @revisionHistory.
  ///
  /// In zh, this message translates to:
  /// **'版本历史与回滚'**
  String get revisionHistory;

  /// No description provided for @restoreDefault.
  ///
  /// In zh, this message translates to:
  /// **'恢复默认'**
  String get restoreDefault;

  /// No description provided for @syncPlatformUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'同步在此平台暂不可用'**
  String get syncPlatformUnavailable;

  /// No description provided for @todos.
  ///
  /// In zh, this message translates to:
  /// **'待办'**
  String get todos;

  /// No description provided for @drafts.
  ///
  /// In zh, this message translates to:
  /// **'草稿'**
  String get drafts;

  /// No description provided for @search.
  ///
  /// In zh, this message translates to:
  /// **'搜索'**
  String get search;

  /// No description provided for @lockedTitle.
  ///
  /// In zh, this message translates to:
  /// **'Elsewhen 已锁定'**
  String get lockedTitle;

  /// No description provided for @incorrectPassword.
  ///
  /// In zh, this message translates to:
  /// **'密码不正确'**
  String get incorrectPassword;

  /// No description provided for @secureStorageUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'安全存储暂不可用，请稍后重试'**
  String get secureStorageUnavailable;

  /// No description provided for @passwordInputHint.
  ///
  /// In zh, this message translates to:
  /// **'输入打开密码，输完按回车'**
  String get passwordInputHint;

  /// No description provided for @verifying.
  ///
  /// In zh, this message translates to:
  /// **'验证中…'**
  String get verifying;

  /// No description provided for @biometricUnlock.
  ///
  /// In zh, this message translates to:
  /// **'使用指纹/面容解锁'**
  String get biometricUnlock;

  /// No description provided for @usePinEntry.
  ///
  /// In zh, this message translates to:
  /// **'改用 4 格输入'**
  String get usePinEntry;

  /// No description provided for @legacyPasswordEntry.
  ///
  /// In zh, this message translates to:
  /// **'旧密码不是 4 位？换输入方式'**
  String get legacyPasswordEntry;

  /// No description provided for @sectionLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'{section}读取失败：{error}'**
  String sectionLoadFailed(String section, String error);

  /// No description provided for @secretUsedBy.
  ///
  /// In zh, this message translates to:
  /// **'使用位置：{references}'**
  String secretUsedBy(String references);

  /// No description provided for @deleteSecretConfirmation.
  ///
  /// In zh, this message translates to:
  /// **'删除 {name}？'**
  String deleteSecretConfirmation(String name);

  /// No description provided for @secretStillUsedBy.
  ///
  /// In zh, this message translates to:
  /// **'仍被 {references} 引用，删除后相关服务将无法连接。'**
  String secretStillUsedBy(String references);

  /// No description provided for @deleteProviderConfirmation.
  ///
  /// In zh, this message translates to:
  /// **'确定删除 Provider「{name}」吗？{warning}'**
  String deleteProviderConfirmation(String name, String warning);

  /// No description provided for @activeProviderDeletionWarning.
  ///
  /// In zh, this message translates to:
  /// **'该配置正处于激活状态，删除后剩余第一个配置将自动激活。'**
  String get activeProviderDeletionWarning;

  /// No description provided for @providerCopyName.
  ///
  /// In zh, this message translates to:
  /// **'{name} 副本'**
  String providerCopyName(String name);

  /// No description provided for @switchProvider.
  ///
  /// In zh, this message translates to:
  /// **'切换 Provider'**
  String get switchProvider;

  /// No description provided for @deleteProvider.
  ///
  /// In zh, this message translates to:
  /// **'删除 Provider'**
  String get deleteProvider;

  /// No description provided for @promptHistoryTitle.
  ///
  /// In zh, this message translates to:
  /// **'{title} · 最近30个版本'**
  String promptHistoryTitle(String title);

  /// No description provided for @approximateTokens.
  ///
  /// In zh, this message translates to:
  /// **'约 {tokens} token'**
  String approximateTokens(int tokens);

  /// No description provided for @promptPersonalPolicy.
  ///
  /// In zh, this message translates to:
  /// **'对话角色与风格'**
  String get promptPersonalPolicy;

  /// No description provided for @promptChatContract.
  ///
  /// In zh, this message translates to:
  /// **'对话协议与执行纪律'**
  String get promptChatContract;

  /// No description provided for @promptSummaryContract.
  ///
  /// In zh, this message translates to:
  /// **'历史摘要协议'**
  String get promptSummaryContract;

  /// No description provided for @promptReadingContract.
  ///
  /// In zh, this message translates to:
  /// **'全文阅读协议'**
  String get promptReadingContract;

  /// No description provided for @promptDigestContract.
  ///
  /// In zh, this message translates to:
  /// **'知识消化协议'**
  String get promptDigestContract;

  /// No description provided for @promptSummaryPolicy.
  ///
  /// In zh, this message translates to:
  /// **'历史摘要重点'**
  String get promptSummaryPolicy;

  /// No description provided for @promptReadingPolicy.
  ///
  /// In zh, this message translates to:
  /// **'全文阅读重点'**
  String get promptReadingPolicy;

  /// No description provided for @promptDigestPolicy.
  ///
  /// In zh, this message translates to:
  /// **'知识消化方法'**
  String get promptDigestPolicy;

  /// No description provided for @promptAnalysisPolicy.
  ///
  /// In zh, this message translates to:
  /// **'事件分析方法'**
  String get promptAnalysisPolicy;

  /// No description provided for @lastFailure.
  ///
  /// In zh, this message translates to:
  /// **'最近失败（{time}）：{error}'**
  String lastFailure(String time, String error);

  /// No description provided for @exportSuccess.
  ///
  /// In zh, this message translates to:
  /// **'已导出 {count} 个知识页到 {folder}'**
  String exportSuccess(int count, String folder);

  /// No description provided for @wikiExportDescription.
  ///
  /// In zh, this message translates to:
  /// **'把知识页导出为带元数据的 Markdown 文件（Obsidian 等通用工具可直接打开）；派生产物与归档/拒绝页不导出，同名覆盖可重复执行。'**
  String get wikiExportDescription;

  /// No description provided for @databaseSize.
  ///
  /// In zh, this message translates to:
  /// **'库文件 {file} MB，其中可回收 {free} MB（历史删除残留）{advice}'**
  String databaseSize(String file, String free, String advice);

  /// No description provided for @compactionRecommended.
  ///
  /// In zh, this message translates to:
  /// **'。建议整理。'**
  String get compactionRecommended;

  /// No description provided for @databaseSpaceReclaimed.
  ///
  /// In zh, this message translates to:
  /// **'整理完成，回收了 {size} MB'**
  String databaseSpaceReclaimed(String size);

  /// No description provided for @noAiUsage.
  ///
  /// In zh, this message translates to:
  /// **'暂无 AI 调用记录。发起对话并生成 AI 回复后，这里会按天统计 token 用量。'**
  String get noAiUsage;

  /// No description provided for @last30DaysTokens.
  ///
  /// In zh, this message translates to:
  /// **'近 30 天 Total'**
  String get last30DaysTokens;

  /// No description provided for @totalCalls.
  ///
  /// In zh, this message translates to:
  /// **'累计调用'**
  String get totalCalls;

  /// No description provided for @callCount.
  ///
  /// In zh, this message translates to:
  /// **'{count} 次'**
  String callCount(int count);

  /// No description provided for @listSeparator.
  ///
  /// In zh, this message translates to:
  /// **'、'**
  String get listSeparator;

  /// No description provided for @providerConfiguration.
  ///
  /// In zh, this message translates to:
  /// **'AI Provider 配置'**
  String get providerConfiguration;

  /// No description provided for @dailyTokenUsage.
  ///
  /// In zh, this message translates to:
  /// **'每日 Token 用量'**
  String get dailyTokenUsage;

  /// No description provided for @analysisQueue.
  ///
  /// In zh, this message translates to:
  /// **'事件分析队列'**
  String get analysisQueue;

  /// No description provided for @knowledgeProcessing.
  ///
  /// In zh, this message translates to:
  /// **'知识消化'**
  String get knowledgeProcessing;

  /// No description provided for @ruleLibrary.
  ///
  /// In zh, this message translates to:
  /// **'规则库'**
  String get ruleLibrary;

  /// No description provided for @tweetFetchService.
  ///
  /// In zh, this message translates to:
  /// **'推文抓取服务'**
  String get tweetFetchService;

  /// No description provided for @providerInUse.
  ///
  /// In zh, this message translates to:
  /// **'对话使用此配置'**
  String get providerInUse;

  /// No description provided for @activateProvider.
  ///
  /// In zh, this message translates to:
  /// **'设为激活'**
  String get activateProvider;

  /// No description provided for @duplicateProviderTooltip.
  ///
  /// In zh, this message translates to:
  /// **'复制配置（API Key 需重新输入）'**
  String get duplicateProviderTooltip;

  /// No description provided for @keyNotConfigured.
  ///
  /// In zh, this message translates to:
  /// **'未配置 Key'**
  String get keyNotConfigured;

  /// No description provided for @notConfigured.
  ///
  /// In zh, this message translates to:
  /// **'未配置'**
  String get notConfigured;

  /// No description provided for @providerNameMissing.
  ///
  /// In zh, this message translates to:
  /// **'配置名称不能为空'**
  String get providerNameMissing;

  /// No description provided for @providerFieldsMissing.
  ///
  /// In zh, this message translates to:
  /// **'base_url 和 model 均不能为空'**
  String get providerFieldsMissing;

  /// No description provided for @apiKeyRequiredForNew.
  ///
  /// In zh, this message translates to:
  /// **'新建配置时必须填写 API Key'**
  String get apiKeyRequiredForNew;

  /// No description provided for @chooseFont.
  ///
  /// In zh, this message translates to:
  /// **'选择字体'**
  String get chooseFont;

  /// No description provided for @searchFonts.
  ///
  /// In zh, this message translates to:
  /// **'搜索字体…'**
  String get searchFonts;

  /// No description provided for @clear.
  ///
  /// In zh, this message translates to:
  /// **'清除'**
  String get clear;

  /// No description provided for @followGlobal.
  ///
  /// In zh, this message translates to:
  /// **'跟随全局'**
  String get followGlobal;

  /// No description provided for @useGlobalFont.
  ///
  /// In zh, this message translates to:
  /// **'使用全局字体设置'**
  String get useGlobalFont;

  /// No description provided for @useSystemFont.
  ///
  /// In zh, this message translates to:
  /// **'跟随操作系统，不指定字体'**
  String get useSystemFont;

  /// No description provided for @noMatchingFonts.
  ///
  /// In zh, this message translates to:
  /// **'没有匹配的字体'**
  String get noMatchingFonts;

  /// No description provided for @fontPreview.
  ///
  /// In zh, this message translates to:
  /// **'中文测试 AaBbGg 1234 · '**
  String get fontPreview;

  /// No description provided for @endpointMalformed.
  ///
  /// In zh, this message translates to:
  /// **'Provider 地址格式需要检查；不会自动修正地址或发送连接测试。'**
  String get endpointMalformed;

  /// No description provided for @endpointLoopback.
  ///
  /// In zh, this message translates to:
  /// **'这是本机环回地址：在另一台设备上会指向那台设备本身，而非原设备。同步保留原地址，不会自动探测或改用其他 Provider。'**
  String get endpointLoopback;

  /// No description provided for @endpointUnspecified.
  ///
  /// In zh, this message translates to:
  /// **'这是未指定地址，通常用于服务监听；请确认本机及其他设备实际可访问的 Provider 地址。不会自动替换或探测。'**
  String get endpointUnspecified;

  /// No description provided for @endpointPrivate.
  ///
  /// In zh, this message translates to:
  /// **'这是内网或链路本地地址，其他设备需要相应的网络连接。同步保留原配置，尚未验证可达性，也不会自动发送模型请求。'**
  String get endpointPrivate;

  /// No description provided for @operationFailed.
  ///
  /// In zh, this message translates to:
  /// **'{action}失败，请重试一次'**
  String operationFailed(String action);

  /// No description provided for @localFontCount.
  ///
  /// In zh, this message translates to:
  /// **'本地字体 ({count})'**
  String localFontCount(int count);

  /// No description provided for @secretManagement.
  ///
  /// In zh, this message translates to:
  /// **'密钥管理'**
  String get secretManagement;

  /// No description provided for @appPassword.
  ///
  /// In zh, this message translates to:
  /// **'打开密码'**
  String get appPassword;

  /// No description provided for @openNavigation.
  ///
  /// In zh, this message translates to:
  /// **'打开导航'**
  String get openNavigation;

  /// No description provided for @createTopic.
  ///
  /// In zh, this message translates to:
  /// **'创建主题'**
  String get createTopic;

  /// No description provided for @importUrl.
  ///
  /// In zh, this message translates to:
  /// **'网址导入'**
  String get importUrl;

  /// No description provided for @hideSearch.
  ///
  /// In zh, this message translates to:
  /// **'隐藏搜索'**
  String get hideSearch;

  /// No description provided for @searchAndDate.
  ///
  /// In zh, this message translates to:
  /// **'搜索与日期定位'**
  String get searchAndDate;

  /// No description provided for @aiReplying.
  ///
  /// In zh, this message translates to:
  /// **'回复中'**
  String get aiReplying;

  /// No description provided for @aiReplyingHint.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在生成回复'**
  String get aiReplyingHint;

  /// No description provided for @aiAnalyzing.
  ///
  /// In zh, this message translates to:
  /// **'分析中'**
  String get aiAnalyzing;

  /// No description provided for @aiAnalyzingHint.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在分析记录'**
  String get aiAnalyzingHint;

  /// No description provided for @aiDigesting.
  ///
  /// In zh, this message translates to:
  /// **'消化中'**
  String get aiDigesting;

  /// No description provided for @aiDigestingHint.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在消化新记录'**
  String get aiDigestingHint;

  /// No description provided for @aiOrganizing.
  ///
  /// In zh, this message translates to:
  /// **'整理中'**
  String get aiOrganizing;

  /// No description provided for @aiOrganizingHint.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在整理知识库'**
  String get aiOrganizingHint;

  /// No description provided for @aiChecking.
  ///
  /// In zh, this message translates to:
  /// **'检测中'**
  String get aiChecking;

  /// No description provided for @aiCheckingHint.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在检测周期报告'**
  String get aiCheckingHint;

  /// No description provided for @goalsCount.
  ///
  /// In zh, this message translates to:
  /// **'{count, plural, =0{目标} other{目标（{count}）}}'**
  String goalsCount(int count);

  /// No description provided for @todoCount.
  ///
  /// In zh, this message translates to:
  /// **'待办（{count}）'**
  String todoCount(int count);

  /// No description provided for @locateByDate.
  ///
  /// In zh, this message translates to:
  /// **'按日期定位'**
  String get locateByDate;

  /// No description provided for @searchMessages.
  ///
  /// In zh, this message translates to:
  /// **'搜索你说过的话'**
  String get searchMessages;

  /// No description provided for @clearLocation.
  ///
  /// In zh, this message translates to:
  /// **'清除定位'**
  String get clearLocation;

  /// No description provided for @backToNow.
  ///
  /// In zh, this message translates to:
  /// **'回到现在'**
  String get backToNow;

  /// No description provided for @jumpToDate.
  ///
  /// In zh, this message translates to:
  /// **'跳到日期'**
  String get jumpToDate;

  /// No description provided for @jump.
  ///
  /// In zh, this message translates to:
  /// **'跳转'**
  String get jump;

  /// No description provided for @showEarlierMessages.
  ///
  /// In zh, this message translates to:
  /// **'显示更早的消息'**
  String get showEarlierMessages;

  /// No description provided for @startConversation.
  ///
  /// In zh, this message translates to:
  /// **'选择一个对话开始聊天'**
  String get startConversation;

  /// No description provided for @noMessages.
  ///
  /// In zh, this message translates to:
  /// **'还没有消息'**
  String get noMessages;

  /// No description provided for @startTyping.
  ///
  /// In zh, this message translates to:
  /// **'在下方输入框开始对话'**
  String get startTyping;

  /// No description provided for @conversationCount.
  ///
  /// In zh, this message translates to:
  /// **'对话{count}次'**
  String conversationCount(int count);

  /// No description provided for @todoCountItems.
  ///
  /// In zh, this message translates to:
  /// **'待办{count}项'**
  String todoCountItems(int count);

  /// No description provided for @copyAll.
  ///
  /// In zh, this message translates to:
  /// **'复制全部对话'**
  String get copyAll;

  /// No description provided for @flipPast.
  ///
  /// In zh, this message translates to:
  /// **'翻篇记录'**
  String get flipPast;

  /// No description provided for @topics.
  ///
  /// In zh, this message translates to:
  /// **'主题'**
  String get topics;

  /// No description provided for @systemNotice.
  ///
  /// In zh, this message translates to:
  /// **'系统提示'**
  String get systemNotice;

  /// No description provided for @copyMessage.
  ///
  /// In zh, this message translates to:
  /// **'复制'**
  String get copyMessage;

  /// No description provided for @saveAsTopic.
  ///
  /// In zh, this message translates to:
  /// **'存为主题'**
  String get saveAsTopic;

  /// No description provided for @saveAsTopicDescription.
  ///
  /// In zh, this message translates to:
  /// **'提取标题与正文，确认后存入主题库'**
  String get saveAsTopicDescription;

  /// No description provided for @regenerate.
  ///
  /// In zh, this message translates to:
  /// **'重新生成'**
  String get regenerate;

  /// No description provided for @recordCategory.
  ///
  /// In zh, this message translates to:
  /// **'记录分类'**
  String get recordCategory;

  /// No description provided for @record.
  ///
  /// In zh, this message translates to:
  /// **'记录'**
  String get record;

  /// No description provided for @discussion.
  ///
  /// In zh, this message translates to:
  /// **'讨论'**
  String get discussion;

  /// No description provided for @includeInRecords.
  ///
  /// In zh, this message translates to:
  /// **'纳入记录'**
  String get includeInRecords;

  /// No description provided for @asDiscussion.
  ///
  /// In zh, this message translates to:
  /// **'作为讨论'**
  String get asDiscussion;

  /// No description provided for @reanalyze.
  ///
  /// In zh, this message translates to:
  /// **'重新分析'**
  String get reanalyze;

  /// No description provided for @savedAsRecord.
  ///
  /// In zh, this message translates to:
  /// **'已纳入记录'**
  String get savedAsRecord;

  /// No description provided for @savedAsDiscussion.
  ///
  /// In zh, this message translates to:
  /// **'已作为讨论，不再进入日流和长期事实'**
  String get savedAsDiscussion;

  /// No description provided for @reanalyzeQueued.
  ///
  /// In zh, this message translates to:
  /// **'已加入重新分析队列'**
  String get reanalyzeQueued;

  /// No description provided for @saveMessageAsTopic.
  ///
  /// In zh, this message translates to:
  /// **'把这条内容存为主题'**
  String get saveMessageAsTopic;

  /// No description provided for @thinking.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在思考…'**
  String get thinking;

  /// No description provided for @startGenerating.
  ///
  /// In zh, this message translates to:
  /// **'正在启动生成…'**
  String get startGenerating;

  /// No description provided for @readingGeneration.
  ///
  /// In zh, this message translates to:
  /// **'正在读取生成状态…'**
  String get readingGeneration;

  /// No description provided for @generationFailedSaved.
  ///
  /// In zh, this message translates to:
  /// **'生成失败，输入已保存'**
  String get generationFailedSaved;

  /// No description provided for @generationStopped.
  ///
  /// In zh, this message translates to:
  /// **'已停止生成'**
  String get generationStopped;

  /// No description provided for @generationInterrupted.
  ///
  /// In zh, this message translates to:
  /// **'生成已中断'**
  String get generationInterrupted;

  /// No description provided for @stopping.
  ///
  /// In zh, this message translates to:
  /// **'正在停止…'**
  String get stopping;

  /// No description provided for @stopGeneration.
  ///
  /// In zh, this message translates to:
  /// **'停止生成'**
  String get stopGeneration;

  /// No description provided for @regenerateThis.
  ///
  /// In zh, this message translates to:
  /// **'重新生成本条'**
  String get regenerateThis;

  /// No description provided for @reconnect.
  ///
  /// In zh, this message translates to:
  /// **'重新连接'**
  String get reconnect;

  /// No description provided for @actualToolSteps.
  ///
  /// In zh, this message translates to:
  /// **'实际工具步骤（最近 {count} 项）'**
  String actualToolSteps(int count);

  /// No description provided for @chooseDirectory.
  ///
  /// In zh, this message translates to:
  /// **'选择要导入的目录'**
  String get chooseDirectory;

  /// No description provided for @selectConversation.
  ///
  /// In zh, this message translates to:
  /// **'请先选择一个会话'**
  String get selectConversation;

  /// No description provided for @noSupplementTask.
  ///
  /// In zh, this message translates to:
  /// **'当前没有可补充的任务，请选择保存或排队。'**
  String get noSupplementTask;

  /// No description provided for @sendFailed.
  ///
  /// In zh, this message translates to:
  /// **'发送失败：{error}'**
  String sendFailed(String error);

  /// No description provided for @savedButNotEligible.
  ///
  /// In zh, this message translates to:
  /// **'输入已保存，但生成资格未确认。请从该消息继续生成，不要重新发送。'**
  String get savedButNotEligible;

  /// No description provided for @enteredTopic.
  ///
  /// In zh, this message translates to:
  /// **'已进入主题：{title}'**
  String enteredTopic(String title);

  /// No description provided for @thinkingNext.
  ///
  /// In zh, this message translates to:
  /// **'AI 正在思考，您可以先输入下一条消息…'**
  String get thinkingNext;

  /// No description provided for @inputMessage.
  ///
  /// In zh, this message translates to:
  /// **'输入消息...'**
  String get inputMessage;

  /// No description provided for @closeInputFormat.
  ///
  /// In zh, this message translates to:
  /// **'关闭输入格式'**
  String get closeInputFormat;

  /// No description provided for @addContent.
  ///
  /// In zh, this message translates to:
  /// **'添加内容'**
  String get addContent;

  /// No description provided for @selectDirectory.
  ///
  /// In zh, this message translates to:
  /// **'选择目录'**
  String get selectDirectory;

  /// No description provided for @viewInputFormat.
  ///
  /// In zh, this message translates to:
  /// **'查看输入格式'**
  String get viewInputFormat;

  /// No description provided for @queueNext.
  ///
  /// In zh, this message translates to:
  /// **'保存并排队下一问'**
  String get queueNext;

  /// No description provided for @send.
  ///
  /// In zh, this message translates to:
  /// **'发送'**
  String get send;

  /// No description provided for @recordOnlyHelp.
  ///
  /// In zh, this message translates to:
  /// **'只保存为记录，不请求 AI'**
  String get recordOnlyHelp;

  /// No description provided for @formatDirectory.
  ///
  /// In zh, this message translates to:
  /// **'导入项目或文件'**
  String get formatDirectory;

  /// No description provided for @formatDirectoryHelp.
  ///
  /// In zh, this message translates to:
  /// **'看看 `/path/to/project` 下的项目，导入到知识库'**
  String get formatDirectoryHelp;

  /// No description provided for @formatConfirm.
  ///
  /// In zh, this message translates to:
  /// **'确认 / 取消'**
  String get formatConfirm;

  /// No description provided for @formatConfirmHelp.
  ///
  /// In zh, this message translates to:
  /// **'处理 AI 草案'**
  String get formatConfirmHelp;

  /// No description provided for @mentionPerson.
  ///
  /// In zh, this message translates to:
  /// **'@人名'**
  String get mentionPerson;

  /// No description provided for @tagTopic.
  ///
  /// In zh, this message translates to:
  /// **'#主题名'**
  String get tagTopic;

  /// No description provided for @link.
  ///
  /// In zh, this message translates to:
  /// **'链接'**
  String get link;

  /// No description provided for @directory.
  ///
  /// In zh, this message translates to:
  /// **'目录'**
  String get directory;

  /// No description provided for @unverifiedDraft.
  ///
  /// In zh, this message translates to:
  /// **'未核验草稿 · 不代表操作成功或引用有效'**
  String get unverifiedDraft;

  /// No description provided for @toolRunning.
  ///
  /// In zh, this message translates to:
  /// **'进行中'**
  String get toolRunning;

  /// No description provided for @toolUncertainNoRetry.
  ///
  /// In zh, this message translates to:
  /// **'结果待核实；不会自动重做'**
  String get toolUncertainNoRetry;

  /// No description provided for @toolReadCompleted.
  ///
  /// In zh, this message translates to:
  /// **'查询已完成'**
  String get toolReadCompleted;

  /// No description provided for @toolProposalReady.
  ///
  /// In zh, this message translates to:
  /// **'草稿已生成，等待审阅；尚未执行'**
  String get toolProposalReady;

  /// No description provided for @toolExecuted.
  ///
  /// In zh, this message translates to:
  /// **'操作已执行（业务回执）'**
  String get toolExecuted;

  /// No description provided for @toolFailed.
  ///
  /// In zh, this message translates to:
  /// **'执行失败'**
  String get toolFailed;

  /// No description provided for @toolUncertain.
  ///
  /// In zh, this message translates to:
  /// **'结果待核实'**
  String get toolUncertain;

  /// No description provided for @toolListRules.
  ///
  /// In zh, this message translates to:
  /// **'查询规则'**
  String get toolListRules;

  /// No description provided for @toolListPages.
  ///
  /// In zh, this message translates to:
  /// **'浏览知识页'**
  String get toolListPages;

  /// No description provided for @toolReadPage.
  ///
  /// In zh, this message translates to:
  /// **'读取知识页'**
  String get toolReadPage;

  /// No description provided for @toolUsage.
  ///
  /// In zh, this message translates to:
  /// **'查询用量'**
  String get toolUsage;

  /// No description provided for @toolSearchKnowledge.
  ///
  /// In zh, this message translates to:
  /// **'检索知识库'**
  String get toolSearchKnowledge;

  /// No description provided for @toolReadLink.
  ///
  /// In zh, this message translates to:
  /// **'读取链接'**
  String get toolReadLink;

  /// No description provided for @toolReadWeb.
  ///
  /// In zh, this message translates to:
  /// **'读取网页'**
  String get toolReadWeb;

  /// No description provided for @toolRecordEvent.
  ///
  /// In zh, this message translates to:
  /// **'记录事件'**
  String get toolRecordEvent;

  /// No description provided for @toolDraftPage.
  ///
  /// In zh, this message translates to:
  /// **'草拟知识页'**
  String get toolDraftPage;

  /// No description provided for @toolSkillScript.
  ///
  /// In zh, this message translates to:
  /// **'执行技能脚本'**
  String get toolSkillScript;

  /// No description provided for @toolMcp.
  ///
  /// In zh, this message translates to:
  /// **'MCP 工具调用'**
  String get toolMcp;

  /// No description provided for @toolListTodos.
  ///
  /// In zh, this message translates to:
  /// **'查询待办'**
  String get toolListTodos;

  /// No description provided for @toolDraftTodo.
  ///
  /// In zh, this message translates to:
  /// **'草拟待办'**
  String get toolDraftTodo;

  /// No description provided for @toolDraftRelations.
  ///
  /// In zh, this message translates to:
  /// **'草拟联系人关联'**
  String get toolDraftRelations;

  /// No description provided for @toolExtractRelations.
  ///
  /// In zh, this message translates to:
  /// **'整理联系人关联'**
  String get toolExtractRelations;

  /// No description provided for @toolEventsByDate.
  ///
  /// In zh, this message translates to:
  /// **'按日期查询事件'**
  String get toolEventsByDate;

  /// No description provided for @toolArchiveChats.
  ///
  /// In zh, this message translates to:
  /// **'准备归档对话'**
  String get toolArchiveChats;

  /// No description provided for @toolRenamePage.
  ///
  /// In zh, this message translates to:
  /// **'准备知识页重命名'**
  String get toolRenamePage;

  /// No description provided for @toolImportSource.
  ///
  /// In zh, this message translates to:
  /// **'准备来源导入'**
  String get toolImportSource;

  /// No description provided for @toolInspectDirectory.
  ///
  /// In zh, this message translates to:
  /// **'分析项目目录'**
  String get toolInspectDirectory;

  /// No description provided for @toolImportFiles.
  ///
  /// In zh, this message translates to:
  /// **'准备目录文件导入'**
  String get toolImportFiles;

  /// No description provided for @toolRevisePage.
  ///
  /// In zh, this message translates to:
  /// **'草拟知识页修订'**
  String get toolRevisePage;

  /// No description provided for @toolStep.
  ///
  /// In zh, this message translates to:
  /// **'处理工具步骤'**
  String get toolStep;

  /// No description provided for @gerundPreparing0.
  ///
  /// In zh, this message translates to:
  /// **'翻阅记忆中…'**
  String get gerundPreparing0;

  /// No description provided for @gerundPreparing1.
  ///
  /// In zh, this message translates to:
  /// **'整理思路中…'**
  String get gerundPreparing1;

  /// No description provided for @gerundPreparing2.
  ///
  /// In zh, this message translates to:
  /// **'回顾对话中…'**
  String get gerundPreparing2;

  /// No description provided for @gerundPreparing3.
  ///
  /// In zh, this message translates to:
  /// **'翻检知识库中…'**
  String get gerundPreparing3;

  /// No description provided for @gerundPreparing4.
  ///
  /// In zh, this message translates to:
  /// **'搜集线索中…'**
  String get gerundPreparing4;

  /// No description provided for @gerundPreparing5.
  ///
  /// In zh, this message translates to:
  /// **'梳理上下文中…'**
  String get gerundPreparing5;

  /// No description provided for @gerundAwaiting0.
  ///
  /// In zh, this message translates to:
  /// **'正在酝酿…'**
  String get gerundAwaiting0;

  /// No description provided for @gerundAwaiting1.
  ///
  /// In zh, this message translates to:
  /// **'正在构思…'**
  String get gerundAwaiting1;

  /// No description provided for @gerundAwaiting2.
  ///
  /// In zh, this message translates to:
  /// **'正在推敲…'**
  String get gerundAwaiting2;

  /// No description provided for @gerundAwaiting3.
  ///
  /// In zh, this message translates to:
  /// **'正在斟酌…'**
  String get gerundAwaiting3;

  /// No description provided for @gerundAwaiting4.
  ///
  /// In zh, this message translates to:
  /// **'灵感涌现中…'**
  String get gerundAwaiting4;

  /// No description provided for @gerundAwaiting5.
  ///
  /// In zh, this message translates to:
  /// **'编织回复中…'**
  String get gerundAwaiting5;

  /// No description provided for @gerundAwaiting6.
  ///
  /// In zh, this message translates to:
  /// **'打磨措辞中…'**
  String get gerundAwaiting6;

  /// No description provided for @gerundAwaiting7.
  ///
  /// In zh, this message translates to:
  /// **'抽丝剥茧中…'**
  String get gerundAwaiting7;

  /// No description provided for @copyThisMessage.
  ///
  /// In zh, this message translates to:
  /// **'复制这条消息'**
  String get copyThisMessage;

  /// No description provided for @message.
  ///
  /// In zh, this message translates to:
  /// **'消息'**
  String get message;

  /// No description provided for @me.
  ///
  /// In zh, this message translates to:
  /// **'我'**
  String get me;

  /// No description provided for @updateMessageStatus.
  ///
  /// In zh, this message translates to:
  /// **'更新消息状态'**
  String get updateMessageStatus;

  /// No description provided for @eventAnalyzing.
  ///
  /// In zh, this message translates to:
  /// **'事件正在分析中，请稍后再试'**
  String get eventAnalyzing;

  /// No description provided for @inputGuidePerson.
  ///
  /// In zh, this message translates to:
  /// **'标注联系人'**
  String get inputGuidePerson;

  /// No description provided for @inputGuideTopic.
  ///
  /// In zh, this message translates to:
  /// **'进入主题'**
  String get inputGuideTopic;

  /// No description provided for @inputGuideLink.
  ///
  /// In zh, this message translates to:
  /// **'导入内容'**
  String get inputGuideLink;

  /// No description provided for @aiProviderMissing.
  ///
  /// In zh, this message translates to:
  /// **'尚未配置 AI Provider，请到设置页填写后重试'**
  String get aiProviderMissing;

  /// No description provided for @copiedItem.
  ///
  /// In zh, this message translates to:
  /// **'已复制{label}'**
  String copiedItem(String label);

  /// No description provided for @importDirectoryPrompt.
  ///
  /// In zh, this message translates to:
  /// **'看看 `{path}` 下的项目，导入到知识库'**
  String importDirectoryPrompt(String path);

  /// No description provided for @messageLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'加载消息失败\n{error}'**
  String messageLoadFailed(String error);

  /// No description provided for @aiReplyFailed.
  ///
  /// In zh, this message translates to:
  /// **'AI 回复失败：{error}'**
  String aiReplyFailed(String error);

  /// No description provided for @goalCoAuthorReady.
  ///
  /// In zh, this message translates to:
  /// **'已带目标上下文到「目标」页的 AI 输入框，确认或修改后发送即可共同拆解'**
  String get goalCoAuthorReady;

  /// No description provided for @goalStudio.
  ///
  /// In zh, this message translates to:
  /// **'AI 立项工作室'**
  String get goalStudio;

  /// No description provided for @newItem.
  ///
  /// In zh, this message translates to:
  /// **'新建'**
  String get newItem;

  /// No description provided for @newGoal.
  ///
  /// In zh, this message translates to:
  /// **'新建目标'**
  String get newGoal;

  /// No description provided for @goalManageInPage.
  ///
  /// In zh, this message translates to:
  /// **'管理与 AI 讨论在「目标」页'**
  String get goalManageInPage;

  /// No description provided for @goalStudioHelp.
  ///
  /// In zh, this message translates to:
  /// **'把一句意图交给 AI（例：「3 年内月收入 10 万」）：它先给现实性评估与假设清单，再拆成一条长程计划与由近及远的里程碑步骤，每步都带可验证判据；落库前都要你确认。'**
  String get goalStudioHelp;

  /// No description provided for @goalTreeHelp.
  ///
  /// In zh, this message translates to:
  /// **'计划确认后出现在左侧目标树。在左侧选中某个目标，这里会切换成那条目标的专属讨论。'**
  String get goalTreeHelp;

  /// No description provided for @noActiveGoals.
  ///
  /// In zh, this message translates to:
  /// **'还没有活跃目标。在「目标」页可新建或与 AI 共同立项。'**
  String get noActiveGoals;

  /// No description provided for @planWithAi.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 聊聊，共同立项'**
  String get planWithAi;

  /// No description provided for @reviewGoalsWithAi.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 回顾 / 立项'**
  String get reviewGoalsWithAi;

  /// No description provided for @noGoalsInStatus.
  ///
  /// In zh, this message translates to:
  /// **'所选状态下暂无目标'**
  String get noGoalsInStatus;

  /// No description provided for @noActiveGoalsCreate.
  ///
  /// In zh, this message translates to:
  /// **'还没有活跃目标，点右上「新建」，或和 AI 共同立项'**
  String get noActiveGoalsCreate;

  /// No description provided for @goalCapHint.
  ///
  /// In zh, this message translates to:
  /// **'活跃已满，新目标将先放入「暂停搁置」'**
  String get goalCapHint;

  /// No description provided for @newGoalHint.
  ///
  /// In zh, this message translates to:
  /// **'设立新目标愿景…（如：发布 Elsewhen 1.0）'**
  String get newGoalHint;

  /// No description provided for @collapseGoalContext.
  ///
  /// In zh, this message translates to:
  /// **'收起目标背景'**
  String get collapseGoalContext;

  /// No description provided for @addGoalContext.
  ///
  /// In zh, this message translates to:
  /// **'补充动力/行动/困难'**
  String get addGoalContext;

  /// No description provided for @goalWhyOptional.
  ///
  /// In zh, this message translates to:
  /// **'💡 为什么做 / 核心动力（可选）'**
  String get goalWhyOptional;

  /// No description provided for @goalWhyHint.
  ///
  /// In zh, this message translates to:
  /// **'如：验证个人认知系统的完整闭环'**
  String get goalWhyHint;

  /// No description provided for @goalActionOptional.
  ///
  /// In zh, this message translates to:
  /// **'📍 接下来一两周要做的事（可选）'**
  String get goalActionOptional;

  /// No description provided for @goalActionHint.
  ///
  /// In zh, this message translates to:
  /// **'如：攻克多端同步快照收敛'**
  String get goalActionHint;

  /// No description provided for @goalObstacleOptional.
  ///
  /// In zh, this message translates to:
  /// **'🚧 当前困难 / 困难（可选）'**
  String get goalObstacleOptional;

  /// No description provided for @goalObstacleHint.
  ///
  /// In zh, this message translates to:
  /// **'如：真机证书配置较繁琐'**
  String get goalObstacleHint;

  /// No description provided for @goalDoneOptional.
  ///
  /// In zh, this message translates to:
  /// **'🎯 验收标准（可选）'**
  String get goalDoneOptional;

  /// No description provided for @goalDoneHint.
  ///
  /// In zh, this message translates to:
  /// **'如：多端测试全绿通过'**
  String get goalDoneHint;

  /// No description provided for @collapsePhase.
  ///
  /// In zh, this message translates to:
  /// **'收起阶段'**
  String get collapsePhase;

  /// No description provided for @expandPhase.
  ///
  /// In zh, this message translates to:
  /// **'展开阶段'**
  String get expandPhase;

  /// No description provided for @currentPhase.
  ///
  /// In zh, this message translates to:
  /// **'当前阶段'**
  String get currentPhase;

  /// No description provided for @moreActions.
  ///
  /// In zh, this message translates to:
  /// **'更多操作'**
  String get moreActions;

  /// No description provided for @reactivateGoal.
  ///
  /// In zh, this message translates to:
  /// **'重新激活（读档唤醒）'**
  String get reactivateGoal;

  /// No description provided for @adjustGoalAction.
  ///
  /// In zh, this message translates to:
  /// **'调整当前行动 / 困难'**
  String get adjustGoalAction;

  /// No description provided for @completeGoal.
  ///
  /// In zh, this message translates to:
  /// **'达成目标（归档为成就）'**
  String get completeGoal;

  /// No description provided for @pauseGoal.
  ///
  /// In zh, this message translates to:
  /// **'暂时搁置（随时读档复活）'**
  String get pauseGoal;

  /// No description provided for @editGoalDetails.
  ///
  /// In zh, this message translates to:
  /// **'编辑全貌'**
  String get editGoalDetails;

  /// No description provided for @currentActionLabel.
  ///
  /// In zh, this message translates to:
  /// **'当前行动：'**
  String get currentActionLabel;

  /// No description provided for @currentObstacleLabel.
  ///
  /// In zh, this message translates to:
  /// **'当前困难：'**
  String get currentObstacleLabel;

  /// No description provided for @goalActionNotSet.
  ///
  /// In zh, this message translates to:
  /// **'还没确定，点击写下接下来一两周要做的事'**
  String get goalActionNotSet;

  /// No description provided for @nextPhaseHint.
  ///
  /// In zh, this message translates to:
  /// **'攻克上一阶段后，将注意力收敛到近 1~2 周最先启动的下一个阶段。'**
  String get nextPhaseHint;

  /// No description provided for @currentActionField.
  ///
  /// In zh, this message translates to:
  /// **'📍 当前行动'**
  String get currentActionField;

  /// No description provided for @nextActionHint.
  ///
  /// In zh, this message translates to:
  /// **'如：完成首版同步快照算法并跑通测试'**
  String get nextActionHint;

  /// No description provided for @clearObstacleField.
  ///
  /// In zh, this message translates to:
  /// **'🚧 当前困难（可留空清除）'**
  String get clearObstacleField;

  /// No description provided for @nextObstacleHint.
  ///
  /// In zh, this message translates to:
  /// **'如：等待对方确认网络穿透端口'**
  String get nextObstacleHint;

  /// No description provided for @updateAction.
  ///
  /// In zh, this message translates to:
  /// **'更新行动'**
  String get updateAction;

  /// No description provided for @restartGoalHint.
  ///
  /// In zh, this message translates to:
  /// **'重新启动此目标，请确认或设定接下来的第一具体行动：'**
  String get restartGoalHint;

  /// No description provided for @restartActionField.
  ///
  /// In zh, this message translates to:
  /// **'📍 重新具体行动'**
  String get restartActionField;

  /// No description provided for @restartActionHint.
  ///
  /// In zh, this message translates to:
  /// **'如：理清代码依赖并设计重构方案'**
  String get restartActionHint;

  /// No description provided for @restartObstacleField.
  ///
  /// In zh, this message translates to:
  /// **'🚧 当前当前困难（可选）'**
  String get restartObstacleField;

  /// No description provided for @restartObstacleHint.
  ///
  /// In zh, this message translates to:
  /// **'如：缺少相关设备环境'**
  String get restartObstacleHint;

  /// No description provided for @confirmReactivate.
  ///
  /// In zh, this message translates to:
  /// **'确认唤醒'**
  String get confirmReactivate;

  /// No description provided for @editFullGoal.
  ///
  /// In zh, this message translates to:
  /// **'编辑目标全貌'**
  String get editFullGoal;

  /// No description provided for @goalVision.
  ///
  /// In zh, this message translates to:
  /// **'目标愿景内容'**
  String get goalVision;

  /// No description provided for @goalWhy.
  ///
  /// In zh, this message translates to:
  /// **'💡 为什么做 / 核心动力'**
  String get goalWhy;

  /// No description provided for @goalNextAction.
  ///
  /// In zh, this message translates to:
  /// **'📍 接下来一两周要做的事'**
  String get goalNextAction;

  /// No description provided for @goalObstacle.
  ///
  /// In zh, this message translates to:
  /// **'🚧 当前困难 / 困难'**
  String get goalObstacle;

  /// No description provided for @goalDone.
  ///
  /// In zh, this message translates to:
  /// **'🎯 验收判据 / 完成标准'**
  String get goalDone;

  /// No description provided for @saveChanges.
  ///
  /// In zh, this message translates to:
  /// **'保存修改'**
  String get saveChanges;

  /// No description provided for @goalActionSaved.
  ///
  /// In zh, this message translates to:
  /// **'已更新行动与困难'**
  String get goalActionSaved;

  /// No description provided for @goalDetailsSaved.
  ///
  /// In zh, this message translates to:
  /// **'已保存目标全貌'**
  String get goalDetailsSaved;

  /// No description provided for @collapseDiscussion.
  ///
  /// In zh, this message translates to:
  /// **'收起讨论'**
  String get collapseDiscussion;

  /// No description provided for @goalStudioInputHint.
  ///
  /// In zh, this message translates to:
  /// **'说说你的长程意图…（草稿不会被自动发送）'**
  String get goalStudioInputHint;

  /// No description provided for @goalChatInputHint.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 聊聊这个目标…（草稿不会被自动发送）'**
  String get goalChatInputHint;

  /// No description provided for @generateReply.
  ///
  /// In zh, this message translates to:
  /// **'生成回复'**
  String get generateReply;

  /// No description provided for @goalStudioWelcome.
  ///
  /// In zh, this message translates to:
  /// **'🎯 说说你想实现的长程意图（比如「3 年内月收入 10 万」）。我会先给现实性评估与假设清单，再拆成一条长程计划与由近及远的里程碑步骤，落库前都要你确认。'**
  String get goalStudioWelcome;

  /// No description provided for @goalChatWelcome.
  ///
  /// In zh, this message translates to:
  /// **'🎯 我们来聊聊这个目标。我可以帮你拆解行动、检视困难、回顾完成标准；需要暂停、标记达成或重启目标时，我会先给出提议，等你确认后才生效。'**
  String get goalChatWelcome;

  /// No description provided for @goalPaused.
  ///
  /// In zh, this message translates to:
  /// **'已将「{goal}」暂时搁置'**
  String goalPaused(String goal);

  /// No description provided for @goalCompletedToast.
  ///
  /// In zh, this message translates to:
  /// **'🎉 达成目标「{goal}」！'**
  String goalCompletedToast(String goal);

  /// No description provided for @goalReactivated.
  ///
  /// In zh, this message translates to:
  /// **'已唤醒「{goal}」并设为活跃目标'**
  String goalReactivated(String goal);

  /// No description provided for @goalCapacity.
  ///
  /// In zh, this message translates to:
  /// **'当前已有 {count} 个活跃目标（精力上限）。请先暂停或达成一个，再腾出精力唤醒此目标。'**
  String goalCapacity(int count);

  /// No description provided for @activeGoalCount.
  ///
  /// In zh, this message translates to:
  /// **'活跃目标 {count} / {maximum}'**
  String activeGoalCount(int count, int maximum);

  /// No description provided for @goalLoadFailed.
  ///
  /// In zh, this message translates to:
  /// **'目标加载失败：{error}'**
  String goalLoadFailed(String error);

  /// No description provided for @newGoalPaused.
  ///
  /// In zh, this message translates to:
  /// **'活跃已满，新愿景「{goal}」已放入「暂停搁置」'**
  String newGoalPaused(String goal);

  /// No description provided for @newGoalCreated.
  ///
  /// In zh, this message translates to:
  /// **'已设立新愿景「{goal}」'**
  String newGoalCreated(String goal);

  /// No description provided for @newGoalCapacityHint.
  ///
  /// In zh, this message translates to:
  /// **'活跃已满 {count} 个，新目标将以「暂停搁置」状态创建，可随时读档唤醒。'**
  String newGoalCapacityHint(int count);

  /// No description provided for @whyGoal.
  ///
  /// In zh, this message translates to:
  /// **'为什么做：{why}'**
  String whyGoal(String why);

  /// No description provided for @doneGoal.
  ///
  /// In zh, this message translates to:
  /// **'怎样算完成：{done}'**
  String doneGoal(String done);

  /// No description provided for @adjustActionTitle.
  ///
  /// In zh, this message translates to:
  /// **'调整当前行动 · {goal}'**
  String adjustActionTitle(String goal);

  /// No description provided for @reactivateGoalTitle.
  ///
  /// In zh, this message translates to:
  /// **'读档唤醒 · {goal}'**
  String reactivateGoalTitle(String goal);

  /// No description provided for @motivationReview.
  ///
  /// In zh, this message translates to:
  /// **'💡 核心动力回顾：{why}'**
  String motivationReview(String why);

  /// No description provided for @goalDiscussionTitle.
  ///
  /// In zh, this message translates to:
  /// **'目标讨论 · {goal}'**
  String goalDiscussionTitle(String goal);

  /// No description provided for @goalNewSeed.
  ///
  /// In zh, this message translates to:
  /// **'我想立一个新的长程目标。请用几个设问帮我明确：为什么做、什么算完成、近 1~2 周的第一具体行动是什么、可能卡在哪。{brief}'**
  String goalNewSeed(String brief);

  /// No description provided for @goalReviewSeed.
  ///
  /// In zh, this message translates to:
  /// **'帮我看看这些目标的推进情况：哪个行动该滚动到下一个阶段了？或者帮我共同规划一个新的长程目标。{brief}'**
  String goalReviewSeed(String brief);

  /// No description provided for @goalBriefItem.
  ///
  /// In zh, this message translates to:
  /// **'「{goal}」（行动：{action}）'**
  String goalBriefItem(String goal, String action);

  /// No description provided for @goalBrief.
  ///
  /// In zh, this message translates to:
  /// **'当前活跃目标：{goals}。'**
  String goalBrief(String goals);

  /// No description provided for @noGoalsBrief.
  ///
  /// In zh, this message translates to:
  /// **'目前还没有活跃目标。'**
  String get noGoalsBrief;

  /// No description provided for @notSet.
  ///
  /// In zh, this message translates to:
  /// **'未设定'**
  String get notSet;

  /// No description provided for @planNewGoal.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 拆解 / 立新目标'**
  String get planNewGoal;

  /// No description provided for @nearTerm.
  ///
  /// In zh, this message translates to:
  /// **'近期'**
  String get nearTerm;

  /// No description provided for @midTerm.
  ///
  /// In zh, this message translates to:
  /// **'中期'**
  String get midTerm;

  /// No description provided for @longTerm.
  ///
  /// In zh, this message translates to:
  /// **'长远'**
  String get longTerm;

  /// No description provided for @activeGoals.
  ///
  /// In zh, this message translates to:
  /// **'活跃'**
  String get activeGoals;

  /// No description provided for @pausedGoals.
  ///
  /// In zh, this message translates to:
  /// **'暂停搁置'**
  String get pausedGoals;

  /// No description provided for @achievedGoals.
  ///
  /// In zh, this message translates to:
  /// **'已达成'**
  String get achievedGoals;

  /// No description provided for @inProgressGoal.
  ///
  /// In zh, this message translates to:
  /// **'进行中'**
  String get inProgressGoal;

  /// No description provided for @pausedGoalState.
  ///
  /// In zh, this message translates to:
  /// **'已暂停'**
  String get pausedGoalState;

  /// No description provided for @pausedShort.
  ///
  /// In zh, this message translates to:
  /// **'暂停'**
  String get pausedShort;

  /// No description provided for @initializingConversation.
  ///
  /// In zh, this message translates to:
  /// **'会话初始化'**
  String get initializingConversation;

  /// No description provided for @loadMessages.
  ///
  /// In zh, this message translates to:
  /// **'消息加载'**
  String get loadMessages;

  /// No description provided for @examplePerson.
  ///
  /// In zh, this message translates to:
  /// **'@张伟 '**
  String get examplePerson;

  /// No description provided for @exampleTopic.
  ///
  /// In zh, this message translates to:
  /// **'#付款流程'**
  String get exampleTopic;

  /// No description provided for @goalSummaryCount.
  ///
  /// In zh, this message translates to:
  /// **'{count, plural, =0{目标} other{目标{count}}}'**
  String goalSummaryCount(int count);

  /// No description provided for @supplementUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'未补充：材料不能安全合并，已完整保留；请单独处理或撤回'**
  String get supplementUnsupported;

  /// No description provided for @supplementInputChanged.
  ///
  /// In zh, this message translates to:
  /// **'未补充：输入版本已变化，仍在队列'**
  String get supplementInputChanged;

  /// No description provided for @supplementContextChanged.
  ///
  /// In zh, this message translates to:
  /// **'未补充：材料版本或任务范围不同，已保留为下一问'**
  String get supplementContextChanged;

  /// No description provided for @supplementBudget.
  ///
  /// In zh, this message translates to:
  /// **'未补充：当前请求预算或协议不足，仍在队列'**
  String get supplementBudget;

  /// No description provided for @supplementClosed.
  ///
  /// In zh, this message translates to:
  /// **'目标任务已结束；继续队列前不会生成'**
  String get supplementClosed;

  /// No description provided for @supplementWaiting.
  ///
  /// In zh, this message translates to:
  /// **'等待工具轮结束后补充；未加入时仍保留排队资格'**
  String get supplementWaiting;

  /// No description provided for @queueUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'队列状态暂不可用；输入不会重新保存。'**
  String get queueUnavailable;

  /// No description provided for @queueActionUnconfirmed.
  ///
  /// In zh, this message translates to:
  /// **'队列操作未确认，请刷新核实。'**
  String get queueActionUnconfirmed;

  /// No description provided for @queuePaused.
  ///
  /// In zh, this message translates to:
  /// **'待生成队列已暂停'**
  String get queuePaused;

  /// No description provided for @queueOrdered.
  ///
  /// In zh, this message translates to:
  /// **'待生成队列 · 按保存顺序处理'**
  String get queueOrdered;

  /// No description provided for @resumeQueue.
  ///
  /// In zh, this message translates to:
  /// **'继续队列'**
  String get resumeQueue;

  /// No description provided for @pauseQueue.
  ///
  /// In zh, this message translates to:
  /// **'暂停队列'**
  String get pauseQueue;

  /// No description provided for @queueInputDescription.
  ///
  /// In zh, this message translates to:
  /// **'待生成列表中的输入尚未发送给模型；撤回只取消生成资格。'**
  String get queueInputDescription;

  /// No description provided for @withdrawGeneration.
  ///
  /// In zh, this message translates to:
  /// **'撤回生成'**
  String get withdrawGeneration;

  /// No description provided for @moreQueuedInputs.
  ///
  /// In zh, this message translates to:
  /// **'另有待生成输入；前面的输入处理后会依次显示。'**
  String get moreQueuedInputs;

  /// No description provided for @injectedSupplements.
  ///
  /// In zh, this message translates to:
  /// **'已加入任务的补充'**
  String get injectedSupplements;

  /// No description provided for @injectedSupplementsHint.
  ///
  /// In zh, this message translates to:
  /// **'仅表示已加入下一请求上下文，不保证网络送达或模型完成；不会再次排队。'**
  String get injectedSupplementsHint;

  /// No description provided for @wikiChat.
  ///
  /// In zh, this message translates to:
  /// **'AI对话'**
  String get wikiChat;

  /// No description provided for @moveChatWindow.
  ///
  /// In zh, this message translates to:
  /// **'拖动以移动对话窗口'**
  String get moveChatWindow;

  /// No description provided for @shrinkChatWindow.
  ///
  /// In zh, this message translates to:
  /// **'缩小对话窗口'**
  String get shrinkChatWindow;

  /// No description provided for @backToMainChat.
  ///
  /// In zh, this message translates to:
  /// **'返回主对话'**
  String get backToMainChat;

  /// No description provided for @aiTasks.
  ///
  /// In zh, this message translates to:
  /// **'AI 任务'**
  String get aiTasks;

  /// No description provided for @summarizePage.
  ///
  /// In zh, this message translates to:
  /// **'总结这一页'**
  String get summarizePage;

  /// No description provided for @extractKeyPoints.
  ///
  /// In zh, this message translates to:
  /// **'提取关键点'**
  String get extractKeyPoints;

  /// No description provided for @captureDecisions.
  ///
  /// In zh, this message translates to:
  /// **'沉淀决定和依据'**
  String get captureDecisions;

  /// No description provided for @organizeNextSteps.
  ///
  /// In zh, this message translates to:
  /// **'整理下一步'**
  String get organizeNextSteps;

  /// No description provided for @translateSelection.
  ///
  /// In zh, this message translates to:
  /// **'翻译片段'**
  String get translateSelection;

  /// No description provided for @writeCopy.
  ///
  /// In zh, this message translates to:
  /// **'写文案'**
  String get writeCopy;

  /// No description provided for @rescanProject.
  ///
  /// In zh, this message translates to:
  /// **'重新扫描项目目录'**
  String get rescanProject;

  /// No description provided for @reimportSource.
  ///
  /// In zh, this message translates to:
  /// **'重新导入来源'**
  String get reimportSource;

  /// No description provided for @taskResetDescription.
  ///
  /// In zh, this message translates to:
  /// **'重新选择将使用当前页的新版本，并清除旧选区和对照材料。'**
  String get taskResetDescription;

  /// No description provided for @reselectTaskMaterial.
  ///
  /// In zh, this message translates to:
  /// **'明确重新选择任务与材料'**
  String get reselectTaskMaterial;

  /// No description provided for @askPageHint.
  ///
  /// In zh, this message translates to:
  /// **'就这一页问问 AI…'**
  String get askPageHint;

  /// No description provided for @pageChatWelcome.
  ///
  /// In zh, this message translates to:
  /// **'👋 我可以帮你总结、提取要点、补充或改写这一页；需要写回知识库时会先给你确认。\n如果这是从本地目录导入的项目，也可以直接说“重新扫描这个项目目录”，我会按最新文件重新导入。'**
  String get pageChatWelcome;

  /// No description provided for @summarizePagePrompt.
  ///
  /// In zh, this message translates to:
  /// **'请总结这一页，提炼最重要的信息。'**
  String get summarizePagePrompt;

  /// No description provided for @keyPointsPrompt.
  ///
  /// In zh, this message translates to:
  /// **'请提取这一页的关键事实、判断和可复用信息。'**
  String get keyPointsPrompt;

  /// No description provided for @decisionsPrompt.
  ///
  /// In zh, this message translates to:
  /// **'请整理这一页中已经明确的决定及其依据。'**
  String get decisionsPrompt;

  /// No description provided for @nextStepsPrompt.
  ///
  /// In zh, this message translates to:
  /// **'请整理这一页的下一步行动和仍未解决的问题。'**
  String get nextStepsPrompt;

  /// No description provided for @translateSelectionPrompt.
  ///
  /// In zh, this message translates to:
  /// **'请把选定的原文片段翻译成 XX 语言，保留结构和专业含义；只处理已提供的片段，先说明范围，不声称翻译整页。'**
  String get translateSelectionPrompt;

  /// No description provided for @writeCopyPrompt.
  ///
  /// In zh, this message translates to:
  /// **'请根据这一页内容，写一版适合发布到 XX 渠道的文案，目标受众是 XX，语气是 XX。'**
  String get writeCopyPrompt;

  /// No description provided for @projectRescannedAssets.
  ///
  /// In zh, this message translates to:
  /// **'项目目录已重新扫描，资产知识页已更新'**
  String get projectRescannedAssets;

  /// No description provided for @replaceProjectPage.
  ///
  /// In zh, this message translates to:
  /// **'替换项目知识页正文？'**
  String get replaceProjectPage;

  /// No description provided for @replaceProjectDescription.
  ///
  /// In zh, this message translates to:
  /// **'当前 AI 回复将替换整篇正文。原内容会保留在页面修订记录中。'**
  String get replaceProjectDescription;

  /// No description provided for @replace.
  ///
  /// In zh, this message translates to:
  /// **'替换'**
  String get replace;

  /// No description provided for @projectPageReplaced.
  ///
  /// In zh, this message translates to:
  /// **'已替换当前项目知识页正文'**
  String get projectPageReplaced;

  /// No description provided for @pageMissing.
  ///
  /// In zh, this message translates to:
  /// **'页面不存在，可能已被删除'**
  String get pageMissing;

  /// No description provided for @projectRescanned.
  ///
  /// In zh, this message translates to:
  /// **'项目目录已重新扫描，知识页已更新'**
  String get projectRescanned;

  /// No description provided for @sourceReimported.
  ///
  /// In zh, this message translates to:
  /// **'已重新导入，知识页内容已更新'**
  String get sourceReimported;

  /// No description provided for @thinkingShort.
  ///
  /// In zh, this message translates to:
  /// **'思考中…'**
  String get thinkingShort;

  /// No description provided for @copiedMessage.
  ///
  /// In zh, this message translates to:
  /// **'已复制消息'**
  String get copiedMessage;

  /// No description provided for @replaceBody.
  ///
  /// In zh, this message translates to:
  /// **'替换正文'**
  String get replaceBody;

  /// No description provided for @saveAsOutput.
  ///
  /// In zh, this message translates to:
  /// **'保存为产出'**
  String get saveAsOutput;

  /// No description provided for @aiReply.
  ///
  /// In zh, this message translates to:
  /// **'AI 回复'**
  String get aiReply;

  /// No description provided for @selectReceipt.
  ///
  /// In zh, this message translates to:
  /// **'选择左侧一条记录查看回执'**
  String get selectReceipt;

  /// No description provided for @backToList.
  ///
  /// In zh, this message translates to:
  /// **'返回列表'**
  String get backToList;

  /// No description provided for @selectProposal.
  ///
  /// In zh, this message translates to:
  /// **'选择左侧一条提议查看详情'**
  String get selectProposal;

  /// No description provided for @pendingReview.
  ///
  /// In zh, this message translates to:
  /// **'待审'**
  String get pendingReview;

  /// No description provided for @decisionHistory.
  ///
  /// In zh, this message translates to:
  /// **'处理记录'**
  String get decisionHistory;

  /// No description provided for @refresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新'**
  String get refresh;

  /// No description provided for @noDecisions.
  ///
  /// In zh, this message translates to:
  /// **'暂无处理记录'**
  String get noDecisions;

  /// No description provided for @noProposals.
  ///
  /// In zh, this message translates to:
  /// **'暂无待审提议'**
  String get noProposals;

  /// No description provided for @previousPage.
  ///
  /// In zh, this message translates to:
  /// **'上一页'**
  String get previousPage;

  /// No description provided for @nextPage.
  ///
  /// In zh, this message translates to:
  /// **'下一页'**
  String get nextPage;

  /// No description provided for @parametersSources.
  ///
  /// In zh, this message translates to:
  /// **'参数与来源'**
  String get parametersSources;

  /// No description provided for @midTermGoal.
  ///
  /// In zh, this message translates to:
  /// **'中期目标'**
  String get midTermGoal;

  /// No description provided for @longTermGoal.
  ///
  /// In zh, this message translates to:
  /// **'长远目标'**
  String get longTermGoal;

  /// No description provided for @nearTermGoal.
  ///
  /// In zh, this message translates to:
  /// **'近期目标'**
  String get nearTermGoal;

  /// No description provided for @motivationLabel.
  ///
  /// In zh, this message translates to:
  /// **'核心动力：'**
  String get motivationLabel;

  /// No description provided for @completionCriteriaLabel.
  ///
  /// In zh, this message translates to:
  /// **'完成判据：'**
  String get completionCriteriaLabel;

  /// No description provided for @feasibilityAssessment.
  ///
  /// In zh, this message translates to:
  /// **'现实性评估'**
  String get feasibilityAssessment;

  /// No description provided for @assumptions.
  ///
  /// In zh, this message translates to:
  /// **'假设清单'**
  String get assumptions;

  /// No description provided for @assumption.
  ///
  /// In zh, this message translates to:
  /// **'假设'**
  String get assumption;

  /// No description provided for @milestoneSteps.
  ///
  /// In zh, this message translates to:
  /// **'里程碑步骤（第一步确认后直接推进，其余排队）'**
  String get milestoneSteps;

  /// No description provided for @proposalTitle.
  ///
  /// In zh, this message translates to:
  /// **'标题'**
  String get proposalTitle;

  /// No description provided for @proposalBody.
  ///
  /// In zh, this message translates to:
  /// **'正文'**
  String get proposalBody;

  /// No description provided for @goalVisionShort.
  ///
  /// In zh, this message translates to:
  /// **'目标愿景'**
  String get goalVisionShort;

  /// No description provided for @ruleContent.
  ///
  /// In zh, this message translates to:
  /// **'规则内容'**
  String get ruleContent;

  /// No description provided for @recordContent.
  ///
  /// In zh, this message translates to:
  /// **'记录内容'**
  String get recordContent;

  /// No description provided for @note.
  ///
  /// In zh, this message translates to:
  /// **'备注'**
  String get note;

  /// No description provided for @dueIsoOptional.
  ///
  /// In zh, this message translates to:
  /// **'截止时间（ISO 8601，可留空）'**
  String get dueIsoOptional;

  /// No description provided for @priorityOptions.
  ///
  /// In zh, this message translates to:
  /// **'优先级（low/normal/high）'**
  String get priorityOptions;

  /// No description provided for @changeNote.
  ///
  /// In zh, this message translates to:
  /// **'修改说明'**
  String get changeNote;

  /// No description provided for @newTitle.
  ///
  /// In zh, this message translates to:
  /// **'新标题'**
  String get newTitle;

  /// No description provided for @phaseOptions.
  ///
  /// In zh, this message translates to:
  /// **'周期阶段（near/mid/long）'**
  String get phaseOptions;

  /// No description provided for @visionOptional.
  ///
  /// In zh, this message translates to:
  /// **'愿景（可留空）'**
  String get visionOptional;

  /// No description provided for @assessmentOptional.
  ///
  /// In zh, this message translates to:
  /// **'现实性评估（可留空）'**
  String get assessmentOptional;

  /// No description provided for @whyMotivation.
  ///
  /// In zh, this message translates to:
  /// **'为什么做 / 核心动力'**
  String get whyMotivation;

  /// No description provided for @skillsExperience.
  ///
  /// In zh, this message translates to:
  /// **'能力与经验'**
  String get skillsExperience;

  /// No description provided for @availableResources.
  ///
  /// In zh, this message translates to:
  /// **'已有资源'**
  String get availableResources;

  /// No description provided for @timeConstraints.
  ///
  /// In zh, this message translates to:
  /// **'时间与现实限制'**
  String get timeConstraints;

  /// No description provided for @preferencesBoundaries.
  ///
  /// In zh, this message translates to:
  /// **'偏好与底线'**
  String get preferencesBoundaries;

  /// No description provided for @actionWeeks.
  ///
  /// In zh, this message translates to:
  /// **'当前 1~2 周具体行动'**
  String get actionWeeks;

  /// No description provided for @obstaclesExpected.
  ///
  /// In zh, this message translates to:
  /// **'当前困难 / 困难预判'**
  String get obstaclesExpected;

  /// No description provided for @completionCriteria.
  ///
  /// In zh, this message translates to:
  /// **'完成判据 / 验收标准'**
  String get completionCriteria;

  /// No description provided for @reviewGoalBasis.
  ///
  /// In zh, this message translates to:
  /// **'审阅所依据的目标版本'**
  String get reviewGoalBasis;

  /// No description provided for @editDraftHint.
  ///
  /// In zh, this message translates to:
  /// **'保存修改只更新草稿；重新查看后，仍需点击接受才执行业务操作。'**
  String get editDraftHint;

  /// No description provided for @rejectThis.
  ///
  /// In zh, this message translates to:
  /// **'拒绝这项'**
  String get rejectThis;

  /// No description provided for @acceptThis.
  ///
  /// In zh, this message translates to:
  /// **'接受这项'**
  String get acceptThis;

  /// No description provided for @processingEllipsis.
  ///
  /// In zh, this message translates to:
  /// **'处理中…'**
  String get processingEllipsis;

  /// No description provided for @saveWithoutExecution.
  ///
  /// In zh, this message translates to:
  /// **'保存修改，不执行'**
  String get saveWithoutExecution;

  /// No description provided for @batchReviewDescription.
  ///
  /// In zh, this message translates to:
  /// **'仅处理下面列出的具体版本。逐项执行，不是整批原子提交；可能部分成功。每项都会显示结果。'**
  String get batchReviewDescription;

  /// No description provided for @targetVersionBasis.
  ///
  /// In zh, this message translates to:
  /// **'目标版本依据'**
  String get targetVersionBasis;

  /// No description provided for @batchFinished.
  ///
  /// In zh, this message translates to:
  /// **'本次逐项处理结束，请查看每项结果。'**
  String get batchFinished;

  /// No description provided for @rejectThese.
  ///
  /// In zh, this message translates to:
  /// **'拒绝这些项'**
  String get rejectThese;

  /// No description provided for @acceptThese.
  ///
  /// In zh, this message translates to:
  /// **'接受这些项'**
  String get acceptThese;

  /// No description provided for @processingItems.
  ///
  /// In zh, this message translates to:
  /// **'逐项处理中…'**
  String get processingItems;

  /// No description provided for @reviewProposals.
  ///
  /// In zh, this message translates to:
  /// **'审阅提议'**
  String get reviewProposals;

  /// No description provided for @globalRuleReview.
  ///
  /// In zh, this message translates to:
  /// **'历史全局规则审阅'**
  String get globalRuleReview;

  /// No description provided for @conversationProposalReview.
  ///
  /// In zh, this message translates to:
  /// **'对话提议审阅'**
  String get conversationProposalReview;

  /// No description provided for @actionActivateRule.
  ///
  /// In zh, this message translates to:
  /// **'个人规则'**
  String get actionActivateRule;

  /// No description provided for @actionCreateTodo.
  ///
  /// In zh, this message translates to:
  /// **'创建待办'**
  String get actionCreateTodo;

  /// No description provided for @actionSavePage.
  ///
  /// In zh, this message translates to:
  /// **'保存知识页'**
  String get actionSavePage;

  /// No description provided for @actionRevisePage.
  ///
  /// In zh, this message translates to:
  /// **'知识页修订或派生'**
  String get actionRevisePage;

  /// No description provided for @actionRenamePage.
  ///
  /// In zh, this message translates to:
  /// **'知识页改名'**
  String get actionRenamePage;

  /// No description provided for @actionRecordEvent.
  ///
  /// In zh, this message translates to:
  /// **'保存记录'**
  String get actionRecordEvent;

  /// No description provided for @actionPeopleRelations.
  ///
  /// In zh, this message translates to:
  /// **'联系人与关系'**
  String get actionPeopleRelations;

  /// No description provided for @actionArchiveChats.
  ///
  /// In zh, this message translates to:
  /// **'归档对话'**
  String get actionArchiveChats;

  /// No description provided for @actionImportSource.
  ///
  /// In zh, this message translates to:
  /// **'导入来源'**
  String get actionImportSource;

  /// No description provided for @actionImportFiles.
  ///
  /// In zh, this message translates to:
  /// **'导入目录文件'**
  String get actionImportFiles;

  /// No description provided for @actionImportProject.
  ///
  /// In zh, this message translates to:
  /// **'导入项目摘要'**
  String get actionImportProject;

  /// No description provided for @actionPlanningProfile.
  ///
  /// In zh, this message translates to:
  /// **'关于我档案'**
  String get actionPlanningProfile;

  /// No description provided for @actionCreateGoal.
  ///
  /// In zh, this message translates to:
  /// **'设立认知目标'**
  String get actionCreateGoal;

  /// No description provided for @actionUpdateGoal.
  ///
  /// In zh, this message translates to:
  /// **'更新目标切片'**
  String get actionUpdateGoal;

  /// No description provided for @actionCreatePlan.
  ///
  /// In zh, this message translates to:
  /// **'长程计划拆解'**
  String get actionCreatePlan;

  /// No description provided for @actionAddSteps.
  ///
  /// In zh, this message translates to:
  /// **'计划追加步骤'**
  String get actionAddSteps;

  /// No description provided for @receiptWaiting.
  ///
  /// In zh, this message translates to:
  /// **'等待执行回执'**
  String get receiptWaiting;

  /// No description provided for @receiptFailedRolledBack.
  ///
  /// In zh, this message translates to:
  /// **'执行失败（已回滚）'**
  String get receiptFailedRolledBack;

  /// No description provided for @declined.
  ///
  /// In zh, this message translates to:
  /// **'已拒绝'**
  String get declined;

  /// No description provided for @reviewSelectedCount.
  ///
  /// In zh, this message translates to:
  /// **'审阅选中（{count}）'**
  String reviewSelectedCount(int count);

  /// No description provided for @reviewSelectedTitle.
  ///
  /// In zh, this message translates to:
  /// **'审阅选中的 {count} 项'**
  String reviewSelectedTitle(int count);

  /// No description provided for @pageNumber.
  ///
  /// In zh, this message translates to:
  /// **'第 {page} 页'**
  String pageNumber(int page);

  /// No description provided for @proposalVersion.
  ///
  /// In zh, this message translates to:
  /// **'{action} · 版本 {version}'**
  String proposalVersion(String action, String version);

  /// No description provided for @updateGoalById.
  ///
  /// In zh, this message translates to:
  /// **'更新目标阶段 (ID: {id})'**
  String updateGoalById(String id);

  /// No description provided for @invalidProposal.
  ///
  /// In zh, this message translates to:
  /// **'提议格式异常，不能接受；可以拒绝并重新草拟。\n{args}'**
  String invalidProposal(String args);

  /// No description provided for @goalId.
  ///
  /// In zh, this message translates to:
  /// **'目标 ID: {id}'**
  String goalId(String id);

  /// No description provided for @planLabel.
  ///
  /// In zh, this message translates to:
  /// **'计划：{plan}'**
  String planLabel(String plan);

  /// No description provided for @intentLabel.
  ///
  /// In zh, this message translates to:
  /// **'意图：「{intent}」'**
  String intentLabel(String intent);

  /// No description provided for @visionLabel.
  ///
  /// In zh, this message translates to:
  /// **'愿景：{vision}'**
  String visionLabel(String vision);

  /// No description provided for @criterionLabel.
  ///
  /// In zh, this message translates to:
  /// **'（判据：{done}）'**
  String criterionLabel(String done);

  /// No description provided for @proposalReopen.
  ///
  /// In zh, this message translates to:
  /// **'{error}\n请关闭后重新打开审阅，不会自动接受新的版本。'**
  String proposalReopen(String error);

  /// No description provided for @fullReceiptCount.
  ///
  /// In zh, this message translates to:
  /// **'展开完整回执（全文 {count} 字）'**
  String fullReceiptCount(int count);

  /// No description provided for @batchUnconfirmed.
  ///
  /// In zh, this message translates to:
  /// **'本项未确认完成，请核对处理记录；不会自动重试：{error}'**
  String batchUnconfirmed(String error);

  /// No description provided for @taskRestoreFailed.
  ///
  /// In zh, this message translates to:
  /// **'原任务暂不能恢复：{error}'**
  String taskRestoreFailed(String error);

  /// No description provided for @reloadMaterialFailed.
  ///
  /// In zh, this message translates to:
  /// **'重新读取材料失败：{error}'**
  String reloadMaterialFailed(String error);

  /// No description provided for @savedAsDerivative.
  ///
  /// In zh, this message translates to:
  /// **'已保存为派生产物：{title}'**
  String savedAsDerivative(String title);

  /// No description provided for @rescan.
  ///
  /// In zh, this message translates to:
  /// **'重新扫描'**
  String get rescan;

  /// No description provided for @reimport.
  ///
  /// In zh, this message translates to:
  /// **'重新导入'**
  String get reimport;

  /// No description provided for @readPage.
  ///
  /// In zh, this message translates to:
  /// **'读取页面'**
  String get readPage;

  /// No description provided for @aiSummary.
  ///
  /// In zh, this message translates to:
  /// **'AI 总结'**
  String get aiSummary;

  /// No description provided for @justNow.
  ///
  /// In zh, this message translates to:
  /// **'刚刚'**
  String get justNow;

  /// No description provided for @minutesAgo.
  ///
  /// In zh, this message translates to:
  /// **'{count} 分钟前'**
  String minutesAgo(int count);

  /// No description provided for @hoursAgo.
  ///
  /// In zh, this message translates to:
  /// **'{count} 小时前'**
  String hoursAgo(int count);

  /// No description provided for @daysAgo.
  ///
  /// In zh, this message translates to:
  /// **'{count} 天前'**
  String daysAgo(int count);

  /// No description provided for @taskLocalQuestion.
  ///
  /// In zh, this message translates to:
  /// **'局部问答'**
  String get taskLocalQuestion;

  /// No description provided for @taskFullSummary.
  ///
  /// In zh, this message translates to:
  /// **'全文总结'**
  String get taskFullSummary;

  /// No description provided for @taskCompare.
  ///
  /// In zh, this message translates to:
  /// **'跨来源对照'**
  String get taskCompare;

  /// No description provided for @taskLocalEdit.
  ///
  /// In zh, this message translates to:
  /// **'局部编辑'**
  String get taskLocalEdit;

  /// No description provided for @taskFullRevision.
  ///
  /// In zh, this message translates to:
  /// **'全页修订'**
  String get taskFullRevision;

  /// No description provided for @taskIndependent.
  ///
  /// In zh, this message translates to:
  /// **'独立新问题'**
  String get taskIndependent;

  /// No description provided for @taskMaterialChanged.
  ///
  /// In zh, this message translates to:
  /// **'原任务材料已变化，请明确重新选择任务与材料；不会自动沿用新版本。'**
  String get taskMaterialChanged;

  /// No description provided for @chooseMaterial.
  ///
  /// In zh, this message translates to:
  /// **'查看 / 选择材料'**
  String get chooseMaterial;

  /// No description provided for @taskDisplayedRevision.
  ///
  /// In zh, this message translates to:
  /// **'任务与显示版本'**
  String get taskDisplayedRevision;

  /// No description provided for @taskScopeHint.
  ///
  /// In zh, this message translates to:
  /// **'后续问题沿用这里的任务和版本；新任务请重新选择。全页修订按连续分块处理，完成后仍需审阅。'**
  String get taskScopeHint;

  /// No description provided for @refreshMaterialRevision.
  ///
  /// In zh, this message translates to:
  /// **'明确刷新版本'**
  String get refreshMaterialRevision;

  /// No description provided for @comparisonSlug.
  ///
  /// In zh, this message translates to:
  /// **'对照材料 slug'**
  String get comparisonSlug;

  /// No description provided for @readAndAdd.
  ///
  /// In zh, this message translates to:
  /// **'读取并加入'**
  String get readAndAdd;

  /// No description provided for @selectionAutomatic.
  ///
  /// In zh, this message translates to:
  /// **'未选区；问答将自动选取相关片段'**
  String get selectionAutomatic;

  /// No description provided for @selectionTooLong.
  ///
  /// In zh, this message translates to:
  /// **'选区超过6000字，请缩小'**
  String get selectionTooLong;

  /// No description provided for @useDisplayedRevision.
  ///
  /// In zh, this message translates to:
  /// **'使用显示的版本'**
  String get useDisplayedRevision;

  /// No description provided for @compassDismissHint.
  ///
  /// In zh, this message translates to:
  /// **'处理急事，7 天内不打扰'**
  String get compassDismissHint;

  /// No description provided for @compassFocusObstacle.
  ///
  /// In zh, this message translates to:
  /// **'聚焦攻克此困难'**
  String get compassFocusObstacle;

  /// No description provided for @compassDiscussProgress.
  ///
  /// In zh, this message translates to:
  /// **'聊聊推进破局'**
  String get compassDiscussProgress;

  /// No description provided for @compassUpdateAction.
  ///
  /// In zh, this message translates to:
  /// **'更新当前行动'**
  String get compassUpdateAction;

  /// No description provided for @compassSnooze.
  ///
  /// In zh, this message translates to:
  /// **'暂不打扰（退避7天）'**
  String get compassSnooze;

  /// No description provided for @compassRealigned.
  ///
  /// In zh, this message translates to:
  /// **'已更新目标行动，航向已校准'**
  String get compassRealigned;

  /// No description provided for @compassDismissed.
  ///
  /// In zh, this message translates to:
  /// **'已暂不打扰，7 天内静默退避'**
  String get compassDismissed;

  /// No description provided for @compassTitle.
  ///
  /// In zh, this message translates to:
  /// **'航向偏离提醒'**
  String get compassTitle;

  /// No description provided for @compassSnoozeHint.
  ///
  /// In zh, this message translates to:
  /// **'暂不打扰（7天）'**
  String get compassSnoozeHint;

  /// No description provided for @compassRollAction.
  ///
  /// In zh, this message translates to:
  /// **'滚动更新行动'**
  String get compassRollAction;

  /// No description provided for @habitDoneFallback.
  ///
  /// In zh, this message translates to:
  /// **'完成了'**
  String get habitDoneFallback;

  /// No description provided for @habitMissedFallback.
  ///
  /// In zh, this message translates to:
  /// **'还没'**
  String get habitMissedFallback;

  /// No description provided for @habitDoneTextFallback.
  ///
  /// In zh, this message translates to:
  /// **'今天完成了'**
  String get habitDoneTextFallback;

  /// No description provided for @habitMissedTextFallback.
  ///
  /// In zh, this message translates to:
  /// **'今天还没完成'**
  String get habitMissedTextFallback;

  /// No description provided for @habitCheckin.
  ///
  /// In zh, this message translates to:
  /// **'习惯问候'**
  String get habitCheckin;

  /// No description provided for @changeTime.
  ///
  /// In zh, this message translates to:
  /// **'改时间'**
  String get changeTime;

  /// No description provided for @pauseCheckins.
  ///
  /// In zh, this message translates to:
  /// **'暂停问候'**
  String get pauseCheckins;

  /// No description provided for @checkinsPaused.
  ///
  /// In zh, this message translates to:
  /// **'已暂停，可在设置页恢复'**
  String get checkinsPaused;

  /// No description provided for @askLater.
  ///
  /// In zh, this message translates to:
  /// **'晚点再问'**
  String get askLater;

  /// No description provided for @discuss.
  ///
  /// In zh, this message translates to:
  /// **'聊聊'**
  String get discuss;

  /// No description provided for @skipToday.
  ///
  /// In zh, this message translates to:
  /// **'今天不问'**
  String get skipToday;

  /// No description provided for @discussReason.
  ///
  /// In zh, this message translates to:
  /// **'聊聊原因'**
  String get discussReason;

  /// No description provided for @checkinTimeUpdated.
  ///
  /// In zh, this message translates to:
  /// **'已按新的时间继续提醒'**
  String get checkinTimeUpdated;

  /// No description provided for @checkinsUpdated.
  ///
  /// In zh, this message translates to:
  /// **'定期问候已更新'**
  String get checkinsUpdated;

  /// No description provided for @checkinDismissed.
  ///
  /// In zh, this message translates to:
  /// **'今天先不问了'**
  String get checkinDismissed;

  /// No description provided for @checkinSnoozed.
  ///
  /// In zh, this message translates to:
  /// **'已推迟，稍后再问一次'**
  String get checkinSnoozed;

  /// No description provided for @checkinSuperseded.
  ///
  /// In zh, this message translates to:
  /// **'已再次提醒'**
  String get checkinSuperseded;

  /// No description provided for @checkinExpired.
  ///
  /// In zh, this message translates to:
  /// **'这条问候已过期'**
  String get checkinExpired;

  /// No description provided for @checkinPlanClosed.
  ///
  /// In zh, this message translates to:
  /// **'这条问候的计划已关闭'**
  String get checkinPlanClosed;

  /// No description provided for @checkinRecordedEvent.
  ///
  /// In zh, this message translates to:
  /// **'已记录，并生成一条事件'**
  String get checkinRecordedEvent;

  /// No description provided for @askAfterHour.
  ///
  /// In zh, this message translates to:
  /// **'好，1 小时后再问一次'**
  String get askAfterHour;

  /// No description provided for @cognitiveInsight.
  ///
  /// In zh, this message translates to:
  /// **'认知洞察'**
  String get cognitiveInsight;

  /// No description provided for @adoptAsRule.
  ///
  /// In zh, this message translates to:
  /// **'采纳为规则'**
  String get adoptAsRule;

  /// No description provided for @enableCheckins.
  ///
  /// In zh, this message translates to:
  /// **'开启习惯问候'**
  String get enableCheckins;

  /// No description provided for @makeTodo.
  ///
  /// In zh, this message translates to:
  /// **'设为待办'**
  String get makeTodo;

  /// No description provided for @acknowledge.
  ///
  /// In zh, this message translates to:
  /// **'知道了'**
  String get acknowledge;

  /// No description provided for @insightRuleApplied.
  ///
  /// In zh, this message translates to:
  /// **'已采纳为个人规则 ✓'**
  String get insightRuleApplied;

  /// No description provided for @insightHabitEnabled.
  ///
  /// In zh, this message translates to:
  /// **'已开启习惯定时问候 ✓'**
  String get insightHabitEnabled;

  /// No description provided for @insightTodoAdded.
  ///
  /// In zh, this message translates to:
  /// **'已加入待办清单 ✓'**
  String get insightTodoAdded;

  /// No description provided for @insightHandled.
  ///
  /// In zh, this message translates to:
  /// **'已处理 ✓'**
  String get insightHandled;

  /// No description provided for @insightDismissed.
  ///
  /// In zh, this message translates to:
  /// **'已了解'**
  String get insightDismissed;

  /// No description provided for @insightExpired.
  ///
  /// In zh, this message translates to:
  /// **'这条洞察已搁置'**
  String get insightExpired;

  /// No description provided for @personalRuleAdopted.
  ///
  /// In zh, this message translates to:
  /// **'已采纳为个人规则'**
  String get personalRuleAdopted;

  /// No description provided for @addedToTodoList.
  ///
  /// In zh, this message translates to:
  /// **'已加入待办清单'**
  String get addedToTodoList;

  /// No description provided for @recorded.
  ///
  /// In zh, this message translates to:
  /// **'已记录'**
  String get recorded;

  /// No description provided for @pageNotFoundDeleted.
  ///
  /// In zh, this message translates to:
  /// **'页面不存在或已删除'**
  String get pageNotFoundDeleted;

  /// No description provided for @habitCheckinsEnabled.
  ///
  /// In zh, this message translates to:
  /// **'已开启习惯定时问候'**
  String get habitCheckinsEnabled;

  /// No description provided for @contactsSuggestionInvalid.
  ///
  /// In zh, this message translates to:
  /// **'有一条联系人关联建议，但数据无法解析。'**
  String get contactsSuggestionInvalid;

  /// No description provided for @choose.
  ///
  /// In zh, this message translates to:
  /// **'选择'**
  String get choose;

  /// No description provided for @contactArgsInvalid.
  ///
  /// In zh, this message translates to:
  /// **'动作参数格式异常，无法选择联系人'**
  String get contactArgsInvalid;

  /// No description provided for @contactArgsUnreadable.
  ///
  /// In zh, this message translates to:
  /// **'动作参数无法解析，无法选择联系人'**
  String get contactArgsUnreadable;

  /// No description provided for @selectionCancelledUnsaved.
  ///
  /// In zh, this message translates to:
  /// **'已取消选择，未保存本次修改'**
  String get selectionCancelledUnsaved;

  /// No description provided for @wikiEmptyHelp.
  ///
  /// In zh, this message translates to:
  /// **'知识库还没有页面\n\n去「导入」tab 粘贴网址或文本，\n或在对话里说「导入 xxx 到知识库」'**
  String get wikiEmptyHelp;

  /// No description provided for @sortRecentlyUpdated.
  ///
  /// In zh, this message translates to:
  /// **'按最近更新'**
  String get sortRecentlyUpdated;

  /// No description provided for @loadFailed.
  ///
  /// In zh, this message translates to:
  /// **'加载失败'**
  String get loadFailed;

  /// No description provided for @reminderTenMinutes.
  ///
  /// In zh, this message translates to:
  /// **'+10分钟'**
  String get reminderTenMinutes;

  /// No description provided for @done.
  ///
  /// In zh, this message translates to:
  /// **'完成'**
  String get done;

  /// No description provided for @dismissReminderHint.
  ///
  /// In zh, this message translates to:
  /// **'关闭（不标记完成）'**
  String get dismissReminderHint;

  /// No description provided for @bodyRequired.
  ///
  /// In zh, this message translates to:
  /// **'正文不能为空'**
  String get bodyRequired;

  /// No description provided for @topicTitleHint.
  ///
  /// In zh, this message translates to:
  /// **'标题（可选，留空自动取正文首行前 60 字）'**
  String get topicTitleHint;

  /// No description provided for @topicTagsHint.
  ///
  /// In zh, this message translates to:
  /// **'标签（可选，空格或逗号分隔，例如：理财 副业）'**
  String get topicTagsHint;

  /// No description provided for @markdownBodyHint.
  ///
  /// In zh, this message translates to:
  /// **'正文（Markdown 支持）'**
  String get markdownBodyHint;

  /// No description provided for @saveToTopics.
  ///
  /// In zh, this message translates to:
  /// **'保存到主题库'**
  String get saveToTopics;

  /// No description provided for @materialScope.
  ///
  /// In zh, this message translates to:
  /// **'材料范围：{task}{range}'**
  String materialScope(String task, String range);

  /// No description provided for @materialRange.
  ///
  /// In zh, this message translates to:
  /// **' · {start}–{end}字'**
  String materialRange(int start, int end);

  /// No description provided for @materialTitleCount.
  ///
  /// In zh, this message translates to:
  /// **'{title} · {count} 字'**
  String materialTitleCount(String title, int count);

  /// No description provided for @selectionHint.
  ///
  /// In zh, this message translates to:
  /// **'在下方原文选择要处理的文字（最多6000字）。当前：{selection}'**
  String selectionHint(String selection);

  /// No description provided for @compassPlanTitle.
  ///
  /// In zh, this message translates to:
  /// **'航向校准 · 计划「{plan}」'**
  String compassPlanTitle(String plan);

  /// No description provided for @compassGoalTitle.
  ///
  /// In zh, this message translates to:
  /// **'航向校准 · 目标「{goal}」'**
  String compassGoalTitle(String goal);

  /// No description provided for @compassStalled.
  ///
  /// In zh, this message translates to:
  /// **'停滞 {days} 天'**
  String compassStalled(int days);

  /// No description provided for @compassActionStalled.
  ///
  /// In zh, this message translates to:
  /// **'当前行动「{action}」已停滞 {days} 天'**
  String compassActionStalled(String action, int days);

  /// No description provided for @compassObstacle.
  ///
  /// In zh, this message translates to:
  /// **'，困难：{obstacle}'**
  String compassObstacle(String obstacle);

  /// No description provided for @compassRecentTopics.
  ///
  /// In zh, this message translates to:
  /// **'。近期注意力多在「{topics}」。'**
  String compassRecentTopics(String topics);

  /// No description provided for @compassObstacleLine.
  ///
  /// In zh, this message translates to:
  /// **'当前困难：{obstacle}'**
  String compassObstacleLine(String obstacle);

  /// No description provided for @compassSeedObstacle.
  ///
  /// In zh, this message translates to:
  /// **'关于目标「{goal}」卡在「{obstacle}」，当前行动是「{action}」，我们聊聊怎么破局？'**
  String compassSeedObstacle(String goal, String obstacle, String action);

  /// No description provided for @compassSeedProgress.
  ///
  /// In zh, this message translates to:
  /// **'关于目标「{goal}」，当前行动是「{action}」，近几天有些停滞，我们聊聊怎么推进？'**
  String compassSeedProgress(String goal, String action);

  /// No description provided for @compassPlanStep.
  ///
  /// In zh, this message translates to:
  /// **'计划「{plan}」· 步骤'**
  String compassPlanStep(String plan);

  /// No description provided for @compassCardDetail.
  ///
  /// In zh, this message translates to:
  /// **'{prefix}「{goal}」当前行动「{action}」已连续 {days} 天未推进，近期精力分散在「{topics}」。'**
  String compassCardDetail(
    String prefix,
    String goal,
    String action,
    int days,
    String topics,
  );

  /// No description provided for @recordedText.
  ///
  /// In zh, this message translates to:
  /// **'已记录：{text}'**
  String recordedText(String text);

  /// No description provided for @checkinDiscussSeed.
  ///
  /// In zh, this message translates to:
  /// **'{text}，因为'**
  String checkinDiscussSeed(String text);

  /// No description provided for @insightSuggestion.
  ///
  /// In zh, this message translates to:
  /// **'建议：{action}'**
  String insightSuggestion(String action);

  /// No description provided for @insightSource.
  ///
  /// In zh, this message translates to:
  /// **'来源：{slug}'**
  String insightSource(String slug);

  /// No description provided for @contactsSuggestion.
  ///
  /// In zh, this message translates to:
  /// **'发现联系人：{people}\n关联事项：{targets}\n请打开“审阅提议”，接受或拒绝这项建议。'**
  String contactsSuggestion(String people, String targets);

  /// No description provided for @chooseContactName.
  ///
  /// In zh, this message translates to:
  /// **'选择联系人「{name}」'**
  String chooseContactName(String name);

  /// No description provided for @chooseTargetName.
  ///
  /// In zh, this message translates to:
  /// **'选择事项「{name}」'**
  String chooseTargetName(String name);

  /// No description provided for @recentPageCount.
  ///
  /// In zh, this message translates to:
  /// **'最近 {count} 条'**
  String recentPageCount(int count);

  /// No description provided for @incubatingCount.
  ///
  /// In zh, this message translates to:
  /// **'孵化中 {count} 条 · 待定论'**
  String incubatingCount(int count);

  /// No description provided for @evidenceCount.
  ///
  /// In zh, this message translates to:
  /// **'证据 {count}'**
  String evidenceCount(int count);

  /// No description provided for @reminderTitle.
  ///
  /// In zh, this message translates to:
  /// **'提醒：{title}（{time}）'**
  String reminderTitle(String title, String time);

  /// No description provided for @topicSavedTitle.
  ///
  /// In zh, this message translates to:
  /// **'已存为主题：《{title}》'**
  String topicSavedTitle(String title);

  /// No description provided for @copyPageTitle.
  ///
  /// In zh, this message translates to:
  /// **'复制标题'**
  String get copyPageTitle;

  /// No description provided for @pageTitleCopied.
  ///
  /// In zh, this message translates to:
  /// **'页面标题已复制'**
  String get pageTitleCopied;

  /// No description provided for @readPageFailed.
  ///
  /// In zh, this message translates to:
  /// **'读取知识页出错'**
  String get readPageFailed;

  /// No description provided for @pageBodyEmpty.
  ///
  /// In zh, this message translates to:
  /// **'*(正文为空)*'**
  String get pageBodyEmpty;

  /// No description provided for @knowledgePageMissing.
  ///
  /// In zh, this message translates to:
  /// **'知识页未找到或尚未生成'**
  String get knowledgePageMissing;

  /// No description provided for @knowledgePageMissingHelp.
  ///
  /// In zh, this message translates to:
  /// **'该条目可能已被删除、重命名，或者是由 AI 回复提及的建议标识符，尚未正式写入知识库。'**
  String get knowledgePageMissingHelp;

  /// No description provided for @openInKnowledgeBase.
  ///
  /// In zh, this message translates to:
  /// **'在知识库中打开'**
  String get openInKnowledgeBase;

  /// No description provided for @checkinQuestionRequired.
  ///
  /// In zh, this message translates to:
  /// **'问法不能为空'**
  String get checkinQuestionRequired;

  /// No description provided for @checkinTimeInvalid.
  ///
  /// In zh, this message translates to:
  /// **'时间需在 07:00–22:00 之间，格式 HH:MM'**
  String get checkinTimeInvalid;

  /// No description provided for @checkinDayRequired.
  ///
  /// In zh, this message translates to:
  /// **'至少选择一天'**
  String get checkinDayRequired;

  /// No description provided for @checkinResponsesRequired.
  ///
  /// In zh, this message translates to:
  /// **'按钮和记录句都不能为空'**
  String get checkinResponsesRequired;

  /// No description provided for @closeCheckinsQuestion.
  ///
  /// In zh, this message translates to:
  /// **'关闭定期问候？'**
  String get closeCheckinsQuestion;

  /// No description provided for @closeCheckinsHelp.
  ///
  /// In zh, this message translates to:
  /// **'已产生的历史问候和回答会保留，之后不再主动问你。'**
  String get closeCheckinsHelp;

  /// No description provided for @askRegularly.
  ///
  /// In zh, this message translates to:
  /// **'定期问我'**
  String get askRegularly;

  /// No description provided for @adjustCheckins.
  ///
  /// In zh, this message translates to:
  /// **'调整定期问候'**
  String get adjustCheckins;

  /// No description provided for @checkinHelp.
  ///
  /// In zh, this message translates to:
  /// **'到点后我会在主对话里发一条消息。所有内容先由你确认，不会自动修改习惯页。'**
  String get checkinHelp;

  /// No description provided for @checkinSyncHelp.
  ///
  /// In zh, this message translates to:
  /// **'多设备同步时只有主设备会在到点后写入问候；其他设备同步显示，不会重复发问。'**
  String get checkinSyncHelp;

  /// No description provided for @inferenceBasis.
  ///
  /// In zh, this message translates to:
  /// **'推断依据'**
  String get inferenceBasis;

  /// No description provided for @checkinQuestion.
  ///
  /// In zh, this message translates to:
  /// **'问法'**
  String get checkinQuestion;

  /// No description provided for @checkinQuestionHint.
  ///
  /// In zh, this message translates to:
  /// **'例如：今天午饭后散步了吗？'**
  String get checkinQuestionHint;

  /// No description provided for @checkinTime.
  ///
  /// In zh, this message translates to:
  /// **'每天什么时候问'**
  String get checkinTime;

  /// No description provided for @repeat.
  ///
  /// In zh, this message translates to:
  /// **'重复'**
  String get repeat;

  /// No description provided for @everyDay.
  ///
  /// In zh, this message translates to:
  /// **'每天'**
  String get everyDay;

  /// No description provided for @checkinDoneFields.
  ///
  /// In zh, this message translates to:
  /// **'完成按钮与记录'**
  String get checkinDoneFields;

  /// No description provided for @checkinMissedFields.
  ///
  /// In zh, this message translates to:
  /// **'未完成按钮与记录'**
  String get checkinMissedFields;

  /// No description provided for @walkDoneHint.
  ///
  /// In zh, this message translates to:
  /// **'走了'**
  String get walkDoneHint;

  /// No description provided for @walkDoneTextHint.
  ///
  /// In zh, this message translates to:
  /// **'今天午饭后散步了'**
  String get walkDoneTextHint;

  /// No description provided for @walkMissedHint.
  ///
  /// In zh, this message translates to:
  /// **'没走'**
  String get walkMissedHint;

  /// No description provided for @walkMissedTextHint.
  ///
  /// In zh, this message translates to:
  /// **'今天还没走'**
  String get walkMissedTextHint;

  /// No description provided for @checkinRecordHelp.
  ///
  /// In zh, this message translates to:
  /// **'按钮文字尽量短；记录句会作为一条真实记录写入，「没完成」默认只留在对话里，不生成事件。'**
  String get checkinRecordHelp;

  /// No description provided for @closeCheckins.
  ///
  /// In zh, this message translates to:
  /// **'关闭定期问候'**
  String get closeCheckins;

  /// No description provided for @feedbackSuggestion.
  ///
  /// In zh, this message translates to:
  /// **'反馈这条建议'**
  String get feedbackSuggestion;

  /// No description provided for @feedbackHelp.
  ///
  /// In zh, this message translates to:
  /// **'选择回复中的建议原文。接受表示认可建议，实际行动仍需另行确认。'**
  String get feedbackHelp;

  /// No description provided for @feedbackOriginal.
  ///
  /// In zh, this message translates to:
  /// **'建议原文（可缩小到其中一段）'**
  String get feedbackOriginal;

  /// No description provided for @feedbackRewrite.
  ///
  /// In zh, this message translates to:
  /// **'我的改写（可选）'**
  String get feedbackRewrite;

  /// No description provided for @feedbackAccept.
  ///
  /// In zh, this message translates to:
  /// **'接受'**
  String get feedbackAccept;

  /// No description provided for @feedbackIgnore.
  ///
  /// In zh, this message translates to:
  /// **'忽略'**
  String get feedbackIgnore;

  /// No description provided for @feedbackSaveRewrite.
  ///
  /// In zh, this message translates to:
  /// **'保存改写'**
  String get feedbackSaveRewrite;

  /// No description provided for @feedbackWithdraw.
  ///
  /// In zh, this message translates to:
  /// **'撤回反馈'**
  String get feedbackWithdraw;

  /// No description provided for @feedbackRewritten.
  ///
  /// In zh, this message translates to:
  /// **'改写'**
  String get feedbackRewritten;

  /// No description provided for @feedbackWithdrawn.
  ///
  /// In zh, this message translates to:
  /// **'撤回'**
  String get feedbackWithdrawn;

  /// No description provided for @pageKindProfile.
  ///
  /// In zh, this message translates to:
  /// **'档案'**
  String get pageKindProfile;

  /// No description provided for @pageKindPerson.
  ///
  /// In zh, this message translates to:
  /// **'联系人'**
  String get pageKindPerson;

  /// No description provided for @pageKindRecurringCost.
  ///
  /// In zh, this message translates to:
  /// **'固定成本'**
  String get pageKindRecurringCost;

  /// No description provided for @pageKindCapability.
  ///
  /// In zh, this message translates to:
  /// **'能力'**
  String get pageKindCapability;

  /// No description provided for @pageKindAsset.
  ///
  /// In zh, this message translates to:
  /// **'资产'**
  String get pageKindAsset;

  /// No description provided for @pageKindProject.
  ///
  /// In zh, this message translates to:
  /// **'项目'**
  String get pageKindProject;

  /// No description provided for @pageKindRelationship.
  ///
  /// In zh, this message translates to:
  /// **'关系'**
  String get pageKindRelationship;

  /// No description provided for @pageKindDecision.
  ///
  /// In zh, this message translates to:
  /// **'决策'**
  String get pageKindDecision;

  /// No description provided for @pageKindHabit.
  ///
  /// In zh, this message translates to:
  /// **'习惯'**
  String get pageKindHabit;

  /// No description provided for @pageKindConstraint.
  ///
  /// In zh, this message translates to:
  /// **'约束'**
  String get pageKindConstraint;

  /// No description provided for @pageKindInsight.
  ///
  /// In zh, this message translates to:
  /// **'洞察'**
  String get pageKindInsight;

  /// No description provided for @pageKindTopic.
  ///
  /// In zh, this message translates to:
  /// **'主题'**
  String get pageKindTopic;

  /// No description provided for @pageKindMethod.
  ///
  /// In zh, this message translates to:
  /// **'方法'**
  String get pageKindMethod;

  /// No description provided for @pageKindCase.
  ///
  /// In zh, this message translates to:
  /// **'案例'**
  String get pageKindCase;

  /// No description provided for @pageKindPrinciple.
  ///
  /// In zh, this message translates to:
  /// **'规律'**
  String get pageKindPrinciple;

  /// No description provided for @pageKindSeries.
  ///
  /// In zh, this message translates to:
  /// **'系列'**
  String get pageKindSeries;

  /// No description provided for @pageKindSource.
  ///
  /// In zh, this message translates to:
  /// **'来源'**
  String get pageKindSource;

  /// No description provided for @checkinDaysPerWeek.
  ///
  /// In zh, this message translates to:
  /// **'每周 {count} 天'**
  String checkinDaysPerWeek(int count);

  /// No description provided for @weekdayShort.
  ///
  /// In zh, this message translates to:
  /// **'{day, select, 1{一} 2{二} 3{三} 4{四} 5{五} 6{六} other{日}}'**
  String weekdayShort(String day);

  /// No description provided for @load.
  ///
  /// In zh, this message translates to:
  /// **'读取'**
  String get load;

  /// No description provided for @promptTemplateLanguage.
  ///
  /// In zh, this message translates to:
  /// **'模板语言'**
  String get promptTemplateLanguage;

  /// No description provided for @promptIncompatibleFallback.
  ///
  /// In zh, this message translates to:
  /// **'当前修订不兼容，使用内置默认'**
  String get promptIncompatibleFallback;

  /// No description provided for @promptParentFallback.
  ///
  /// In zh, this message translates to:
  /// **'使用父语言资源：{requested} → {resolved}'**
  String promptParentFallback(String requested, String resolved);

  /// No description provided for @promptMissingTranslation.
  ///
  /// In zh, this message translates to:
  /// **'缺少译文，使用内置默认：{requested} → {resolved}'**
  String promptMissingTranslation(String requested, String resolved);

  /// No description provided for @aiResponseLanguage.
  ///
  /// In zh, this message translates to:
  /// **'AI 回答语言'**
  String get aiResponseLanguage;

  /// No description provided for @aiInstructionLanguage.
  ///
  /// In zh, this message translates to:
  /// **'指令语言'**
  String get aiInstructionLanguage;

  /// No description provided for @languageAutomatic.
  ///
  /// In zh, this message translates to:
  /// **'自动'**
  String get languageAutomatic;

  /// No description provided for @instructionFollowContext.
  ///
  /// In zh, this message translates to:
  /// **'使用默认策略'**
  String get instructionFollowContext;

  /// No description provided for @languagePolicyHelp.
  ///
  /// In zh, this message translates to:
  /// **'回答自动模式根据本轮与最近可靠会话语言决定。默认指令在对话中跟随界面，在后台跟随知识语言；设置变更只影响新运行。'**
  String get languagePolicyHelp;

  /// No description provided for @knowledgeLanguageReadonly.
  ///
  /// In zh, this message translates to:
  /// **'知识产出语言：{locale}（当前只读）'**
  String knowledgeLanguageReadonly(String locale);

  /// No description provided for @runLanguageUi.
  ///
  /// In zh, this message translates to:
  /// **'界面语言：{locale}'**
  String runLanguageUi(String locale);

  /// No description provided for @runLanguageInstructions.
  ///
  /// In zh, this message translates to:
  /// **'指令语言：{locale}'**
  String runLanguageInstructions(String locale);

  /// No description provided for @runLanguageResponse.
  ///
  /// In zh, this message translates to:
  /// **'回答语言：{locale} · {source}'**
  String runLanguageResponse(String locale, String source);

  /// No description provided for @runLanguageKnowledge.
  ///
  /// In zh, this message translates to:
  /// **'知识产出语言：{locale}'**
  String runLanguageKnowledge(String locale);

  /// No description provided for @runLanguageFallback.
  ///
  /// In zh, this message translates to:
  /// **'回退：{reason}'**
  String runLanguageFallback(String reason);

  /// No description provided for @languageSourceExplicit.
  ///
  /// In zh, this message translates to:
  /// **'本轮明确要求'**
  String get languageSourceExplicit;

  /// No description provided for @languageSourcePreference.
  ///
  /// In zh, this message translates to:
  /// **'固定偏好'**
  String get languageSourcePreference;

  /// No description provided for @languageSourceCurrent.
  ///
  /// In zh, this message translates to:
  /// **'本轮交流语言'**
  String get languageSourceCurrent;

  /// No description provided for @languageSourceConversation.
  ///
  /// In zh, this message translates to:
  /// **'沿用会话语言'**
  String get languageSourceConversation;

  /// No description provided for @languageSourceFallback.
  ///
  /// In zh, this message translates to:
  /// **'默认回退'**
  String get languageSourceFallback;

  /// No description provided for @languageSourceDataset.
  ///
  /// In zh, this message translates to:
  /// **'数据集策略'**
  String get languageSourceDataset;

  /// No description provided for @languageSourceLegacy.
  ///
  /// In zh, this message translates to:
  /// **'历史策略'**
  String get languageSourceLegacy;

  /// No description provided for @promptContextContract.
  ///
  /// In zh, this message translates to:
  /// **'对话场景与档案协议'**
  String get promptContextContract;

  /// No description provided for @promptToolDescriptionsContract.
  ///
  /// In zh, this message translates to:
  /// **'工具说明协议'**
  String get promptToolDescriptionsContract;

  /// No description provided for @promptToolMetadataContract.
  ///
  /// In zh, this message translates to:
  /// **'技能目录与工具启用协议'**
  String get promptToolMetadataContract;

  /// No description provided for @promptExecutionContract.
  ///
  /// In zh, this message translates to:
  /// **'运行修复与页内处理协议'**
  String get promptExecutionContract;

  /// No description provided for @promptProcessingContract.
  ///
  /// In zh, this message translates to:
  /// **'后台处理与分析协议'**
  String get promptProcessingContract;

  /// No description provided for @promptReplyContract.
  ///
  /// In zh, this message translates to:
  /// **'对话答复与待审状态文案'**
  String get promptReplyContract;

  /// No description provided for @promptKnowledgeProcessingContract.
  ///
  /// In zh, this message translates to:
  /// **'知识整理与主题维护协议'**
  String get promptKnowledgeProcessingContract;

  /// No description provided for @promptLookupContract.
  ///
  /// In zh, this message translates to:
  /// **'本机数据直查文案'**
  String get promptLookupContract;

  /// No description provided for @promptSupplementContract.
  ///
  /// In zh, this message translates to:
  /// **'任务补充材料与引用协议'**
  String get promptSupplementContract;

  /// No description provided for @knowledgeLanguage.
  ///
  /// In zh, this message translates to:
  /// **'知识语言'**
  String get knowledgeLanguage;

  /// No description provided for @knowledgeLanguageHelp.
  ///
  /// In zh, this message translates to:
  /// **'新知识使用此语言。已开始的任务沿用原语言，已有页面和原文保持不变；修改后不会重新处理整个知识库。'**
  String get knowledgeLanguageHelp;

  /// No description provided for @knowledgeLanguageUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'当前版本不支持此数据集的语言策略，知识生成已暂停。请更新应用后继续。'**
  String get knowledgeLanguageUnsupported;

  /// No description provided for @knowledgeLanguageOwnerOnly.
  ///
  /// In zh, this message translates to:
  /// **'只有数据集的主设备可以修改。'**
  String get knowledgeLanguageOwnerOnly;

  /// No description provided for @knowledgeLanguageCapabilityRequired.
  ///
  /// In zh, this message translates to:
  /// **'修改知识语言前，请连接并更新所有已配对设备。'**
  String get knowledgeLanguageCapabilityRequired;

  /// No description provided for @title.
  ///
  /// In zh, this message translates to:
  /// **'标题'**
  String get title;

  /// No description provided for @copy.
  ///
  /// In zh, this message translates to:
  /// **'复制'**
  String get copy;

  /// No description provided for @todoInputHint.
  ///
  /// In zh, this message translates to:
  /// **'记一条要跟进的事…（回车添加）'**
  String get todoInputHint;

  /// No description provided for @todoNoteHint.
  ///
  /// In zh, this message translates to:
  /// **'补充说明（可选）'**
  String get todoNoteHint;

  /// No description provided for @todoEmptyHelp.
  ///
  /// In zh, this message translates to:
  /// **'还没有待办\n\n可以手动添加，或在对话里让 AI 分析出\n需要跟进的事（需确认后创建）'**
  String get todoEmptyHelp;

  /// No description provided for @todoOpenGroup.
  ///
  /// In zh, this message translates to:
  /// **'进行中 · {count}'**
  String todoOpenGroup(int count);

  /// No description provided for @todoDoneGroup.
  ///
  /// In zh, this message translates to:
  /// **'已完成 · {count}'**
  String todoDoneGroup(int count);

  /// No description provided for @todoEdit.
  ///
  /// In zh, this message translates to:
  /// **'编辑待办'**
  String get todoEdit;

  /// No description provided for @todoTitleHint.
  ///
  /// In zh, this message translates to:
  /// **'要跟进的事…'**
  String get todoTitleHint;

  /// No description provided for @todoDueHint.
  ///
  /// In zh, this message translates to:
  /// **'截止日期（可选，YYYY-MM-DD，留空清除）'**
  String get todoDueHint;

  /// No description provided for @todoReminderOption.
  ///
  /// In zh, this message translates to:
  /// **'到期提醒（到时弹横幅/通知）'**
  String get todoReminderOption;

  /// No description provided for @todoPriority.
  ///
  /// In zh, this message translates to:
  /// **'优先级'**
  String get todoPriority;

  /// No description provided for @priorityHigh.
  ///
  /// In zh, this message translates to:
  /// **'高'**
  String get priorityHigh;

  /// No description provided for @priorityNormal.
  ///
  /// In zh, this message translates to:
  /// **'中'**
  String get priorityNormal;

  /// No description provided for @priorityLow.
  ///
  /// In zh, this message translates to:
  /// **'低'**
  String get priorityLow;

  /// No description provided for @todoDueDate.
  ///
  /// In zh, this message translates to:
  /// **'截止 {date}'**
  String todoDueDate(String date);

  /// No description provided for @todoReminderDate.
  ///
  /// In zh, this message translates to:
  /// **'提醒 {date}'**
  String todoReminderDate(String date);

  /// No description provided for @todoStatusOpen.
  ///
  /// In zh, this message translates to:
  /// **'进行中'**
  String get todoStatusOpen;

  /// No description provided for @todoStatusArchived.
  ///
  /// In zh, this message translates to:
  /// **'已归档'**
  String get todoStatusArchived;

  /// No description provided for @errorFieldRequired.
  ///
  /// In zh, this message translates to:
  /// **'{field}不能为空。'**
  String errorFieldRequired(String field);

  /// No description provided for @errorNotFound.
  ///
  /// In zh, this message translates to:
  /// **'找不到{resource}，请刷新后重试。'**
  String errorNotFound(String resource);

  /// No description provided for @errorSettingsChanged.
  ///
  /// In zh, this message translates to:
  /// **'设置已更新，请刷新后重新修改。'**
  String get errorSettingsChanged;

  /// No description provided for @errorSyncUnlockRequired.
  ///
  /// In zh, this message translates to:
  /// **'请先解锁同步，再修改此设置。'**
  String get errorSyncUnlockRequired;

  /// No description provided for @errorLanguageUnsupported.
  ///
  /// In zh, this message translates to:
  /// **'当前版本不支持所选语言或策略，请更新应用后重试。'**
  String get errorLanguageUnsupported;

  /// No description provided for @errorReminderTimePast.
  ///
  /// In zh, this message translates to:
  /// **'提醒时间已过，请先设置一个未来时间。'**
  String get errorReminderTimePast;

  /// No description provided for @errorReminderTimeRequired.
  ///
  /// In zh, this message translates to:
  /// **'请先设置截止时间，再开启提醒。'**
  String get errorReminderTimeRequired;

  /// No description provided for @errorDetails.
  ///
  /// In zh, this message translates to:
  /// **'查看详情'**
  String get errorDetails;

  /// No description provided for @open.
  ///
  /// In zh, this message translates to:
  /// **'打开'**
  String get open;

  /// No description provided for @knowledgePage.
  ///
  /// In zh, this message translates to:
  /// **'知识页'**
  String get knowledgePage;

  /// No description provided for @habitReminderSettings.
  ///
  /// In zh, this message translates to:
  /// **'习惯提醒'**
  String get habitReminderSettings;

  /// No description provided for @habitReminderSettingsHelp.
  ///
  /// In zh, this message translates to:
  /// **'这里汇总所有未归档习惯页的定期问候。时间、问法和按钮改动会直接保存到对应习惯页；主设备到点后把问候写进主对话，其他设备只同步显示，不会重复发问。'**
  String get habitReminderSettingsHelp;

  /// No description provided for @habitReminderEmpty.
  ///
  /// In zh, this message translates to:
  /// **'还没有未归档的习惯页。先在知识库创建「习惯」页，再回到这里设置定期问候。'**
  String get habitReminderEmpty;

  /// No description provided for @habitReminderCounts.
  ///
  /// In zh, this message translates to:
  /// **'运行中 {enabled} · 已暂停 {paused} · 未设置 {unset}'**
  String habitReminderCounts(int enabled, int paused, int unset);

  /// No description provided for @habitReminderActive.
  ///
  /// In zh, this message translates to:
  /// **'运行中'**
  String get habitReminderActive;

  /// No description provided for @habitReminderUnset.
  ///
  /// In zh, this message translates to:
  /// **'尚未设置定期问候'**
  String get habitReminderUnset;

  /// No description provided for @habitReminderSetup.
  ///
  /// In zh, this message translates to:
  /// **'设置'**
  String get habitReminderSetup;

  /// No description provided for @habitReminderAdjust.
  ///
  /// In zh, this message translates to:
  /// **'调整'**
  String get habitReminderAdjust;

  /// No description provided for @habitReminderResumed.
  ///
  /// In zh, this message translates to:
  /// **'定期问候已恢复'**
  String get habitReminderResumed;

  /// No description provided for @habitReminderPaused.
  ///
  /// In zh, this message translates to:
  /// **'定期问候已暂停'**
  String get habitReminderPaused;

  /// No description provided for @habitWeekSchedule.
  ///
  /// In zh, this message translates to:
  /// **'每周 {days}'**
  String habitWeekSchedule(String days);

  /// No description provided for @habitReminderNotSet.
  ///
  /// In zh, this message translates to:
  /// **'未设置'**
  String get habitReminderNotSet;

  /// No description provided for @trayShowWindow.
  ///
  /// In zh, this message translates to:
  /// **'显示主窗口'**
  String get trayShowWindow;

  /// No description provided for @trayQuit.
  ///
  /// In zh, this message translates to:
  /// **'退出 Elsewhen'**
  String get trayQuit;

  /// No description provided for @syncKeepAliveDefault.
  ///
  /// In zh, this message translates to:
  /// **'正在与另一台设备同步；请保持应用运行。'**
  String get syncKeepAliveDefault;

  /// No description provided for @syncKeepAliveSetup.
  ///
  /// In zh, this message translates to:
  /// **'正在准备本机同步身份'**
  String get syncKeepAliveSetup;

  /// No description provided for @syncKeepAlivePairOwner.
  ///
  /// In zh, this message translates to:
  /// **'正在与加入设备配对'**
  String get syncKeepAlivePairOwner;

  /// No description provided for @syncKeepAlivePairJoin.
  ///
  /// In zh, this message translates to:
  /// **'正在与已有设备配对'**
  String get syncKeepAlivePairJoin;

  /// No description provided for @syncKeepAliveEnrollment.
  ///
  /// In zh, this message translates to:
  /// **'正在完成设备交接与初始同步'**
  String get syncKeepAliveEnrollment;

  /// No description provided for @syncNotificationTitle.
  ///
  /// In zh, this message translates to:
  /// **'Elsewhen 正在同步'**
  String get syncNotificationTitle;

  /// No description provided for @syncNotificationChannel.
  ///
  /// In zh, this message translates to:
  /// **'设备同步'**
  String get syncNotificationChannel;

  /// No description provided for @syncNotificationDescription.
  ///
  /// In zh, this message translates to:
  /// **'配对与初始快照传输期间保持应用运行'**
  String get syncNotificationDescription;

  /// No description provided for @syncObjectConflictsTitle.
  ///
  /// In zh, this message translates to:
  /// **'待办与目标同步冲突'**
  String get syncObjectConflictsTitle;

  /// No description provided for @syncObjectConflictsHelp.
  ///
  /// In zh, this message translates to:
  /// **'来自其他设备的待办、目标、计划、规则或知识语言策略与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。'**
  String get syncObjectConflictsHelp;

  /// No description provided for @syncObjectConflictsEmpty.
  ///
  /// In zh, this message translates to:
  /// **'没有待审阅的待办/目标/计划/规则或知识语言冲突。'**
  String get syncObjectConflictsEmpty;

  /// No description provided for @syncConflictFrom.
  ///
  /// In zh, this message translates to:
  /// **'{kind} · 来自 {device} 的改动'**
  String syncConflictFrom(String kind, String device);

  /// No description provided for @syncKindTodo.
  ///
  /// In zh, this message translates to:
  /// **'待办'**
  String get syncKindTodo;

  /// No description provided for @syncKindGoal.
  ///
  /// In zh, this message translates to:
  /// **'目标'**
  String get syncKindGoal;

  /// No description provided for @syncKindPlan.
  ///
  /// In zh, this message translates to:
  /// **'计划'**
  String get syncKindPlan;

  /// No description provided for @syncKindRule.
  ///
  /// In zh, this message translates to:
  /// **'规则'**
  String get syncKindRule;

  /// No description provided for @syncDeletedReason.
  ///
  /// In zh, this message translates to:
  /// **'对端已删除本条，而之后仍有改动要送达'**
  String get syncDeletedReason;

  /// No description provided for @syncGoalDeletedReason.
  ///
  /// In zh, this message translates to:
  /// **'对端已删除本条（目标无删除语义，删除后收到编辑必审）'**
  String get syncGoalDeletedReason;

  /// No description provided for @syncBaseChangedReason.
  ///
  /// In zh, this message translates to:
  /// **'两端都修改了同一条，各自基于旧版本'**
  String get syncBaseChangedReason;

  /// No description provided for @syncGoalLimitReason.
  ///
  /// In zh, this message translates to:
  /// **'对端目标会使本端活跃目标超出上限'**
  String get syncGoalLimitReason;

  /// No description provided for @syncLanguageChangedReason.
  ///
  /// In zh, this message translates to:
  /// **'知识语言策略的版本不一致，请核对主设备签名的版本。'**
  String get syncLanguageChangedReason;

  /// No description provided for @syncLocalDeleted.
  ///
  /// In zh, this message translates to:
  /// **'本端（已删除状态）'**
  String get syncLocalDeleted;

  /// No description provided for @syncLocalCurrent.
  ///
  /// In zh, this message translates to:
  /// **'本端当前'**
  String get syncLocalCurrent;

  /// No description provided for @syncIncomingVersion.
  ///
  /// In zh, this message translates to:
  /// **'对端送来的版本'**
  String get syncIncomingVersion;

  /// No description provided for @syncKeepLocal.
  ///
  /// In zh, this message translates to:
  /// **'保留本端'**
  String get syncKeepLocal;

  /// No description provided for @syncAcceptIncoming.
  ///
  /// In zh, this message translates to:
  /// **'采纳对端'**
  String get syncAcceptIncoming;

  /// No description provided for @syncDeleted.
  ///
  /// In zh, this message translates to:
  /// **'已删除'**
  String get syncDeleted;

  /// No description provided for @syncEmptyValue.
  ///
  /// In zh, this message translates to:
  /// **'（空）'**
  String get syncEmptyValue;

  /// No description provided for @syncStatusUpdated.
  ///
  /// In zh, this message translates to:
  /// **'状态 {status} · 更新 {updated}'**
  String syncStatusUpdated(String status, String updated);

  /// No description provided for @syncRefresh.
  ///
  /// In zh, this message translates to:
  /// **'重新拉取'**
  String get syncRefresh;

  /// No description provided for @syncLoadingConflicts.
  ///
  /// In zh, this message translates to:
  /// **'正在读取冲突…'**
  String get syncLoadingConflicts;

  /// No description provided for @syncNoActionNeeded.
  ///
  /// In zh, this message translates to:
  /// **'无需处理'**
  String get syncNoActionNeeded;

  /// No description provided for @syncReviewCount.
  ///
  /// In zh, this message translates to:
  /// **'{count} 条待审阅'**
  String syncReviewCount(int count);

  /// No description provided for @errorConflictChanged.
  ///
  /// In zh, this message translates to:
  /// **'本端记录已变化，请刷新后重新审阅。'**
  String get errorConflictChanged;

  /// No description provided for @errorConflictResolved.
  ///
  /// In zh, this message translates to:
  /// **'该冲突已裁决，请刷新列表。'**
  String get errorConflictResolved;

  /// No description provided for @errorGoalLimit.
  ///
  /// In zh, this message translates to:
  /// **'本端活跃目标已满，无法直接采纳对端；请先归档目标或选择保留本端。'**
  String get errorGoalLimit;

  /// No description provided for @knowledgeReadingTitle.
  ///
  /// In zh, this message translates to:
  /// **'阅读与使用'**
  String get knowledgeReadingTitle;

  /// No description provided for @knowledgeReadingHelp.
  ///
  /// In zh, this message translates to:
  /// **'记录自己的阅读与使用进度；不会改变原文或观点认可状态。'**
  String get knowledgeReadingHelp;

  /// No description provided for @readingUnread.
  ///
  /// In zh, this message translates to:
  /// **'待读'**
  String get readingUnread;

  /// No description provided for @readingRead.
  ///
  /// In zh, this message translates to:
  /// **'已读'**
  String get readingRead;

  /// No description provided for @readingValuable.
  ///
  /// In zh, this message translates to:
  /// **'值得使用'**
  String get readingValuable;

  /// No description provided for @readingAdopted.
  ///
  /// In zh, this message translates to:
  /// **'已采用'**
  String get readingAdopted;

  /// No description provided for @readingArchived.
  ///
  /// In zh, this message translates to:
  /// **'归档'**
  String get readingArchived;

  /// No description provided for @libraryAllFilter.
  ///
  /// In zh, this message translates to:
  /// **'全部{filter}'**
  String libraryAllFilter(String filter);

  /// No description provided for @librarySearchHint.
  ///
  /// In zh, this message translates to:
  /// **'搜索标题或全文'**
  String get librarySearchHint;

  /// No description provided for @clearSearch.
  ///
  /// In zh, this message translates to:
  /// **'清除搜索'**
  String get clearSearch;

  /// No description provided for @libraryArea.
  ///
  /// In zh, this message translates to:
  /// **'分区'**
  String get libraryArea;

  /// No description provided for @libraryImported.
  ///
  /// In zh, this message translates to:
  /// **'素材库'**
  String get libraryImported;

  /// No description provided for @libraryInsight.
  ///
  /// In zh, this message translates to:
  /// **'知识沉淀'**
  String get libraryInsight;

  /// No description provided for @libraryNetwork.
  ///
  /// In zh, this message translates to:
  /// **'联系人与项目'**
  String get libraryNetwork;

  /// No description provided for @libraryKind.
  ///
  /// In zh, this message translates to:
  /// **'类型'**
  String get libraryKind;

  /// No description provided for @libraryReadingState.
  ///
  /// In zh, this message translates to:
  /// **'阅读状态'**
  String get libraryReadingState;

  /// No description provided for @tags.
  ///
  /// In zh, this message translates to:
  /// **'标签'**
  String get tags;

  /// No description provided for @librarySourceKind.
  ///
  /// In zh, this message translates to:
  /// **'原料'**
  String get librarySourceKind;

  /// No description provided for @pageKindNote.
  ///
  /// In zh, this message translates to:
  /// **'笔记'**
  String get pageKindNote;

  /// No description provided for @loadMore.
  ///
  /// In zh, this message translates to:
  /// **'加载更多'**
  String get loadMore;

  /// No description provided for @libraryNoMatch.
  ///
  /// In zh, this message translates to:
  /// **'没有匹配的知识页'**
  String get libraryNoMatch;

  /// No description provided for @libraryAllShown.
  ///
  /// In zh, this message translates to:
  /// **'已显示全部结果'**
  String get libraryAllShown;

  /// No description provided for @knowledgeBatchTitle.
  ///
  /// In zh, this message translates to:
  /// **'批量审阅修订'**
  String get knowledgeBatchTitle;

  /// No description provided for @knowledgeBatchHelp.
  ///
  /// In zh, this message translates to:
  /// **'先查看正文与适用条件，每批最多确认 20 项'**
  String get knowledgeBatchHelp;

  /// No description provided for @knowledgeViewRevision.
  ///
  /// In zh, this message translates to:
  /// **'查看本项修订'**
  String get knowledgeViewRevision;

  /// No description provided for @knowledgeAcceptCount.
  ///
  /// In zh, this message translates to:
  /// **'确认采纳 {count} 项'**
  String knowledgeAcceptCount(int count);

  /// No description provided for @knowledgeRejectSelected.
  ///
  /// In zh, this message translates to:
  /// **'拒绝所选'**
  String get knowledgeRejectSelected;

  /// No description provided for @knowledgeAccepted.
  ///
  /// In zh, this message translates to:
  /// **'已采纳'**
  String get knowledgeAccepted;

  /// No description provided for @knowledgeRejected.
  ///
  /// In zh, this message translates to:
  /// **'已拒绝'**
  String get knowledgeRejected;

  /// No description provided for @priorRejectionTitle.
  ///
  /// In zh, this message translates to:
  /// **'此前否定过这条来源'**
  String get priorRejectionTitle;

  /// No description provided for @priorRejectionReason.
  ///
  /// In zh, this message translates to:
  /// **'你曾把这条来源丢弃为负知识，当时的原因：'**
  String get priorRejectionReason;

  /// No description provided for @priorRejectionHelp.
  ///
  /// In zh, this message translates to:
  /// **'重新导入会重新入册并从孵化中重新开始；要继续吗？'**
  String get priorRejectionHelp;

  /// No description provided for @saveAnyway.
  ///
  /// In zh, this message translates to:
  /// **'仍要保存'**
  String get saveAnyway;

  /// No description provided for @sourceUpdateTitle.
  ///
  /// In zh, this message translates to:
  /// **'来源内容有更新'**
  String get sourceUpdateTitle;

  /// No description provided for @sourceUpdateHelp.
  ///
  /// In zh, this message translates to:
  /// **'确认后保存为新版本，旧原文保留；引用它的知识页会提示复核。'**
  String get sourceUpdateHelp;

  /// No description provided for @sourceSavedVersion.
  ///
  /// In zh, this message translates to:
  /// **'已保存版本'**
  String get sourceSavedVersion;

  /// No description provided for @sourceIncomingVersion.
  ///
  /// In zh, this message translates to:
  /// **'本次抓取'**
  String get sourceIncomingVersion;

  /// No description provided for @sourceKeepOld.
  ///
  /// In zh, this message translates to:
  /// **'保留旧版本'**
  String get sourceKeepOld;

  /// No description provided for @sourceSaveNew.
  ///
  /// In zh, this message translates to:
  /// **'确认保存新版本'**
  String get sourceSaveNew;

  /// No description provided for @wikiImportUrl.
  ///
  /// In zh, this message translates to:
  /// **'网址导入'**
  String get wikiImportUrl;

  /// No description provided for @wikiImportUrlHelp.
  ///
  /// In zh, this message translates to:
  /// **'抓取网页或推文，确认后保存'**
  String get wikiImportUrlHelp;

  /// No description provided for @wikiCreateTopic.
  ///
  /// In zh, this message translates to:
  /// **'创建主题'**
  String get wikiCreateTopic;

  /// No description provided for @wikiCreateTopicHelp.
  ///
  /// In zh, this message translates to:
  /// **'输入内容并创建知识页'**
  String get wikiCreateTopicHelp;

  /// No description provided for @knowledgeReview.
  ///
  /// In zh, this message translates to:
  /// **'知识审阅'**
  String get knowledgeReview;

  /// No description provided for @noPendingWork.
  ///
  /// In zh, this message translates to:
  /// **'无待办'**
  String get noPendingWork;

  /// No description provided for @wikiCloseConfirm.
  ///
  /// In zh, this message translates to:
  /// **'关闭前确认'**
  String get wikiCloseConfirm;

  /// No description provided for @wikiUnsavedChanges.
  ///
  /// In zh, this message translates to:
  /// **'该页面还有未保存的编辑，关闭将丢失这些修改。'**
  String get wikiUnsavedChanges;

  /// No description provided for @wikiDiscardClose.
  ///
  /// In zh, this message translates to:
  /// **'放弃修改并关闭'**
  String get wikiDiscardClose;

  /// No description provided for @wikiSaveClose.
  ///
  /// In zh, this message translates to:
  /// **'保存并关闭'**
  String get wikiSaveClose;

  /// No description provided for @wikiCloseTab.
  ///
  /// In zh, this message translates to:
  /// **'关闭标签页'**
  String get wikiCloseTab;

  /// No description provided for @wikiPageUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'页面不存在或已被删除'**
  String get wikiPageUnavailable;

  /// No description provided for @wikiEditTags.
  ///
  /// In zh, this message translates to:
  /// **'编辑标签'**
  String get wikiEditTags;

  /// No description provided for @wikiTagsHelp.
  ///
  /// In zh, this message translates to:
  /// **'用空格或逗号分隔多个标签，留空即清空'**
  String get wikiTagsHelp;

  /// No description provided for @wikiTagsExample.
  ///
  /// In zh, this message translates to:
  /// **'例如：工作 Rust 投资'**
  String get wikiTagsExample;

  /// No description provided for @wikiEnterUrl.
  ///
  /// In zh, this message translates to:
  /// **'请先粘贴一个链接'**
  String get wikiEnterUrl;

  /// No description provided for @wikiEnterContent.
  ///
  /// In zh, this message translates to:
  /// **'请先输入要保存的内容'**
  String get wikiEnterContent;

  /// No description provided for @wikiSavedTitle.
  ///
  /// In zh, this message translates to:
  /// **'已保存到知识库：《{title}》'**
  String wikiSavedTitle(String title);

  /// No description provided for @wikiSave.
  ///
  /// In zh, this message translates to:
  /// **'保存到知识库'**
  String get wikiSave;

  /// No description provided for @wikiUrlHelp.
  ///
  /// In zh, this message translates to:
  /// **'粘贴任意网址：x.com/twitter.com 推文或其他网页链接，也支持文章型推文'**
  String get wikiUrlHelp;

  /// No description provided for @wikiUrlHint.
  ///
  /// In zh, this message translates to:
  /// **'https://x.com/… 或 https://example.com/…'**
  String get wikiUrlHint;

  /// No description provided for @wikiFetching.
  ///
  /// In zh, this message translates to:
  /// **'抓取中…'**
  String get wikiFetching;

  /// No description provided for @wikiFetch.
  ///
  /// In zh, this message translates to:
  /// **'抓取'**
  String get wikiFetch;

  /// No description provided for @wikiPreviewHelp.
  ///
  /// In zh, this message translates to:
  /// **'抓取后会新开预览 tab，预览内容后点「保存到知识库」才会入库'**
  String get wikiPreviewHelp;

  /// No description provided for @wikiTitleHint.
  ///
  /// In zh, this message translates to:
  /// **'标题（可选，留空自动取前 60 字）'**
  String get wikiTitleHint;

  /// No description provided for @wikiTagsHint.
  ///
  /// In zh, this message translates to:
  /// **'标签（可选，空格或逗号分隔，例如：工作 投资）'**
  String get wikiTagsHint;

  /// No description provided for @wikiSourceLoading.
  ///
  /// In zh, this message translates to:
  /// **'正在读取来源…'**
  String get wikiSourceLoading;

  /// No description provided for @wikiSourceRetry.
  ///
  /// In zh, this message translates to:
  /// **'来源读取失败，点击重试'**
  String get wikiSourceRetry;

  /// No description provided for @wikiSourceUnavailable.
  ///
  /// In zh, this message translates to:
  /// **'来源页已不可用'**
  String get wikiSourceUnavailable;

  /// No description provided for @wikiBasedOn.
  ///
  /// In zh, this message translates to:
  /// **'基于：{title}'**
  String wikiBasedOn(String title);

  /// No description provided for @wikiSaved.
  ///
  /// In zh, this message translates to:
  /// **'已保存到知识库'**
  String get wikiSaved;

  /// No description provided for @wikiContent.
  ///
  /// In zh, this message translates to:
  /// **'内容'**
  String get wikiContent;

  /// No description provided for @wikiOriginal.
  ///
  /// In zh, this message translates to:
  /// **'原文'**
  String get wikiOriginal;

  /// No description provided for @wikiOutputs.
  ///
  /// In zh, this message translates to:
  /// **'产出'**
  String get wikiOutputs;

  /// No description provided for @wikiRelations.
  ///
  /// In zh, this message translates to:
  /// **'关联'**
  String get wikiRelations;

  /// No description provided for @wikiSources.
  ///
  /// In zh, this message translates to:
  /// **'来源'**
  String get wikiSources;

  /// No description provided for @wikiSourcesRevisions.
  ///
  /// In zh, this message translates to:
  /// **'来源与修订'**
  String get wikiSourcesRevisions;

  /// No description provided for @wikiPageChangedTitle.
  ///
  /// In zh, this message translates to:
  /// **'页面已被后台更新'**
  String get wikiPageChangedTitle;

  /// No description provided for @wikiPageChangedHelp.
  ///
  /// In zh, this message translates to:
  /// **'你编辑期间，该页面已被更新。\n「重新加载」放弃你的修改并查看最新内容；\n「强制覆盖」以你的修改覆盖更新（两个版本都留在修订历史里）。'**
  String get wikiPageChangedHelp;

  /// No description provided for @wikiReload.
  ///
  /// In zh, this message translates to:
  /// **'重新加载'**
  String get wikiReload;

  /// No description provided for @wikiOverwrite.
  ///
  /// In zh, this message translates to:
  /// **'强制覆盖'**
  String get wikiOverwrite;

  /// No description provided for @wikiCollectedOn.
  ///
  /// In zh, this message translates to:
  /// **'采集 {date}'**
  String wikiCollectedOn(String date);

  /// No description provided for @wikiCheckinHint.
  ///
  /// In zh, this message translates to:
  /// **'到点在主对话里问我，问法和按钮先由你确认'**
  String get wikiCheckinHint;

  /// No description provided for @wikiCheckinPaused.
  ///
  /// In zh, this message translates to:
  /// **'已暂停 · {time}'**
  String wikiCheckinPaused(String time);

  /// No description provided for @wikiCheckins.
  ///
  /// In zh, this message translates to:
  /// **'定期问候'**
  String get wikiCheckins;

  /// No description provided for @wikiCopyTitle.
  ///
  /// In zh, this message translates to:
  /// **'复制标题'**
  String get wikiCopyTitle;

  /// No description provided for @wikiTitleCopied.
  ///
  /// In zh, this message translates to:
  /// **'标题已复制'**
  String get wikiTitleCopied;

  /// No description provided for @wikiNoEventOrigins.
  ///
  /// In zh, this message translates to:
  /// **'源于 尚无事件溯源'**
  String get wikiNoEventOrigins;

  /// No description provided for @wikiEventOrigins.
  ///
  /// In zh, this message translates to:
  /// **'源于 {count} 条事件'**
  String wikiEventOrigins(int count);

  /// No description provided for @wikiAddTags.
  ///
  /// In zh, this message translates to:
  /// **'添加标签'**
  String get wikiAddTags;

  /// No description provided for @wikiContactRelations.
  ///
  /// In zh, this message translates to:
  /// **'联系人关系'**
  String get wikiContactRelations;

  /// No description provided for @wikiTagsUpdated.
  ///
  /// In zh, this message translates to:
  /// **'标签已更新'**
  String get wikiTagsUpdated;

  /// No description provided for @wikiImmersive.
  ///
  /// In zh, this message translates to:
  /// **'沉浸阅读'**
  String get wikiImmersive;

  /// No description provided for @wikiImmersiveHelp.
  ///
  /// In zh, this message translates to:
  /// **'沉浸阅读（Esc 退出）'**
  String get wikiImmersiveHelp;

  /// No description provided for @wikiExitImmersive.
  ///
  /// In zh, this message translates to:
  /// **'退出沉浸阅读（Esc）'**
  String get wikiExitImmersive;

  /// No description provided for @wikiReadingSettings.
  ///
  /// In zh, this message translates to:
  /// **'阅读设置'**
  String get wikiReadingSettings;

  /// No description provided for @wikiReadingSettingsHelp.
  ///
  /// In zh, this message translates to:
  /// **'正文阅读设置（字体 / 字号 / 行距）'**
  String get wikiReadingSettingsHelp;

  /// No description provided for @wikiEditContent.
  ///
  /// In zh, this message translates to:
  /// **'编辑正文'**
  String get wikiEditContent;

  /// No description provided for @wikiDeletePage.
  ///
  /// In zh, this message translates to:
  /// **'删除页面'**
  String get wikiDeletePage;

  /// No description provided for @wikiDeleteQuestion.
  ///
  /// In zh, this message translates to:
  /// **'删除页面？'**
  String get wikiDeleteQuestion;

  /// No description provided for @wikiDeleteHelp.
  ///
  /// In zh, this message translates to:
  /// **'「{title}」将被删除，修订历史随之清除，且不可恢复。其他设备同步后同样删除；它们与此页冲突的离线编辑会保留为冲突候选。'**
  String wikiDeleteHelp(String title);

  /// No description provided for @errorWikiPageChanged.
  ///
  /// In zh, this message translates to:
  /// **'页面已更新，请重新加载或确认覆盖。'**
  String get errorWikiPageChanged;

  /// No description provided for @errorWikiMaterialReadonly.
  ///
  /// In zh, this message translates to:
  /// **'素材原文只读，无法编辑正文。'**
  String get errorWikiMaterialReadonly;

  /// No description provided for @errorWikiContentRequired.
  ///
  /// In zh, this message translates to:
  /// **'正文不能为空。'**
  String get errorWikiContentRequired;

  /// No description provided for @errorWikiContentTooLong.
  ///
  /// In zh, this message translates to:
  /// **'正文不能超过 65,536 个字符。'**
  String get errorWikiContentTooLong;

  /// No description provided for @wikiDiscussPage.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 讨论此页'**
  String get wikiDiscussPage;

  /// No description provided for @wikiAskAi.
  ///
  /// In zh, this message translates to:
  /// **'问 AI'**
  String get wikiAskAi;

  /// No description provided for @wikiProjectPathCopied.
  ///
  /// In zh, this message translates to:
  /// **'项目路径已复制'**
  String get wikiProjectPathCopied;

  /// No description provided for @wikiEditProjectPath.
  ///
  /// In zh, this message translates to:
  /// **'修改项目路径'**
  String get wikiEditProjectPath;

  /// No description provided for @wikiChooseProjectDirectory.
  ///
  /// In zh, this message translates to:
  /// **'选择项目目录'**
  String get wikiChooseProjectDirectory;

  /// No description provided for @wikiOldPath.
  ///
  /// In zh, this message translates to:
  /// **'旧路径'**
  String get wikiOldPath;

  /// No description provided for @wikiNewPath.
  ///
  /// In zh, this message translates to:
  /// **'新路径'**
  String get wikiNewPath;

  /// No description provided for @wikiProjectPathUpdated.
  ///
  /// In zh, this message translates to:
  /// **'项目路径已更新'**
  String get wikiProjectPathUpdated;

  /// No description provided for @wikiSourceLinkCopied.
  ///
  /// In zh, this message translates to:
  /// **'来源链接已复制'**
  String get wikiSourceLinkCopied;

  /// No description provided for @wikiRelatedPages.
  ///
  /// In zh, this message translates to:
  /// **'相关页面'**
  String get wikiRelatedPages;

  /// No description provided for @wikiAddRelatedPage.
  ///
  /// In zh, this message translates to:
  /// **'添加相关页面'**
  String get wikiAddRelatedPage;

  /// No description provided for @wikiNoRelatedCandidates.
  ///
  /// In zh, this message translates to:
  /// **'暂时没有可选的页面'**
  String get wikiNoRelatedCandidates;

  /// No description provided for @wikiTweet.
  ///
  /// In zh, this message translates to:
  /// **'推文'**
  String get wikiTweet;

  /// No description provided for @wikiWebpage.
  ///
  /// In zh, this message translates to:
  /// **'网页'**
  String get wikiWebpage;

  /// No description provided for @wikiImportTagsHint.
  ///
  /// In zh, this message translates to:
  /// **'标签（可选，空格分隔，例如：方法 投资）'**
  String get wikiImportTagsHint;

  /// No description provided for @wikiHome.
  ///
  /// In zh, this message translates to:
  /// **'首页'**
  String get wikiHome;

  /// No description provided for @wikiPostTab.
  ///
  /// In zh, this message translates to:
  /// **'推文 {id}'**
  String wikiPostTab(String id);

  /// No description provided for @wikiRelationLabel.
  ///
  /// In zh, this message translates to:
  /// **'{relation, select, related{相关} mentions{提及} mentionedBy{被提及} part_of{从属} contains{包含本页} same_topic{同主题} supports{佐证} supportedBy{被佐证} contradicts{相悖} contradictedBy{被相悖} other{{relation}}}'**
  String wikiRelationLabel(String relation);

  /// No description provided for @tweetDiscuss.
  ///
  /// In zh, this message translates to:
  /// **'与 AI 讨论这篇推文'**
  String get tweetDiscuss;

  /// No description provided for @tweetDiscussHelp.
  ///
  /// In zh, this message translates to:
  /// **'保存前可先和 AI 过一遍，点「保存到知识库」才入库'**
  String get tweetDiscussHelp;

  /// No description provided for @tweetUnknownAuthor.
  ///
  /// In zh, this message translates to:
  /// **'未知作者'**
  String get tweetUnknownAuthor;

  /// No description provided for @tweetByAuthor.
  ///
  /// In zh, this message translates to:
  /// **'{author} 的推文'**
  String tweetByAuthor(String author);

  /// No description provided for @tweetSaved.
  ///
  /// In zh, this message translates to:
  /// **'已保存'**
  String get tweetSaved;

  /// No description provided for @tweetOriginal.
  ///
  /// In zh, this message translates to:
  /// **'抓取内容（原文）'**
  String get tweetOriginal;

  /// No description provided for @tweetLinkCopied.
  ///
  /// In zh, this message translates to:
  /// **'推文链接已复制'**
  String get tweetLinkCopied;

  /// No description provided for @tweetFormatting.
  ///
  /// In zh, this message translates to:
  /// **'AI 整理中…'**
  String get tweetFormatting;

  /// No description provided for @tweetFormat.
  ///
  /// In zh, this message translates to:
  /// **'AI 整理为 Markdown'**
  String get tweetFormat;

  /// No description provided for @tweetFormatAgain.
  ///
  /// In zh, this message translates to:
  /// **'重新整理'**
  String get tweetFormatAgain;

  /// No description provided for @tweetDiscardFormat.
  ///
  /// In zh, this message translates to:
  /// **'放弃整理'**
  String get tweetDiscardFormat;

  /// No description provided for @tweetSeparateDraftHelp.
  ///
  /// In zh, this message translates to:
  /// **'保存原文，整理稿另存为产物'**
  String get tweetSeparateDraftHelp;

  /// No description provided for @tweetFormatPreview.
  ///
  /// In zh, this message translates to:
  /// **'AI 整理结果（Markdown 预览）'**
  String get tweetFormatPreview;

  /// No description provided for @tweetChatHint.
  ///
  /// In zh, this message translates to:
  /// **'就这篇推文问问 AI…（回车发送，Shift+回车换行）'**
  String get tweetChatHint;

  /// No description provided for @artifactWorks.
  ///
  /// In zh, this message translates to:
  /// **'衍生作品'**
  String get artifactWorks;

  /// No description provided for @artifactRefresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新衍生作品'**
  String get artifactRefresh;

  /// No description provided for @artifactEmpty.
  ///
  /// In zh, this message translates to:
  /// **'还没有衍生作品。在本页对话里让 AI 生成摘要、观点、灵感或脚本，保存后会在这里列为版本。'**
  String get artifactEmpty;

  /// No description provided for @artifactNotAdopted.
  ///
  /// In zh, this message translates to:
  /// **'未采用'**
  String get artifactNotAdopted;

  /// No description provided for @artifactMetadata.
  ///
  /// In zh, this message translates to:
  /// **'模型：{model} · 流程版本：{strategy}'**
  String artifactMetadata(String model, String strategy);

  /// No description provided for @artifactUnknownModel.
  ///
  /// In zh, this message translates to:
  /// **'未知（未记录）'**
  String get artifactUnknownModel;

  /// No description provided for @artifactUnknownStrategy.
  ///
  /// In zh, this message translates to:
  /// **'未知（旧产物）'**
  String get artifactUnknownStrategy;

  /// No description provided for @artifactInstruction.
  ///
  /// In zh, this message translates to:
  /// **'生成指令'**
  String get artifactInstruction;

  /// No description provided for @artifactNoInstruction.
  ///
  /// In zh, this message translates to:
  /// **'此版本未记录生成指令'**
  String get artifactNoInstruction;

  /// No description provided for @artifactAdoptedRevision.
  ///
  /// In zh, this message translates to:
  /// **'查看实际采用的修订'**
  String get artifactAdoptedRevision;

  /// No description provided for @artifactNewerRevisionHelp.
  ///
  /// In zh, this message translates to:
  /// **'当前正文已有新修订；采用记录仍指向之前确认的修订。'**
  String get artifactNewerRevisionHelp;

  /// No description provided for @artifactOpenPage.
  ///
  /// In zh, this message translates to:
  /// **'打开知识页'**
  String get artifactOpenPage;

  /// No description provided for @artifactCopyBody.
  ///
  /// In zh, this message translates to:
  /// **'复制全文'**
  String get artifactCopyBody;

  /// No description provided for @artifactBodyCopied.
  ///
  /// In zh, this message translates to:
  /// **'已复制完整正文'**
  String get artifactBodyCopied;

  /// No description provided for @artifactCompareSelect.
  ///
  /// In zh, this message translates to:
  /// **'加入比较'**
  String get artifactCompareSelect;

  /// No description provided for @artifactUnadopt.
  ///
  /// In zh, this message translates to:
  /// **'取消采用'**
  String get artifactUnadopt;

  /// No description provided for @artifactAdopt.
  ///
  /// In zh, this message translates to:
  /// **'采用这个版本'**
  String get artifactAdopt;

  /// No description provided for @artifactMoreVersions.
  ///
  /// In zh, this message translates to:
  /// **'加载更多版本'**
  String get artifactMoreVersions;

  /// No description provided for @artifactCompare.
  ///
  /// In zh, this message translates to:
  /// **'版本比较'**
  String get artifactCompare;

  /// No description provided for @artifactType.
  ///
  /// In zh, this message translates to:
  /// **'{type, select, summary{摘要} opinion{观点} idea{灵感} script{脚本} ai_summary{AI 总结} other{{type}}}'**
  String artifactType(String type);

  /// No description provided for @workItemUpdateStatus.
  ///
  /// In zh, this message translates to:
  /// **'更新状态'**
  String get workItemUpdateStatus;

  /// No description provided for @workItemUpdatePriority.
  ///
  /// In zh, this message translates to:
  /// **'更新优先级'**
  String get workItemUpdatePriority;

  /// No description provided for @workItemPriority.
  ///
  /// In zh, this message translates to:
  /// **'{priority, select, high{高优先级} low{低优先级} other{普通优先级}}'**
  String workItemPriority(String priority);

  /// No description provided for @workItemChooseDue.
  ///
  /// In zh, this message translates to:
  /// **'选择截止日期'**
  String get workItemChooseDue;

  /// No description provided for @workItemSetDue.
  ///
  /// In zh, this message translates to:
  /// **'设置截止日期'**
  String get workItemSetDue;

  /// No description provided for @entityMerged.
  ///
  /// In zh, this message translates to:
  /// **'此实体已合并'**
  String get entityMerged;

  /// No description provided for @entityMergedInto.
  ///
  /// In zh, this message translates to:
  /// **'此实体已合并到 {target}'**
  String entityMergedInto(String target);

  /// No description provided for @entityViewTarget.
  ///
  /// In zh, this message translates to:
  /// **'查看目标'**
  String get entityViewTarget;

  /// No description provided for @entityUndoMerge.
  ///
  /// In zh, this message translates to:
  /// **'撤销合并'**
  String get entityUndoMerge;

  /// No description provided for @entityMerge.
  ///
  /// In zh, this message translates to:
  /// **'合并实体'**
  String get entityMerge;

  /// No description provided for @entityNoMergeCandidates.
  ///
  /// In zh, this message translates to:
  /// **'没有可合并的同类型实体'**
  String get entityNoMergeCandidates;

  /// No description provided for @entityChooseTarget.
  ///
  /// In zh, this message translates to:
  /// **'选择目标实体'**
  String get entityChooseTarget;

  /// No description provided for @entityMergeInto.
  ///
  /// In zh, this message translates to:
  /// **'合并到'**
  String get entityMergeInto;

  /// No description provided for @entityContinue.
  ///
  /// In zh, this message translates to:
  /// **'继续'**
  String get entityContinue;

  /// No description provided for @entityMergeQuestion.
  ///
  /// In zh, this message translates to:
  /// **'合并到“{title}”？'**
  String entityMergeQuestion(String title);

  /// No description provided for @entityMergeHelp.
  ///
  /// In zh, this message translates to:
  /// **'事实、别名和关系会迁移到目标实体；重复内容会合并。原始事件不会改变。只要迁移后的内容没有被修改，就可以从旧实体页撤销。'**
  String get entityMergeHelp;

  /// No description provided for @entityConfirmMerge.
  ///
  /// In zh, this message translates to:
  /// **'确认合并'**
  String get entityConfirmMerge;

  /// No description provided for @entityMergeSaved.
  ///
  /// In zh, this message translates to:
  /// **'已合并到“{title}”，原始事件仍保留'**
  String entityMergeSaved(String title);

  /// No description provided for @entityUndoMergeQuestion.
  ///
  /// In zh, this message translates to:
  /// **'撤销这次合并？'**
  String get entityUndoMergeQuestion;

  /// No description provided for @entityUndoMergeHelp.
  ///
  /// In zh, this message translates to:
  /// **'只恢复本次合并迁移的事实、别名和关系。若这些内容合并后已被修改，系统会拒绝操作以保护新数据。原始事件不会改变。'**
  String get entityUndoMergeHelp;

  /// No description provided for @entityMergeUndone.
  ///
  /// In zh, this message translates to:
  /// **'合并已撤销，实体内容已恢复'**
  String get entityMergeUndone;

  /// No description provided for @entityRelatedTodos.
  ///
  /// In zh, this message translates to:
  /// **'关联待办'**
  String get entityRelatedTodos;

  /// No description provided for @entityOpenWorkItem.
  ///
  /// In zh, this message translates to:
  /// **'打开工作项'**
  String get entityOpenWorkItem;

  /// No description provided for @entityFacts.
  ///
  /// In zh, this message translates to:
  /// **'结构化事实'**
  String get entityFacts;

  /// No description provided for @entityFactConflictHelp.
  ///
  /// In zh, this message translates to:
  /// **'发现可能冲突的事实。历史来源均已保留，请查看来源后移除错误的派生事实。'**
  String get entityFactConflictHelp;

  /// No description provided for @entityFactSource.
  ///
  /// In zh, this message translates to:
  /// **'{date} · 置信度 {confidence}/5 · 查看来源 {source}'**
  String entityFactSource(String date, String confidence, String source);

  /// No description provided for @entityRemoveFact.
  ///
  /// In zh, this message translates to:
  /// **'纠正：移除此事实'**
  String get entityRemoveFact;

  /// No description provided for @entityRemoveFactQuestion.
  ///
  /// In zh, this message translates to:
  /// **'移除这条事实？'**
  String get entityRemoveFactQuestion;

  /// No description provided for @entityRemoveFactHelp.
  ///
  /// In zh, this message translates to:
  /// **'只会移除知识页中的派生事实，不会删除原始事件。'**
  String get entityRemoveFactHelp;

  /// No description provided for @entityRemove.
  ///
  /// In zh, this message translates to:
  /// **'移除'**
  String get entityRemove;

  /// No description provided for @entityFactRemoved.
  ///
  /// In zh, this message translates to:
  /// **'事实已移除，原始事件仍保留'**
  String get entityFactRemoved;

  /// No description provided for @entitySourceMissing.
  ///
  /// In zh, this message translates to:
  /// **'来源事件不存在'**
  String get entitySourceMissing;

  /// No description provided for @entitySourceEvent.
  ///
  /// In zh, this message translates to:
  /// **'来源事件'**
  String get entitySourceEvent;

  /// No description provided for @entityAiSummary.
  ///
  /// In zh, this message translates to:
  /// **'AI 摘要'**
  String get entityAiSummary;

  /// No description provided for @entityEmotionAnnotations.
  ///
  /// In zh, this message translates to:
  /// **'情绪标注（文本，无数值）'**
  String get entityEmotionAnnotations;

  /// No description provided for @entityEmotionAboutEvent.
  ///
  /// In zh, this message translates to:
  /// **'对这件事：{feeling}'**
  String entityEmotionAboutEvent(String feeling);

  /// No description provided for @entityEmotionAboutTarget.
  ///
  /// In zh, this message translates to:
  /// **'对 {target}：{feeling}'**
  String entityEmotionAboutTarget(String target, String feeling);

  /// No description provided for @entityAiGuidance.
  ///
  /// In zh, this message translates to:
  /// **'AI 指导（派生层）'**
  String get entityAiGuidance;

  /// No description provided for @entityRelatedReference.
  ///
  /// In zh, this message translates to:
  /// **'相关：{slug} — {note}'**
  String entityRelatedReference(String slug, String note);

  /// No description provided for @entityAnalysisMetadata.
  ///
  /// In zh, this message translates to:
  /// **'记录于 {date} · 分析状态 {status}'**
  String entityAnalysisMetadata(String date, String status);

  /// No description provided for @entityAnalysisStatus.
  ///
  /// In zh, this message translates to:
  /// **'{status, select, pending{待处理} running{处理中} succeeded{已完成} failed{失败} skipped{已跳过} other{{status}}}'**
  String entityAnalysisStatus(String status);

  /// No description provided for @entityGuidance.
  ///
  /// In zh, this message translates to:
  /// **'相关指导'**
  String get entityGuidance;

  /// No description provided for @entityViewGuidance.
  ///
  /// In zh, this message translates to:
  /// **'查看指导'**
  String get entityViewGuidance;

  /// No description provided for @entityGuidanceDetails.
  ///
  /// In zh, this message translates to:
  /// **'{date} - 点击查看要点'**
  String entityGuidanceDetails(String date);

  /// No description provided for @entityGuidanceHelpful.
  ///
  /// In zh, this message translates to:
  /// **'有用（再点一次清除）'**
  String get entityGuidanceHelpful;

  /// No description provided for @entityGuidanceNotRelevant.
  ///
  /// In zh, this message translates to:
  /// **'不相关（再点一次清除）'**
  String get entityGuidanceNotRelevant;

  /// No description provided for @entityRecordedOn.
  ///
  /// In zh, this message translates to:
  /// **'记录于 {date}'**
  String entityRecordedOn(String date);

  /// No description provided for @entityReusableLessons.
  ///
  /// In zh, this message translates to:
  /// **'可复用经验'**
  String get entityReusableLessons;

  /// No description provided for @entityRefinedTodos.
  ///
  /// In zh, this message translates to:
  /// **'待办精炼'**
  String get entityRefinedTodos;

  /// No description provided for @runLanguagePreserveSource.
  ///
  /// In zh, this message translates to:
  /// **'产出策略：保留来源原文语言（仅排版整理）'**
  String get runLanguagePreserveSource;

  /// No description provided for @l6ReviewHistoryIsNotAPersonaldee4d.
  ///
  /// In zh, this message translates to:
  /// **'审阅历史未归属规则'**
  String get l6ReviewHistoryIsNotAPersonaldee4d;

  /// No description provided for @l6All778fc.
  ///
  /// In zh, this message translates to:
  /// **'全部'**
  String get l6All778fc;

  /// No description provided for @l6RunLoga8ce4.
  ///
  /// In zh, this message translates to:
  /// **'运行日志'**
  String get l6RunLoga8ce4;

  /// No description provided for @l6NoRunsYet022d0.
  ///
  /// In zh, this message translates to:
  /// **'还没有运行记录'**
  String get l6NoRunsYet022d0;

  /// No description provided for @l6ProtectedOnlyEvidenceCanBeAddedba2bf.
  ///
  /// In zh, this message translates to:
  /// **'受保护（仅累加证据）'**
  String get l6ProtectedOnlyEvidenceCanBeAddedba2bf;

  /// No description provided for @l6Savinga032e.
  ///
  /// In zh, this message translates to:
  /// **'保存中'**
  String get l6Savinga032e;

  /// No description provided for @l6ScanInvitationQrCode4882e.
  ///
  /// In zh, this message translates to:
  /// **'扫描邀请二维码'**
  String get l6ScanInvitationQrCode4882e;

  /// No description provided for @l6CameraAccessIsUnavailableCopyAnd95be5.
  ///
  /// In zh, this message translates to:
  /// **'无法访问相机，可用复制/粘贴邀请码代替。'**
  String get l6CameraAccessIsUnavailableCopyAnd95be5;

  /// No description provided for @l6DeviceKeysAreHeldInSystem9ad4e.
  ///
  /// In zh, this message translates to:
  /// **'本机密钥由系统安全存储托管（仅限本设备，不进云备份/云同步）。'**
  String get l6DeviceKeysAreHeldInSystem9ad4e;

  /// No description provided for @l6DeviceKeyRecordsAreMissingRecover91b1e.
  ///
  /// In zh, this message translates to:
  /// **'缺少本机密钥记录。请通过原设备恢复；不会生成替代密钥。'**
  String get l6DeviceKeyRecordsAreMissingRecover91b1e;

  /// No description provided for @l6DeviceKeyRecordsAreDamagedUnlocking1284a.
  ///
  /// In zh, this message translates to:
  /// **'本机密钥记录损坏，已停止解锁且不会覆盖原记录。'**
  String get l6DeviceKeyRecordsAreDamagedUnlocking1284a;

  /// No description provided for @l6SystemSecureStorageIsUnavailableOr5cc06.
  ///
  /// In zh, this message translates to:
  /// **'系统安全存储不可用或已锁定，请解锁系统密钥库后刷新。'**
  String get l6SystemSecureStorageIsUnavailableOr5cc06;

  /// No description provided for @l6CheckingDeviceKeyStorage3d187.
  ///
  /// In zh, this message translates to:
  /// **'正在检查本机密钥存储…'**
  String get l6CheckingDeviceKeyStorage3d187;

  /// No description provided for @l6AuthorizedDevicesb4736.
  ///
  /// In zh, this message translates to:
  /// **'已授权设备'**
  String get l6AuthorizedDevicesb4736;

  /// No description provided for @l6RefreshDevices7b112.
  ///
  /// In zh, this message translates to:
  /// **'刷新设备列表'**
  String get l6RefreshDevices7b112;

  /// No description provided for @l6OwnerDevicede929.
  ///
  /// In zh, this message translates to:
  /// **'主设备'**
  String get l6OwnerDevicede929;

  /// No description provided for @l6RunsAiAnalysis6697e.
  ///
  /// In zh, this message translates to:
  /// **'负责 AI 分析'**
  String get l6RunsAiAnalysis6697e;

  /// No description provided for @l6ThisDevice8b4c1.
  ///
  /// In zh, this message translates to:
  /// **'本机'**
  String get l6ThisDevice8b4c1;

  /// No description provided for @l6Revoke9fcef.
  ///
  /// In zh, this message translates to:
  /// **'撤销'**
  String get l6Revoke9fcef;

  /// No description provided for @l6TransferAiAnalysisResponsibilityBeforeRevoking455d7.
  ///
  /// In zh, this message translates to:
  /// **'需先移交 AI 分析职责后再撤销'**
  String get l6TransferAiAnalysisResponsibilityBeforeRevoking455d7;

  /// No description provided for @l6RevokeThisDeviceSSyncAccess2a8d0.
  ///
  /// In zh, this message translates to:
  /// **'撤销设备同步权限？'**
  String get l6RevokeThisDeviceSSyncAccess2a8d0;

  /// No description provided for @l6KnowledgeSyncConflicts67265.
  ///
  /// In zh, this message translates to:
  /// **'知识库同步冲突'**
  String get l6KnowledgeSyncConflicts67265;

  /// No description provided for @l6KnowledgeChangesFromAnotherDeviceConflict5a183.
  ///
  /// In zh, this message translates to:
  /// **'来自其他设备的知识页改动与本端互相冲突。选择「保留本端」或「采纳对端」，不做字段拼接。'**
  String get l6KnowledgeChangesFromAnotherDeviceConflict5a183;

  /// No description provided for @l6NoKnowledgeSyncConflictsNeedReview2246e.
  ///
  /// In zh, this message translates to:
  /// **'没有待审阅的知识库同步冲突。'**
  String get l6NoKnowledgeSyncConflictsNeedReview2246e;

  /// No description provided for @l6TheIncomingVersionDeletesThisPagec27d8.
  ///
  /// In zh, this message translates to:
  /// **'对端已删除本页，而本端之后仍有改动要送达'**
  String get l6TheIncomingVersionDeletesThisPagec27d8;

  /// No description provided for @l6BothDevicesRenamedThisPageDifferently200f1.
  ///
  /// In zh, this message translates to:
  /// **'两端把本页改名到了不同标题'**
  String get l6BothDevicesRenamedThisPageDifferently200f1;

  /// No description provided for @l6TheIncomingPathConflictsWithAnother49464.
  ///
  /// In zh, this message translates to:
  /// **'对端的新路径与本端另一页重名'**
  String get l6TheIncomingPathConflictsWithAnother49464;

  /// No description provided for @l6TheIncomingEditIsBasedOnf0ddf.
  ///
  /// In zh, this message translates to:
  /// **'对端的修改基于旧版本，本页已先变过'**
  String get l6TheIncomingEditIsBasedOnf0ddf;

  /// No description provided for @l6LocalVersionDeleted25fe7.
  ///
  /// In zh, this message translates to:
  /// **'本端（已删除 State）'**
  String get l6LocalVersionDeleted25fe7;

  /// No description provided for @l6ThisPageDoesNotExistLocallya6853.
  ///
  /// In zh, this message translates to:
  /// **'（本端没有该页）'**
  String get l6ThisPageDoesNotExistLocallya6853;

  /// No description provided for @l6Untitledd8f9b.
  ///
  /// In zh, this message translates to:
  /// **'（无标题）'**
  String get l6Untitledd8f9b;

  /// No description provided for @l6OrganizeTopicscabf2.
  ///
  /// In zh, this message translates to:
  /// **'整理主题结构'**
  String get l6OrganizeTopicscabf2;

  /// No description provided for @l6MergeSimilarTopicsOrSplitA6542f.
  ///
  /// In zh, this message translates to:
  /// **'合并相近主题或拆分过大的主题，预览确认后生效'**
  String get l6MergeSimilarTopicsOrSplitA6542f;

  /// No description provided for @l6OldTopicsAreArchivedAfterConfirmationf5e56.
  ///
  /// In zh, this message translates to:
  /// **'确认后旧主题归档保留，来源和历史仍可追溯。新主题作为参考知识，不会升级为个人规则。'**
  String get l6OldTopicsAreArchivedAfterConfirmationf5e56;

  /// No description provided for @l6SearchTopicsToMergef60aa.
  ///
  /// In zh, this message translates to:
  /// **'搜索待合并的主题…'**
  String get l6SearchTopicsToMergef60aa;

  /// No description provided for @l6FindTopicsa638d.
  ///
  /// In zh, this message translates to:
  /// **'查找主题'**
  String get l6FindTopicsa638d;

  /// No description provided for @l6PreviewMerge6d8f5.
  ///
  /// In zh, this message translates to:
  /// **'预览合并'**
  String get l6PreviewMerge6d8f5;

  /// No description provided for @l6PreviewSplit6f910.
  ///
  /// In zh, this message translates to:
  /// **'预览拆分本页'**
  String get l6PreviewSplit6f910;

  /// No description provided for @l6AwaitingConfirmation27b58.
  ///
  /// In zh, this message translates to:
  /// **'待确认'**
  String get l6AwaitingConfirmation27b58;

  /// No description provided for @l6Revoked61063.
  ///
  /// In zh, this message translates to:
  /// **'已撤销'**
  String get l6Revoked61063;

  /// No description provided for @l6UndoThisBatch7aa0a.
  ///
  /// In zh, this message translates to:
  /// **'撤销整批整理'**
  String get l6UndoThisBatch7aa0a;

  /// No description provided for @l6ConfirmAndArchiveOldTopics37635.
  ///
  /// In zh, this message translates to:
  /// **'确认整理并归档旧主题'**
  String get l6ConfirmAndArchiveOldTopics37635;

  /// No description provided for @l6RejectPlan8b441.
  ///
  /// In zh, this message translates to:
  /// **'拒绝方案'**
  String get l6RejectPlan8b441;

  /// No description provided for @l6LoadEarlierPlans65e31.
  ///
  /// In zh, this message translates to:
  /// **'加载更早整理方案'**
  String get l6LoadEarlierPlans65e31;

  /// No description provided for @l6RestartTheAppAfterSwitchingToea99d.
  ///
  /// In zh, this message translates to:
  /// **'切换后重启应用即可看到全部同步数据。'**
  String get l6RestartTheAppAfterSwitchingToea99d;

  /// No description provided for @l6EnterDatasete3665.
  ///
  /// In zh, this message translates to:
  /// **'进入数据集'**
  String get l6EnterDatasete3665;

  /// No description provided for @l6CouldNotLoadSyncStatusSyncb8a20.
  ///
  /// In zh, this message translates to:
  /// **'同步状态读取失败\n（同步可能尚未启用）'**
  String get l6CouldNotLoadSyncStatusSyncb8a20;

  /// No description provided for @l6Processorb4366.
  ///
  /// In zh, this message translates to:
  /// **'处理器'**
  String get l6Processorb4366;

  /// No description provided for @l6NoSyncYet8ce44.
  ///
  /// In zh, this message translates to:
  /// **'尚未同步过'**
  String get l6NoSyncYet8ce44;

  /// No description provided for @l6HeartbeatOnStandbyIdleda794.
  ///
  /// In zh, this message translates to:
  /// **'心跳待命（空闲）'**
  String get l6HeartbeatOnStandbyIdleda794;

  /// No description provided for @l6Status62e95.
  ///
  /// In zh, this message translates to:
  /// **'状态'**
  String get l6Status62e95;

  /// No description provided for @l6LoadLinkableKnowledgea71cb.
  ///
  /// In zh, this message translates to:
  /// **'读取可关联知识'**
  String get l6LoadLinkableKnowledgea71cb;

  /// No description provided for @l6LoadResolutionRevision2bf89.
  ///
  /// In zh, this message translates to:
  /// **'读取解决修订'**
  String get l6LoadResolutionRevision2bf89;

  /// No description provided for @l6ResolutionSavedf80e5.
  ///
  /// In zh, this message translates to:
  /// **'已保存解决记录'**
  String get l6ResolutionSavedf80e5;

  /// No description provided for @l6LinkedResolutionsb26bd.
  ///
  /// In zh, this message translates to:
  /// **'关联解决记录'**
  String get l6LinkedResolutionsb26bd;

  /// No description provided for @l6LinkTheIssueToAKnowledge08e6c.
  ///
  /// In zh, this message translates to:
  /// **'将问题关联到覆盖全部依据的知识修订'**
  String get l6LinkTheIssueToAKnowledge08e6c;

  /// No description provided for @l6NoKnowledgePageCoversTheseSources10eed.
  ///
  /// In zh, this message translates to:
  /// **'暂无覆盖这些来源的知识页。请先在相关主题中整理分歧及适用条件，再回到此处关联。'**
  String get l6NoKnowledgePageCoversTheseSources10eed;

  /// No description provided for @l6KnowledgePageThatResolvesTheIssueb3d20.
  ///
  /// In zh, this message translates to:
  /// **'解决问题的知识页'**
  String get l6KnowledgePageThatResolvesTheIssueb3d20;

  /// No description provided for @l6ReviewRevisionContent73b70.
  ///
  /// In zh, this message translates to:
  /// **'核对修订正文'**
  String get l6ReviewRevisionContent73b70;

  /// No description provided for @l6HowThisRevisionResolvesTheDisagreemented5c3.
  ///
  /// In zh, this message translates to:
  /// **'这次修订如何处理分歧'**
  String get l6HowThisRevisionResolvesTheDisagreemented5c3;

  /// No description provided for @l6ConfirmLinkedResolution5427f.
  ///
  /// In zh, this message translates to:
  /// **'确认关联解决'**
  String get l6ConfirmLinkedResolution5427f;

  /// No description provided for @l6RefreshLinkableKnowledge5a5b4.
  ///
  /// In zh, this message translates to:
  /// **'刷新可关联知识'**
  String get l6RefreshLinkableKnowledge5a5b4;

  /// No description provided for @l6LoadAvailableSourcesefee2.
  ///
  /// In zh, this message translates to:
  /// **'读取可用来源'**
  String get l6LoadAvailableSourcesefee2;

  /// No description provided for @l6RepairSourceEvidence7c494.
  ///
  /// In zh, this message translates to:
  /// **'修复来源依据'**
  String get l6RepairSourceEvidence7c494;

  /// No description provided for @l6KeepValidSourcesOrChooseReplacementse7069.
  ///
  /// In zh, this message translates to:
  /// **'保留有效来源，或选择替代原料；审阅后更新知识'**
  String get l6KeepValidSourcesOrChooseReplacementse7069;

  /// No description provided for @l6RejectedOrArchivedSourcesCannotSupport758ff.
  ///
  /// In zh, this message translates to:
  /// **'不认可或已归档的来源不能继续支持新结论。下方默认保留有效来源，原知识在确认修订前保留。'**
  String get l6RejectedOrArchivedSourcesCannotSupport758ff;

  /// No description provided for @l6FindReplacementSources05a6a.
  ///
  /// In zh, this message translates to:
  /// **'查找替代原料'**
  String get l6FindReplacementSources05a6a;

  /// No description provided for @l6TheFirst50SourcesAreShown7e891.
  ///
  /// In zh, this message translates to:
  /// **'列表显示前 50 项，可输入标题定位其他原料。已选来源仍保留。'**
  String get l6TheFirst50SourcesAreShown7e891;

  /// No description provided for @l6UnavailableExcludedFromEvidenceForThe84882.
  ///
  /// In zh, this message translates to:
  /// **'不可用，将从新修订依据中排除'**
  String get l6UnavailableExcludedFromEvidenceForThe84882;

  /// No description provided for @l6SourcesForThisKnowledgec088b.
  ///
  /// In zh, this message translates to:
  /// **'当前知识的来源'**
  String get l6SourcesForThisKnowledgec088b;

  /// No description provided for @l6AvailableAsAReplacementSourceda692.
  ///
  /// In zh, this message translates to:
  /// **'可作为替代来源'**
  String get l6AvailableAsAReplacementSourceda692;

  /// No description provided for @l6ThereIsNotEnoughEvidenceSelect1b56c.
  ///
  /// In zh, this message translates to:
  /// **'剩余依据不足。请选择或先导入有效原料；当前知识仍待补证据。'**
  String get l6ThereIsNotEnoughEvidenceSelect1b56c;

  /// No description provided for @l6ChooseUpTo8SourcesNarrow5655d.
  ///
  /// In zh, this message translates to:
  /// **'最多选择 8 份原料，请缩小范围。'**
  String get l6ChooseUpTo8SourcesNarrow5655d;

  /// No description provided for @l6PrepareRepairProposale29a1.
  ///
  /// In zh, this message translates to:
  /// **'准备修复建议'**
  String get l6PrepareRepairProposale29a1;

  /// No description provided for @l6RefreshSources59684.
  ///
  /// In zh, this message translates to:
  /// **'刷新来源'**
  String get l6RefreshSources59684;

  /// No description provided for @l6ReloadSources59500.
  ///
  /// In zh, this message translates to:
  /// **'重新读取来源'**
  String get l6ReloadSources59500;

  /// No description provided for @l6LoadReviewHistory6b7da.
  ///
  /// In zh, this message translates to:
  /// **'读取处理历史'**
  String get l6LoadReviewHistory6b7da;

  /// No description provided for @l6KnowledgeHistory37e16.
  ///
  /// In zh, this message translates to:
  /// **'知识演进'**
  String get l6KnowledgeHistory37e16;

  /// No description provided for @l6HistoryOfProposalsAndRevisionsAccepted1535c.
  ///
  /// In zh, this message translates to:
  /// **'这页内容被建议与修订的历史（采纳 / 拒绝 / 关联解决）'**
  String get l6HistoryOfProposalsAndRevisionsAccepted1535c;

  /// No description provided for @l6ThisPageHasNoRevisionsYet1b848.
  ///
  /// In zh, this message translates to:
  /// **'这页还没有过修订记录。'**
  String get l6ThisPageHasNoRevisionsYet1b848;

  /// No description provided for @l6ResolutionNotesca3c9.
  ///
  /// In zh, this message translates to:
  /// **'处理说明'**
  String get l6ResolutionNotesca3c9;

  /// No description provided for @l6AcceptedChanges114f5.
  ///
  /// In zh, this message translates to:
  /// **'采纳的差异段'**
  String get l6AcceptedChanges114f5;

  /// No description provided for @l6SuggestedConditions5bf71.
  ///
  /// In zh, this message translates to:
  /// **'建议适用条件'**
  String get l6SuggestedConditions5bf71;

  /// No description provided for @l6SavedConditions0ae9a.
  ///
  /// In zh, this message translates to:
  /// **'保存的适用条件'**
  String get l6SavedConditions0ae9a;

  /// No description provided for @l6LinkedRevision4f2da.
  ///
  /// In zh, this message translates to:
  /// **'关联修订'**
  String get l6LinkedRevision4f2da;

  /// No description provided for @l6PreviousContent5b344.
  ///
  /// In zh, this message translates to:
  /// **'修改前正文'**
  String get l6PreviousContent5b344;

  /// No description provided for @l6SavedContentc80ef.
  ///
  /// In zh, this message translates to:
  /// **'实际保存的正文'**
  String get l6SavedContentc80ef;

  /// No description provided for @l6Reload77849.
  ///
  /// In zh, this message translates to:
  /// **'重新读取'**
  String get l6Reload77849;

  /// No description provided for @l6LoadMoreRecords37269.
  ///
  /// In zh, this message translates to:
  /// **'加载更多记录'**
  String get l6LoadMoreRecords37269;

  /// No description provided for @l6Waiting25d23.
  ///
  /// In zh, this message translates to:
  /// **'等待'**
  String get l6Waiting25d23;

  /// No description provided for @l6Skipped9f38a.
  ///
  /// In zh, this message translates to:
  /// **'已跳过'**
  String get l6Skipped9f38a;

  /// No description provided for @l6ProcessingQueueb0388.
  ///
  /// In zh, this message translates to:
  /// **'整理队列'**
  String get l6ProcessingQueueb0388;

  /// No description provided for @l6ViewPendingWorkReadingProgressAnd0ed68.
  ///
  /// In zh, this message translates to:
  /// **'查看积压、阅读进度和等待原因'**
  String get l6ViewPendingWorkReadingProgressAnd0ed68;

  /// No description provided for @l6NoTasksInThisState7922e.
  ///
  /// In zh, this message translates to:
  /// **'此状态下暂无任务。'**
  String get l6NoTasksInThisState7922e;

  /// No description provided for @l6ReloadQueueb9f40.
  ///
  /// In zh, this message translates to:
  /// **'重新读取队列'**
  String get l6ReloadQueueb9f40;

  /// No description provided for @l6LoadMoreTasks1db39.
  ///
  /// In zh, this message translates to:
  /// **'加载更多任务'**
  String get l6LoadMoreTasks1db39;

  /// No description provided for @l6PrepareSourceRepair2b5a2.
  ///
  /// In zh, this message translates to:
  /// **'准备来源修复'**
  String get l6PrepareSourceRepair2b5a2;

  /// No description provided for @l6ThereIsNotEnoughEvidenceAddfdb09.
  ///
  /// In zh, this message translates to:
  /// **'剩余依据不足，请先补充有效原料。当前知识保留并等待补证据。'**
  String get l6ThereIsNotEnoughEvidenceAddfdb09;

  /// No description provided for @l6TheSelectedSourcesAreBeingReadb3e01.
  ///
  /// In zh, this message translates to:
  /// **'所选原料正在后台阅读全文，请在阅读完成后准备修复建议。'**
  String get l6TheSelectedSourcesAreBeingReadb3e01;

  /// No description provided for @l6UpstreamKnowledgeae238.
  ///
  /// In zh, this message translates to:
  /// **'上游知识'**
  String get l6UpstreamKnowledgeae238;

  /// No description provided for @l6UpstreamKnowledgeNeedsReviewOpenItsb9c0d.
  ///
  /// In zh, this message translates to:
  /// **'上游知识仍待复核，请先打开上游页面检查更新。'**
  String get l6UpstreamKnowledgeNeedsReviewOpenItsb9c0d;

  /// No description provided for @l6Actionf3ea6.
  ///
  /// In zh, this message translates to:
  /// **'操作'**
  String get l6Actionf3ea6;

  /// No description provided for @l6PageDoesNotExist55c9e.
  ///
  /// In zh, this message translates to:
  /// **'页面不存在'**
  String get l6PageDoesNotExist55c9e;

  /// No description provided for @l6SourceRecords5bc8d.
  ///
  /// In zh, this message translates to:
  /// **'来源记录'**
  String get l6SourceRecords5bc8d;

  /// No description provided for @l6SourceRecordDoesNotExist25551.
  ///
  /// In zh, this message translates to:
  /// **'来源记录不存在'**
  String get l6SourceRecordDoesNotExist25551;

  /// No description provided for @l6CouldNotLoadd9f60.
  ///
  /// In zh, this message translates to:
  /// **'读取失败'**
  String get l6CouldNotLoadd9f60;

  /// No description provided for @l6LoadSourceRecordse26e1.
  ///
  /// In zh, this message translates to:
  /// **'读取来源记录'**
  String get l6LoadSourceRecordse26e1;

  /// No description provided for @l6DecideOnThisSource4e2fd.
  ///
  /// In zh, this message translates to:
  /// **'摘录定论'**
  String get l6DecideOnThisSource4e2fd;

  /// No description provided for @l6DiscardAsRejectedKnowledge14322.
  ///
  /// In zh, this message translates to:
  /// **'丢弃为负知识'**
  String get l6DiscardAsRejectedKnowledge14322;

  /// No description provided for @l6RequiredReasonForDiscarding0dc5c.
  ///
  /// In zh, this message translates to:
  /// **'必填：丢弃原因'**
  String get l6RequiredReasonForDiscarding0dc5c;

  /// No description provided for @l6OrganizeKnowledge210a3.
  ///
  /// In zh, this message translates to:
  /// **'整理知识'**
  String get l6OrganizeKnowledge210a3;

  /// No description provided for @l6ExtractOnDemandSaveNewReference28c44.
  ///
  /// In zh, this message translates to:
  /// **'按需提炼，新增参考知识直接保存'**
  String get l6ExtractOnDemandSaveNewReference28c44;

  /// No description provided for @l6ExtractAMethod9c1f5.
  ///
  /// In zh, this message translates to:
  /// **'提炼方法'**
  String get l6ExtractAMethod9c1f5;

  /// No description provided for @l6ReusableStepsAndConditions4c52e.
  ///
  /// In zh, this message translates to:
  /// **'可复用的步骤与适用条件'**
  String get l6ReusableStepsAndConditions4c52e;

  /// No description provided for @l6OrganizeACaseac694.
  ///
  /// In zh, this message translates to:
  /// **'整理案例'**
  String get l6OrganizeACaseac694;

  /// No description provided for @l6ContextApproachAndOutcomeee83d.
  ///
  /// In zh, this message translates to:
  /// **'具体情境、做法与结果'**
  String get l6ContextApproachAndOutcomeee83d;

  /// No description provided for @l6ExtractAPrinciple14a19.
  ///
  /// In zh, this message translates to:
  /// **'提炼规律'**
  String get l6ExtractAPrinciple14a19;

  /// No description provided for @l6PrinciplesEvidenceAndLimits440fe.
  ///
  /// In zh, this message translates to:
  /// **'规律、依据与适用边界'**
  String get l6PrinciplesEvidenceAndLimits440fe;

  /// No description provided for @l6ExtractKnowledge196cf.
  ///
  /// In zh, this message translates to:
  /// **'提炼知识'**
  String get l6ExtractKnowledge196cf;

  /// No description provided for @l6IncludeInKnowledge29c9c.
  ///
  /// In zh, this message translates to:
  /// **'纳入知识'**
  String get l6IncludeInKnowledge29c9c;

  /// No description provided for @l6IncludedInKnowledge9a862.
  ///
  /// In zh, this message translates to:
  /// **'已纳入知识'**
  String get l6IncludedInKnowledge9a862;

  /// No description provided for @l6DiscardedRejectedKnowledge57399.
  ///
  /// In zh, this message translates to:
  /// **'已丢弃（负知识）'**
  String get l6DiscardedRejectedKnowledge57399;

  /// No description provided for @l6ReferenceKnowledgeIsOrganizedAutomaticallyYoucefad.
  ///
  /// In zh, this message translates to:
  /// **'后台自动整理参考知识；人工修改与规则确认由你决定'**
  String get l6ReferenceKnowledgeIsOrganizedAutomaticallyYoucefad;

  /// No description provided for @l6LoadProposal021f2.
  ///
  /// In zh, this message translates to:
  /// **'读取提案'**
  String get l6LoadProposal021f2;

  /// No description provided for @l6NoChangesNeedYourConfirmationNew743fb.
  ///
  /// In zh, this message translates to:
  /// **'暂无需要你确认的修改。新增参考知识已直接保存在产出中。'**
  String get l6NoChangesNeedYourConfirmationNew743fb;

  /// No description provided for @l6SourceAndContentChecks5517e.
  ///
  /// In zh, this message translates to:
  /// **'来源与内容检查'**
  String get l6SourceAndContentChecks5517e;

  /// No description provided for @l6LoadCheckse2e22.
  ///
  /// In zh, this message translates to:
  /// **'读取检查项'**
  String get l6LoadCheckse2e22;

  /// No description provided for @l6NoChecksNeedReview4053b.
  ///
  /// In zh, this message translates to:
  /// **'暂无待核对提示。'**
  String get l6NoChecksNeedReview4053b;

  /// No description provided for @l6FailedAutomaticRetryScheduledc9e00.
  ///
  /// In zh, this message translates to:
  /// **'失败，稍后自动重试'**
  String get l6FailedAutomaticRetryScheduledc9e00;

  /// No description provided for @l6SupportingEvidenceabba3.
  ///
  /// In zh, this message translates to:
  /// **'引用依据'**
  String get l6SupportingEvidenceabba3;

  /// No description provided for @l6KnowledgeCitedInThisAnswer4ad6e.
  ///
  /// In zh, this message translates to:
  /// **'本条回答实际引用的知识'**
  String get l6KnowledgeCitedInThisAnswer4ad6e;

  /// No description provided for @l6ThisAnswerHasNoSystemVerified0ba58.
  ///
  /// In zh, this message translates to:
  /// **'这条回答没有经过系统核验的知识引用。'**
  String get l6ThisAnswerHasNoSystemVerified0ba58;

  /// No description provided for @l6SourceVersionDoesNotExist8a671.
  ///
  /// In zh, this message translates to:
  /// **'来源版本不存在'**
  String get l6SourceVersionDoesNotExist8a671;

  /// No description provided for @l6ViewSourceEvent3c3f9.
  ///
  /// In zh, this message translates to:
  /// **'查看来源事件'**
  String get l6ViewSourceEvent3c3f9;

  /// No description provided for @l6BackgroundProcessingOrganizesReferenceKnowledgeAnd36787.
  ///
  /// In zh, this message translates to:
  /// **'后台会整理参考知识，并结合相关资料维护共同主题。按需提炼的新增产出直接保存；涉及人工修改或已确认内容时才需审阅。原文保留，个人规则由你确认。'**
  String get l6BackgroundProcessingOrganizesReferenceKnowledgeAnd36787;

  /// No description provided for @l6SuggestionsToReview822f0.
  ///
  /// In zh, this message translates to:
  /// **'待审建议'**
  String get l6SuggestionsToReview822f0;

  /// No description provided for @l6CompiledReferenceKnowledge9efb1.
  ///
  /// In zh, this message translates to:
  /// **'已整理的参考知识'**
  String get l6CompiledReferenceKnowledge9efb1;

  /// No description provided for @l6NoKnowledgeOutputsYetYouCan01967.
  ///
  /// In zh, this message translates to:
  /// **'暂无知识产出。原文仍可直接使用；后台整理会按内容决定是否形成知识。'**
  String get l6NoKnowledgeOutputsYetYouCan01967;

  /// No description provided for @l6LoadChanges6cd73.
  ///
  /// In zh, this message translates to:
  /// **'读取差异'**
  String get l6LoadChanges6cd73;

  /// No description provided for @l6ReviewProposal8e384.
  ///
  /// In zh, this message translates to:
  /// **'处理提案'**
  String get l6ReviewProposal8e384;

  /// No description provided for @l6Handled219bd.
  ///
  /// In zh, this message translates to:
  /// **'已处理'**
  String get l6Handled219bd;

  /// No description provided for @l6CompareWithCurrentContenta3c20.
  ///
  /// In zh, this message translates to:
  /// **'对照当前正文'**
  String get l6CompareWithCurrentContenta3c20;

  /// No description provided for @l6CurrentContent47be3.
  ///
  /// In zh, this message translates to:
  /// **'当前正文'**
  String get l6CurrentContent47be3;

  /// No description provided for @l6LoadPageContent3f16e.
  ///
  /// In zh, this message translates to:
  /// **'读取页面正文'**
  String get l6LoadPageContent3f16e;

  /// No description provided for @l6ReviewParagraphByParagraph072ad.
  ///
  /// In zh, this message translates to:
  /// **'逐段审阅'**
  String get l6ReviewParagraphByParagraph072ad;

  /// No description provided for @l6OriginalParagraphe62c3.
  ///
  /// In zh, this message translates to:
  /// **'原段落'**
  String get l6OriginalParagraphe62c3;

  /// No description provided for @l6SuggestedParagraphcd73c.
  ///
  /// In zh, this message translates to:
  /// **'建议段落'**
  String get l6SuggestedParagraphcd73c;

  /// No description provided for @l6AlsoAcceptTheSuggestedConditionsbd478.
  ///
  /// In zh, this message translates to:
  /// **'同时采纳建议的适用条件'**
  String get l6AlsoAcceptTheSuggestedConditionsbd478;

  /// No description provided for @l6WhichIssuesThisRevisionResolvesYou7c3a0.
  ///
  /// In zh, this message translates to:
  /// **'本次修订解决了哪些提示（由你确认）：'**
  String get l6WhichIssuesThisRevisionResolvesYou7c3a0;

  /// No description provided for @l6ChangesToSourceEvidenceRequireFull6139c.
  ///
  /// In zh, this message translates to:
  /// **'如果来源依据同时变化，需要完整确认；局部采纳只用于来源不变的修订。'**
  String get l6ChangesToSourceEvidenceRequireFull6139c;

  /// No description provided for @l6ViewSourceMaterialdf8d0.
  ///
  /// In zh, this message translates to:
  /// **'查看原料'**
  String get l6ViewSourceMaterialdf8d0;

  /// No description provided for @l6LoadSourceSnapshot1cba8.
  ///
  /// In zh, this message translates to:
  /// **'读取素材快照'**
  String get l6LoadSourceSnapshot1cba8;

  /// No description provided for @l6ViewEventfdcb3.
  ///
  /// In zh, this message translates to:
  /// **'查看事件'**
  String get l6ViewEventfdcb3;

  /// No description provided for @l6HideComparisond3329.
  ///
  /// In zh, this message translates to:
  /// **'收起对照'**
  String get l6HideComparisond3329;

  /// No description provided for @l6CompareWithTheCurrentContentConfirming9fbf7.
  ///
  /// In zh, this message translates to:
  /// **'请对照当前正文。确认会应用以上修改；若本页是个人规则，也表示你确认修订后的规则及适用条件。'**
  String get l6CompareWithTheCurrentContentConfirming9fbf7;

  /// No description provided for @l6ConfirmRevision97031.
  ///
  /// In zh, this message translates to:
  /// **'确认修订'**
  String get l6ConfirmRevision97031;

  /// No description provided for @l6RejectSuggestionb7fe0.
  ///
  /// In zh, this message translates to:
  /// **'拒绝建议'**
  String get l6RejectSuggestionb7fe0;

  /// No description provided for @l6HandledOriginalTextPreserved3bc8e.
  ///
  /// In zh, this message translates to:
  /// **'已处理，原文保持不变'**
  String get l6HandledOriginalTextPreserved3bc8e;

  /// No description provided for @l6RejectedDecisionPreserved97f65.
  ///
  /// In zh, this message translates to:
  /// **'已拒绝；保留决定记录'**
  String get l6RejectedDecisionPreserved97f65;

  /// No description provided for @l6DismissHint0e8a6.
  ///
  /// In zh, this message translates to:
  /// **'忽略提示'**
  String get l6DismissHint0e8a6;

  /// No description provided for @l6OperationFailed09e42.
  ///
  /// In zh, this message translates to:
  /// **'操作失败'**
  String get l6OperationFailed09e42;

  /// No description provided for @l6DismissCheck617f7.
  ///
  /// In zh, this message translates to:
  /// **'忽略检查项'**
  String get l6DismissCheck617f7;

  /// No description provided for @l6LoadRevisions0cbe1.
  ///
  /// In zh, this message translates to:
  /// **'读取修订记录'**
  String get l6LoadRevisions0cbe1;

  /// No description provided for @l6GenerateRestorationProposal29825.
  ///
  /// In zh, this message translates to:
  /// **'生成恢复提案'**
  String get l6GenerateRestorationProposal29825;

  /// No description provided for @l6TheRestorationProposalIsInOutputsde198.
  ///
  /// In zh, this message translates to:
  /// **'恢复提案已放入产出页，确认后生效；当前来源和规则设置保留。'**
  String get l6TheRestorationProposalIsInOutputsde198;

  /// No description provided for @l6PrepareRestoration94c4c.
  ///
  /// In zh, this message translates to:
  /// **'准备恢复'**
  String get l6PrepareRestoration94c4c;

  /// No description provided for @l6SourceAssessment7740d.
  ///
  /// In zh, this message translates to:
  /// **'材料评价'**
  String get l6SourceAssessment7740d;

  /// No description provided for @l6Endorse8a231.
  ///
  /// In zh, this message translates to:
  /// **'认可'**
  String get l6Endorse8a231;

  /// No description provided for @l6Reject802d6.
  ///
  /// In zh, this message translates to:
  /// **'不认可'**
  String get l6Reject802d6;

  /// No description provided for @l6UseThisAsAPersonalRulefc28c.
  ///
  /// In zh, this message translates to:
  /// **'确认作为规则使用？'**
  String get l6UseThisAsAPersonalRulefc28c;

  /// No description provided for @l6AiWillConsultThisRuleWhenf912f.
  ///
  /// In zh, this message translates to:
  /// **'AI 会在符合适用条件时对照这条规则。请确认内容可靠且适用条件准确。'**
  String get l6AiWillConsultThisRuleWhenf912f;

  /// No description provided for @l6ConfirmUpgrade8c3fd.
  ///
  /// In zh, this message translates to:
  /// **'确认升级'**
  String get l6ConfirmUpgrade8c3fd;

  /// No description provided for @l6SaveMetadata757dd.
  ///
  /// In zh, this message translates to:
  /// **'保存元数据'**
  String get l6SaveMetadata757dd;

  /// No description provided for @l6Conditions24481.
  ///
  /// In zh, this message translates to:
  /// **'适用条件'**
  String get l6Conditions24481;

  /// No description provided for @l6ReferenceMaterialc3d33.
  ///
  /// In zh, this message translates to:
  /// **'参考材料'**
  String get l6ReferenceMaterialc3d33;

  /// No description provided for @l6ReusableMethod12837.
  ///
  /// In zh, this message translates to:
  /// **'可复用方法'**
  String get l6ReusableMethod12837;

  /// No description provided for @l6ConfirmedRuleba4b1.
  ///
  /// In zh, this message translates to:
  /// **'已确认规则'**
  String get l6ConfirmedRuleba4b1;

  /// No description provided for @l6SaveConditionsAndStrength46f33.
  ///
  /// In zh, this message translates to:
  /// **'保存适用条件与强度'**
  String get l6SaveConditionsAndStrength46f33;

  /// No description provided for @l6SourcesLetYouVerifyEvidenceRevisionsccae6.
  ///
  /// In zh, this message translates to:
  /// **'来源用于核对依据，修订记录用于查看知识演进。日常阅读和 AI 检索不需要逐项操作。'**
  String get l6SourcesLetYouVerifyEvidenceRevisionsccae6;

  /// No description provided for @l6UpstreamBasis0fd54.
  ///
  /// In zh, this message translates to:
  /// **'上游依据'**
  String get l6UpstreamBasis0fd54;

  /// No description provided for @l6NoVerifiableSourcesYet8b47f.
  ///
  /// In zh, this message translates to:
  /// **'尚无可核验来源。'**
  String get l6NoVerifiableSourcesYet8b47f;

  /// No description provided for @l6SourceEvents2c4b3.
  ///
  /// In zh, this message translates to:
  /// **'来源事件记录'**
  String get l6SourceEvents2c4b3;

  /// No description provided for @l6OriginalSourceVersions24704.
  ///
  /// In zh, this message translates to:
  /// **'原文历史版本'**
  String get l6OriginalSourceVersions24704;

  /// No description provided for @l6KnowledgeRevisions42307.
  ///
  /// In zh, this message translates to:
  /// **'知识修订记录'**
  String get l6KnowledgeRevisions42307;

  /// No description provided for @l6ViewAndRestorePastRevisionsOfd9312.
  ///
  /// In zh, this message translates to:
  /// **'查看本页正文的历次修订与恢复'**
  String get l6ViewAndRestorePastRevisionsOfd9312;

  /// No description provided for @l6SourceIsTemporarilyUnavailablecb78e.
  ///
  /// In zh, this message translates to:
  /// **'来源暂时无法读取'**
  String get l6SourceIsTemporarilyUnavailablecb78e;

  /// No description provided for @l6DeviceInformation67298.
  ///
  /// In zh, this message translates to:
  /// **'本机信息'**
  String get l6DeviceInformation67298;

  /// No description provided for @l6Rename1cd80.
  ///
  /// In zh, this message translates to:
  /// **'重命名'**
  String get l6Rename1cd80;

  /// No description provided for @l6DeviceId464c0.
  ///
  /// In zh, this message translates to:
  /// **'本机 ID：'**
  String get l6DeviceId464c0;

  /// No description provided for @l6RenameThisDevice304fd.
  ///
  /// In zh, this message translates to:
  /// **'重命名本机设备'**
  String get l6RenameThisDevice304fd;

  /// No description provided for @l6DeviceNamed889e.
  ///
  /// In zh, this message translates to:
  /// **'设备名'**
  String get l6DeviceNamed889e;

  /// No description provided for @l6RelayServiceUnlockDeviceKeysTo0103f.
  ///
  /// In zh, this message translates to:
  /// **'中继服务：解锁本机密钥后可见'**
  String get l6RelayServiceUnlockDeviceKeysTo0103f;

  /// No description provided for @l6RelayServiceda32a.
  ///
  /// In zh, this message translates to:
  /// **'中继服务：'**
  String get l6RelayServiceda32a;

  /// No description provided for @l6CopyRelayAddress61683.
  ///
  /// In zh, this message translates to:
  /// **'复制中继地址'**
  String get l6CopyRelayAddress61683;

  /// No description provided for @l6RelayAddressCopied7db6a.
  ///
  /// In zh, this message translates to:
  /// **'中继地址已复制'**
  String get l6RelayAddressCopied7db6a;

  /// No description provided for @l6Restore79748.
  ///
  /// In zh, this message translates to:
  /// **'恢复'**
  String get l6Restore79748;

  /// No description provided for @l6TransferAiAnalysisResponsibilityfe457.
  ///
  /// In zh, this message translates to:
  /// **'AI 分析职责移交'**
  String get l6TransferAiAnalysisResponsibilityfe457;

  /// No description provided for @l6ThisDeviceIsNotInvolvedInaf62a.
  ///
  /// In zh, this message translates to:
  /// **'本机状态：未参与移交。'**
  String get l6ThisDeviceIsNotInvolvedInaf62a;

  /// No description provided for @l6ResponsibilityHasBeenHandedOffWaitingeac50.
  ///
  /// In zh, this message translates to:
  /// **'本机已让出 AI 分析职责，等待目标设备确认接管。'**
  String get l6ResponsibilityHasBeenHandedOffWaitingeac50;

  /// No description provided for @l6AHandoffAwaitsThisDeviceSdd081.
  ///
  /// In zh, this message translates to:
  /// **'有移交等待本机确认（同步已追平）。'**
  String get l6AHandoffAwaitsThisDeviceSdd081;

  /// No description provided for @l6AHandoffAwaitsConfirmationButThis0f114.
  ///
  /// In zh, this message translates to:
  /// **'有移交等待本机确认，但本机尚未同步到移交边界。'**
  String get l6AHandoffAwaitsConfirmationButThis0f114;

  /// No description provided for @l6ThisDeviceRunsAiAnalysisThe54d4b.
  ///
  /// In zh, this message translates to:
  /// **'本机正在负责 AI 分析（此前的一次接管已完成）。'**
  String get l6ThisDeviceRunsAiAnalysisThe54d4b;

  /// No description provided for @l6AcceptResponsibility6c220.
  ///
  /// In zh, this message translates to:
  /// **'确认接管'**
  String get l6AcceptResponsibility6c220;

  /// No description provided for @l6NoEligibleTargetDevicesAuthorizeAaba52.
  ///
  /// In zh, this message translates to:
  /// **'暂无可接收设备，请先授权设备加入。'**
  String get l6NoEligibleTargetDevicesAuthorizeAaba52;

  /// No description provided for @l6TargetDevice1b73f.
  ///
  /// In zh, this message translates to:
  /// **'目标设备'**
  String get l6TargetDevice1b73f;

  /// No description provided for @l6TransferAiAnalysisResponsibility2e5ba.
  ///
  /// In zh, this message translates to:
  /// **'让出 AI 分析职责'**
  String get l6TransferAiAnalysisResponsibility2e5ba;

  /// No description provided for @l6WaitingForTheOwnerToApprovebf73f.
  ///
  /// In zh, this message translates to:
  /// **'等待主设备审批移交证明。'**
  String get l6WaitingForTheOwnerToApprovebf73f;

  /// No description provided for @l6ProcessingHandoffed58d.
  ///
  /// In zh, this message translates to:
  /// **'正在处理移交步骤，请等待…'**
  String get l6ProcessingHandoffed58d;

  /// No description provided for @l6TransferAiAnalysisResponsibility5d2ad.
  ///
  /// In zh, this message translates to:
  /// **'让出 AI 分析职责？'**
  String get l6TransferAiAnalysisResponsibility5d2ad;

  /// No description provided for @l6Transferc79cc.
  ///
  /// In zh, this message translates to:
  /// **'让出'**
  String get l6Transferc79cc;

  /// No description provided for @l6JoinWithAnInvitation78bca.
  ///
  /// In zh, this message translates to:
  /// **'通过邀请码加入'**
  String get l6JoinWithAnInvitation78bca;

  /// No description provided for @l6InvitationFromATrustedDeviceade9c.
  ///
  /// In zh, this message translates to:
  /// **'来自可信设备的邀请码'**
  String get l6InvitationFromATrustedDeviceade9c;

  /// No description provided for @l6PreviewInvitationOfflinee39bc.
  ///
  /// In zh, this message translates to:
  /// **'预览邀请（不联网）'**
  String get l6PreviewInvitationOfflinee39bc;

  /// No description provided for @l6JoinAnExistingDatasetb3b50.
  ///
  /// In zh, this message translates to:
  /// **'加入现有数据集'**
  String get l6JoinAnExistingDatasetb3b50;

  /// No description provided for @l6PreviewAndVerifyInvitation1afde.
  ///
  /// In zh, this message translates to:
  /// **'预览并核对邀请'**
  String get l6PreviewAndVerifyInvitation1afde;

  /// No description provided for @l6SendJoinRequestd5d20.
  ///
  /// In zh, this message translates to:
  /// **'发送加入请求'**
  String get l6SendJoinRequestd5d20;

  /// No description provided for @l6WaitingForOwnerApproval9e900.
  ///
  /// In zh, this message translates to:
  /// **'等待主设备批准'**
  String get l6WaitingForOwnerApproval9e900;

  /// No description provided for @l6ConfirmInstallation555cc.
  ///
  /// In zh, this message translates to:
  /// **'安装确认'**
  String get l6ConfirmInstallation555cc;

  /// No description provided for @l6WaitingForAuthorizationAndInitialSync6cedf.
  ///
  /// In zh, this message translates to:
  /// **'等待成员授权与初始同步'**
  String get l6WaitingForAuthorizationAndInitialSync6cedf;

  /// No description provided for @l6ReceivingTheInitialSnapshot032cf.
  ///
  /// In zh, this message translates to:
  /// **'正在接收初始快照'**
  String get l6ReceivingTheInitialSnapshot032cf;

  /// No description provided for @l6RelayMailboxFullDeliveryResumesAutomatically4a933.
  ///
  /// In zh, this message translates to:
  /// **'中继信箱已满：对方正在取件腾出空间，稍候会自动继续，无需操作。'**
  String get l6RelayMailboxFullDeliveryResumesAutomatically4a933;

  /// No description provided for @l6ScanQrCodeb8a8f.
  ///
  /// In zh, this message translates to:
  /// **'扫码'**
  String get l6ScanQrCodeb8a8f;

  /// No description provided for @l6RelayToConnectTo06ef4.
  ///
  /// In zh, this message translates to:
  /// **'将连接的中继'**
  String get l6RelayToConnectTo06ef4;

  /// No description provided for @l6InvitingDeviceSPublicKeyFingerprint283a5.
  ///
  /// In zh, this message translates to:
  /// **'邀请设备公钥指纹'**
  String get l6InvitingDeviceSPublicKeyFingerprint283a5;

  /// No description provided for @l6ThisDeviceSPublicKeyFingerprintb7e3a.
  ///
  /// In zh, this message translates to:
  /// **'本机公钥指纹（请在已有设备核对）'**
  String get l6ThisDeviceSPublicKeyFingerprintb7e3a;

  /// No description provided for @l6ReviewTheInvitationThenExplicitlyChoosece487.
  ///
  /// In zh, this message translates to:
  /// **'请核对邀请后明确选择加入。'**
  String get l6ReviewTheInvitationThenExplicitlyChoosece487;

  /// No description provided for @l6PreparingAndVerifyingProtectedDeviceKeys60dc8.
  ///
  /// In zh, this message translates to:
  /// **'正在准备并确认本机受保护密钥。'**
  String get l6PreparingAndVerifyingProtectedDeviceKeys60dc8;

  /// No description provided for @l6DeviceKeysAreReadyRequestDeliverye8427.
  ///
  /// In zh, this message translates to:
  /// **'本机密钥已准备，尚未确认请求投递。'**
  String get l6DeviceKeysAreReadyRequestDeliverye8427;

  /// No description provided for @l6WaitingForAnExistingDeviceTo330c1.
  ///
  /// In zh, this message translates to:
  /// **'等待已有设备核对指纹并明确批准。'**
  String get l6WaitingForAnExistingDeviceTo330c1;

  /// No description provided for @l6WaitingForSystemConfirmationNoProof188b0.
  ///
  /// In zh, this message translates to:
  /// **'等待系统确认，确认前不发送证明。'**
  String get l6WaitingForSystemConfirmationNoProof188b0;

  /// No description provided for @l6InstallationIsSavedProofDeliveryIsf28de.
  ///
  /// In zh, this message translates to:
  /// **'安装已保存，尚未确认安装证明投递。'**
  String get l6InstallationIsSavedProofDeliveryIsf28de;

  /// No description provided for @l6InstallationProofDeliveredWaitingForAuthorization5d65b.
  ///
  /// In zh, this message translates to:
  /// **'安装证明已投递，等待授权与初始同步。'**
  String get l6InstallationProofDeliveredWaitingForAuthorization5d65b;

  /// No description provided for @l6CoordinatingTheApprovedHandoffAutomaticallyCheck9b863.
  ///
  /// In zh, this message translates to:
  /// **'正在自动协调已批准的初始交接，请查看交接进度。'**
  String get l6CoordinatingTheApprovedHandoffAutomaticallyCheck9b863;

  /// No description provided for @l6ConfirmingHandoffRevisionsSendingResumesAftera5068.
  ///
  /// In zh, this message translates to:
  /// **'正在确认交接修订，确认前不继续发送。'**
  String get l6ConfirmingHandoffRevisionsSendingResumesAftera5068;

  /// No description provided for @l6TheInvitationExpiredRequestANewf2a42.
  ///
  /// In zh, this message translates to:
  /// **'邀请已过期，需重新邀请。'**
  String get l6TheInvitationExpiredRequestANewf2a42;

  /// No description provided for @l6JoiningRequiresANewInvitationOr2a99d.
  ///
  /// In zh, this message translates to:
  /// **'加入需要重新邀请或恢复。'**
  String get l6JoiningRequiresANewInvitationOr2a99d;

  /// No description provided for @l6NewDeviceNamed42db.
  ///
  /// In zh, this message translates to:
  /// **'新设备名称'**
  String get l6NewDeviceNamed42db;

  /// No description provided for @l6IHaveCheckedTheInvitationAnd15ccd.
  ///
  /// In zh, this message translates to:
  /// **'我已核对邀请和中继，明确同意连接该地址，并接收全部 Provider 配置和 API Key。'**
  String get l6IHaveCheckedTheInvitationAnd15ccd;

  /// No description provided for @l6RequestToJoinUsingASeparate835e5.
  ///
  /// In zh, this message translates to:
  /// **'在独立暂存库发起加入'**
  String get l6RequestToJoinUsingASeparate835e5;

  /// No description provided for @l6ResendOriginalJoinRequest97725.
  ///
  /// In zh, this message translates to:
  /// **'重发原加入请求'**
  String get l6ResendOriginalJoinRequest97725;

  /// No description provided for @l6CheckForApproval58c9a.
  ///
  /// In zh, this message translates to:
  /// **'检查已有设备审批'**
  String get l6CheckForApproval58c9a;

  /// No description provided for @l6ResendSavedInstallationProof3d80a.
  ///
  /// In zh, this message translates to:
  /// **'重发已保存的安装证明'**
  String get l6ResendSavedInstallationProof3d80a;

  /// No description provided for @l6StopThisDeviceSEnrollmentb3a98.
  ///
  /// In zh, this message translates to:
  /// **'停止本机加入流程'**
  String get l6StopThisDeviceSEnrollmentb3a98;

  /// No description provided for @l6ApprovalCleanupIsNotConfirmedResending9fd47.
  ///
  /// In zh, this message translates to:
  /// **'审批邮件的清理尚未确认；重发证明可再次确认清理，不会重新安装。'**
  String get l6ApprovalCleanupIsNotConfirmedResending9fd47;

  /// No description provided for @l6ViewRecoverableEnrollmentStagingEntriesf095c.
  ///
  /// In zh, this message translates to:
  /// **'查看可恢复的加入暂存项'**
  String get l6ViewRecoverableEnrollmentStagingEntriesf095c;

  /// No description provided for @l6UnlockAndRecovere9f7c.
  ///
  /// In zh, this message translates to:
  /// **'解锁恢复'**
  String get l6UnlockAndRecovere9f7c;

  /// No description provided for @l6DeleteThisStagingEntrya06bf.
  ///
  /// In zh, this message translates to:
  /// **'删除这条暂存项？'**
  String get l6DeleteThisStagingEntrya06bf;

  /// No description provided for @l6TheCurrentStorageBackendDoesNot2cb78.
  ///
  /// In zh, this message translates to:
  /// **'当前存储后端不支持设备同步。'**
  String get l6TheCurrentStorageBackendDoesNot2cb78;

  /// No description provided for @l6SyncIsEnabled210fe.
  ///
  /// In zh, this message translates to:
  /// **'同步已开启'**
  String get l6SyncIsEnabled210fe;

  /// No description provided for @l6SyncIsPausedThisDeviceS7fecc.
  ///
  /// In zh, this message translates to:
  /// **'同步已暂停（本机密钥已锁定）'**
  String get l6SyncIsPausedThisDeviceS7fecc;

  /// No description provided for @l6SyncIsDisabledThisDeviceIs6aad5.
  ///
  /// In zh, this message translates to:
  /// **'同步已关闭（本机未被授权）'**
  String get l6SyncIsDisabledThisDeviceIs6aad5;

  /// No description provided for @l6SyncNeedsRepair8e4fa.
  ///
  /// In zh, this message translates to:
  /// **'同步需要修复'**
  String get l6SyncNeedsRepair8e4fa;

  /// No description provided for @l6SyncIsNotEnabledbccba.
  ///
  /// In zh, this message translates to:
  /// **'同步未开启'**
  String get l6SyncIsNotEnabledbccba;

  /// No description provided for @l6LoadingSyncStatus861b0.
  ///
  /// In zh, this message translates to:
  /// **'正在读取同步状态'**
  String get l6LoadingSyncStatus861b0;

  /// No description provided for @l6SyncStatusIsUnavailable0971a.
  ///
  /// In zh, this message translates to:
  /// **'同步状态暂不可用'**
  String get l6SyncStatusIsUnavailable0971a;

  /// No description provided for @l6OwnerDeviceRunsBackgroundAiAnalysis73dc6.
  ///
  /// In zh, this message translates to:
  /// **'主设备 · 负责后台 AI 分析'**
  String get l6OwnerDeviceRunsBackgroundAiAnalysis73dc6;

  /// No description provided for @l6MemberDeviceRunsBackgroundAiAnalysisf055d.
  ///
  /// In zh, this message translates to:
  /// **'成员设备 · 负责后台 AI 分析'**
  String get l6MemberDeviceRunsBackgroundAiAnalysisf055d;

  /// No description provided for @l6MemberDevicee071b.
  ///
  /// In zh, this message translates to:
  /// **'成员设备'**
  String get l6MemberDevicee071b;

  /// No description provided for @l6Pairing6aed7.
  ///
  /// In zh, this message translates to:
  /// **'绑定'**
  String get l6Pairing6aed7;

  /// No description provided for @l6Manage4989b.
  ///
  /// In zh, this message translates to:
  /// **'管理'**
  String get l6Manage4989b;

  /// No description provided for @l6Clearingf54a6.
  ///
  /// In zh, this message translates to:
  /// **'正在清除…'**
  String get l6Clearingf54a6;

  /// No description provided for @l6ClearThisDeviceSSyncIdentityab953.
  ///
  /// In zh, this message translates to:
  /// **'清除本机同步身份'**
  String get l6ClearThisDeviceSSyncIdentityab953;

  /// No description provided for @l6SavedProviderApiKeysArePreservedb2ce9.
  ///
  /// In zh, this message translates to:
  /// **'· 已保存的 Provider API Key 会自动保留（重新配对后自动迁移）；'**
  String get l6SavedProviderApiKeysArePreservedb2ce9;

  /// No description provided for @l6SavedProviderApiKeysWillStop714a6.
  ///
  /// In zh, this message translates to:
  /// **'· 已保存的 Provider API Key 会失效，需要重新填写；'**
  String get l6SavedProviderApiKeysWillStop714a6;

  /// No description provided for @l6IUnderstandThatThisDeviceMust3caf2.
  ///
  /// In zh, this message translates to:
  /// **'我已知晓，本机需要重新配对才能同步'**
  String get l6IUnderstandThatThisDeviceMust3caf2;

  /// No description provided for @l6IUnderstandThatProviderApiKeys4b028.
  ///
  /// In zh, this message translates to:
  /// **'我已知晓，Provider API Key 需要重新填写'**
  String get l6IUnderstandThatProviderApiKeys4b028;

  /// No description provided for @l6ClearThisDeviceSSyncIdentityf7c83.
  ///
  /// In zh, this message translates to:
  /// **'清除本机同步身份？'**
  String get l6ClearThisDeviceSSyncIdentityf7c83;

  /// No description provided for @l6AfterClearing12369.
  ///
  /// In zh, this message translates to:
  /// **'清除后：'**
  String get l6AfterClearing12369;

  /// No description provided for @l6ThisDeviceLeavesTheDatasetAnd73b97.
  ///
  /// In zh, this message translates to:
  /// **'· 本机退出当前数据集，需要重新配对或重新加入才能再同步；'**
  String get l6ThisDeviceLeavesTheDatasetAnd73b97;

  /// No description provided for @l6ChangesNotYetSyncedToOther8c274.
  ///
  /// In zh, this message translates to:
  /// **'· 还没同步到其它设备的改动不会自动补发。'**
  String get l6ChangesNotYetSyncedToOther8c274;

  /// No description provided for @l6NotesConversationsAndKnowledgeArePreserved9b3e9.
  ///
  /// In zh, this message translates to:
  /// **'笔记、对话、知识库不会被删除。'**
  String get l6NotesConversationsAndKnowledgeArePreserved9b3e9;

  /// No description provided for @l6ClearAndPairAgain845d1.
  ///
  /// In zh, this message translates to:
  /// **'清除并重新配对'**
  String get l6ClearAndPairAgain845d1;

  /// No description provided for @l6DeviceName4433e.
  ///
  /// In zh, this message translates to:
  /// **'设备名称'**
  String get l6DeviceName4433e;

  /// No description provided for @l6PrepareThisDevice9d671.
  ///
  /// In zh, this message translates to:
  /// **'准备本机'**
  String get l6PrepareThisDevice9d671;

  /// No description provided for @l6PrepareThisDeviceForSyncee24c.
  ///
  /// In zh, this message translates to:
  /// **'准备本机加入同步？'**
  String get l6PrepareThisDeviceForSyncee24c;

  /// No description provided for @l6IUnderstandThatAuthorizedDevicesWill1e411.
  ///
  /// In zh, this message translates to:
  /// **'我理解：后续授权设备加入时，会共享所有 Provider 配置和 API Key。'**
  String get l6IUnderstandThatAuthorizedDevicesWill1e411;

  /// No description provided for @l6IUnderstandPrepareThisDevice9598b.
  ///
  /// In zh, this message translates to:
  /// **'我理解并准备'**
  String get l6IUnderstandPrepareThisDevice9598b;

  /// No description provided for @l6DeviceSyncIsDisabledInThise1b42.
  ///
  /// In zh, this message translates to:
  /// **'设备同步已在当前构建中关闭（编译开关 ELSEWHEN_SYNC_PREVIEW）。'**
  String get l6DeviceSyncIsDisabledInThise1b42;

  /// No description provided for @l6AfterPairingDeviceListsAndSync6d4f5.
  ///
  /// In zh, this message translates to:
  /// **'完成绑定后，这里会显示设备列表与同步管理。'**
  String get l6AfterPairingDeviceListsAndSync6d4f5;

  /// No description provided for @l6DeviceIdIsUnavailableRefreshAnd762d2.
  ///
  /// In zh, this message translates to:
  /// **'本机设备 ID 不可见，请刷新。'**
  String get l6DeviceIdIsUnavailableRefreshAnd762d2;

  /// No description provided for @l6AuthorizedDevicesAppearHereAfterThey67f15.
  ///
  /// In zh, this message translates to:
  /// **'授权其他设备后，这里会显示设备列表。'**
  String get l6AuthorizedDevicesAppearHereAfterThey67f15;

  /// No description provided for @l6LockDeviceKeys6ac4e.
  ///
  /// In zh, this message translates to:
  /// **'锁定本机密钥'**
  String get l6LockDeviceKeys6ac4e;

  /// No description provided for @l6UnlockDeviceKeysa84f8.
  ///
  /// In zh, this message translates to:
  /// **'解锁本机密钥'**
  String get l6UnlockDeviceKeysa84f8;

  /// No description provided for @l6RefreshStatus7cc7f.
  ///
  /// In zh, this message translates to:
  /// **'刷新状态'**
  String get l6RefreshStatus7cc7f;

  /// No description provided for @l6Diagnostics0b673.
  ///
  /// In zh, this message translates to:
  /// **'诊断信息'**
  String get l6Diagnostics0b673;

  /// No description provided for @l6DatasetId395bc.
  ///
  /// In zh, this message translates to:
  /// **'数据集标识'**
  String get l6DatasetId395bc;

  /// No description provided for @l6ConfirmDeviceEnrollment159c2.
  ///
  /// In zh, this message translates to:
  /// **'确认加入设备'**
  String get l6ConfirmDeviceEnrollment159c2;

  /// No description provided for @l6CompareThePublicKeyFingerprintWitha360c.
  ///
  /// In zh, this message translates to:
  /// **'请与新设备核对公钥指纹：'**
  String get l6CompareThePublicKeyFingerprintWitha360c;

  /// No description provided for @l6ConfirmThatYouTrustThisDevice20292.
  ///
  /// In zh, this message translates to:
  /// **'确认这台设备可信，并允许其获得全部 Provider 配置和 API Key。'**
  String get l6ConfirmThatYouTrustThisDevice20292;

  /// No description provided for @l6Back11d02.
  ///
  /// In zh, this message translates to:
  /// **'返回'**
  String get l6Back11d02;

  /// No description provided for @l6ApproveThisDeviced11af.
  ///
  /// In zh, this message translates to:
  /// **'批准这台设备'**
  String get l6ApproveThisDeviced11af;

  /// No description provided for @l6InviteADevice9f3fa.
  ///
  /// In zh, this message translates to:
  /// **'邀请设备'**
  String get l6InviteADevice9f3fa;

  /// No description provided for @l6NewDevicesCanRequestToJoinbe25a.
  ///
  /// In zh, this message translates to:
  /// **'新设备可通过“加入现有数据集”发起加入；批准后自动完成交接。'**
  String get l6NewDevicesCanRequestToJoinbe25a;

  /// No description provided for @l6YourRelayUrl9ff3a.
  ///
  /// In zh, this message translates to:
  /// **'你已部署的中继 URL'**
  String get l6YourRelayUrl9ff3a;

  /// No description provided for @l6IUnderstandThatCreatingAnInvitation5f3db.
  ///
  /// In zh, this message translates to:
  /// **'我理解：创建邀请会连接此中继；批准加入后将共享 Provider 配置和 API Key。'**
  String get l6IUnderstandThatCreatingAnInvitation5f3db;

  /// No description provided for @l6CreateATemporaryInvitationa2809.
  ///
  /// In zh, this message translates to:
  /// **'创建短期邀请'**
  String get l6CreateATemporaryInvitationa2809;

  /// No description provided for @l6InvitationCreatedShowTheQrCode15986.
  ///
  /// In zh, this message translates to:
  /// **'邀请已生成（出示二维码或复制邀请）'**
  String get l6InvitationCreatedShowTheQrCode15986;

  /// No description provided for @l6ANewDeviceHasRequestedTo3129b.
  ///
  /// In zh, this message translates to:
  /// **'新设备已发起加入请求'**
  String get l6ANewDeviceHasRequestedTo3129b;

  /// No description provided for @l6ApprovedDeliveringApprovalc4bcf.
  ///
  /// In zh, this message translates to:
  /// **'已批准，正在投递审批'**
  String get l6ApprovedDeliveringApprovalc4bcf;

  /// No description provided for @l6ApprovalDeliveredWaitingForTheNewaeaa7.
  ///
  /// In zh, this message translates to:
  /// **'审批已送达，等待新设备完成安装'**
  String get l6ApprovalDeliveredWaitingForTheNewaeaa7;

  /// No description provided for @l6NewDeviceJoinsCheckTheDevice29220.
  ///
  /// In zh, this message translates to:
  /// **'新设备接入（在「管理」确认设备列表）'**
  String get l6NewDeviceJoinsCheckTheDevice29220;

  /// No description provided for @l6PreparingTheInitialSnapshotb1613.
  ///
  /// In zh, this message translates to:
  /// **'正在准备初始快照'**
  String get l6PreparingTheInitialSnapshotb1613;

  /// No description provided for @l6WaitingForInitialSnapshotSelectione32c8.
  ///
  /// In zh, this message translates to:
  /// **'等待选择初始快照'**
  String get l6WaitingForInitialSnapshotSelectione32c8;

  /// No description provided for @l6SendingTheInitialSnapshotfa3fd.
  ///
  /// In zh, this message translates to:
  /// **'正在发送初始快照'**
  String get l6SendingTheInitialSnapshotfa3fd;

  /// No description provided for @l6ShareInvitationsOnlyWithTrustedDevices47059.
  ///
  /// In zh, this message translates to:
  /// **'邀请码仅交给可信设备。'**
  String get l6ShareInvitationsOnlyWithTrustedDevices47059;

  /// No description provided for @l6CopyInvitationf7293.
  ///
  /// In zh, this message translates to:
  /// **'复制邀请'**
  String get l6CopyInvitationf7293;

  /// No description provided for @l6CancelInvitation17e4c.
  ///
  /// In zh, this message translates to:
  /// **'取消邀请'**
  String get l6CancelInvitation17e4c;

  /// No description provided for @l6VerifyAndApprove5c161.
  ///
  /// In zh, this message translates to:
  /// **'核对并审批'**
  String get l6VerifyAndApprove5c161;

  /// No description provided for @l6ResendSavedApproval48def.
  ///
  /// In zh, this message translates to:
  /// **'重发已保存的审批'**
  String get l6ResendSavedApproval48def;

  /// No description provided for @l6ProviderSyncConflicts31e02.
  ///
  /// In zh, this message translates to:
  /// **'Provider 同步冲突'**
  String get l6ProviderSyncConflicts31e02;

  /// No description provided for @l6KeysAreMaskedKeepTheLocal2e55f.
  ///
  /// In zh, this message translates to:
  /// **'密钥已掩码。选择「保留本地」或「接受远端」整包应用，不做字段拼接；地址提示仅为静态分析，不代表实际可达。'**
  String get l6KeysAreMaskedKeepTheLocal2e55f;

  /// No description provided for @l6NoProviderSyncConflictsNeedReview4dc86.
  ///
  /// In zh, this message translates to:
  /// **'没有待审阅的 Provider 同步冲突。'**
  String get l6NoProviderSyncConflictsNeedReview4dc86;

  /// No description provided for @l6CurrentLocalConfiguration683a0.
  ///
  /// In zh, this message translates to:
  /// **'本机当前配置'**
  String get l6CurrentLocalConfiguration683a0;

  /// No description provided for @l6RemoteConfiguration04e8c.
  ///
  /// In zh, this message translates to:
  /// **'远端配置'**
  String get l6RemoteConfiguration04e8c;

  /// No description provided for @l6KeepLocal6d6a5.
  ///
  /// In zh, this message translates to:
  /// **'保留本地'**
  String get l6KeepLocal6d6a5;

  /// No description provided for @l6AcceptRemote4546a.
  ///
  /// In zh, this message translates to:
  /// **'接受远端'**
  String get l6AcceptRemote4546a;

  /// No description provided for @l6NoConfigurationa33dc.
  ///
  /// In zh, this message translates to:
  /// **'（无配置）'**
  String get l6NoConfigurationa33dc;

  /// No description provided for @l6KeyNotSetefd96.
  ///
  /// In zh, this message translates to:
  /// **'Key：未设置'**
  String get l6KeyNotSetefd96;

  /// No description provided for @l6LoopbackAddressReachableOnlyFromThis5a7ca.
  ///
  /// In zh, this message translates to:
  /// **'环回地址：仅本机可达，同步到其他设备后无法访问'**
  String get l6LoopbackAddressReachableOnlyFromThis5a7ca;

  /// No description provided for @l6PrivateNetworkAddressReachableOnlyOnf9ec8.
  ///
  /// In zh, this message translates to:
  /// **'内网地址：仅同一网络内设备可达'**
  String get l6PrivateNetworkAddressReachableOnlyOnf9ec8;

  /// No description provided for @l6MissingHttpSPrefixMostClients85171.
  ///
  /// In zh, this message translates to:
  /// **'缺少 http(s):// 前缀，多数客户端无法识别'**
  String get l6MissingHttpSPrefixMostClients85171;

  /// No description provided for @l6UnusualProtocolMayNotBeSupportedf9ae0.
  ///
  /// In zh, this message translates to:
  /// **'非常规协议，可能不被支持'**
  String get l6UnusualProtocolMayNotBeSupportedf9ae0;

  /// No description provided for @l6MissingHostNameb705c.
  ///
  /// In zh, this message translates to:
  /// **'缺少主机名'**
  String get l6MissingHostNameb705c;

  /// No description provided for @l6LastSyncSkippedLockedOrBackground113af.
  ///
  /// In zh, this message translates to:
  /// **'最近同步：本轮跳过（锁定或后台忙碌）'**
  String get l6LastSyncSkippedLockedOrBackground113af;

  /// No description provided for @l6RelayMailboxFullWaitingForThec480e.
  ///
  /// In zh, this message translates to:
  /// **'中继信箱已满，等对方取件'**
  String get l6RelayMailboxFullWaitingForThec480e;

  /// No description provided for @l6RelayMessageFormatMismatchUpdateThe1d3c3.
  ///
  /// In zh, this message translates to:
  /// **'与中继的消息格式不一致，需要更新版本'**
  String get l6RelayMessageFormatMismatchUpdateThe1d3c3;

  /// No description provided for @l6RepeatedContentValidationFailuresDeliveryPausedb04cc.
  ///
  /// In zh, this message translates to:
  /// **'内容校验反复失败，已暂停投递'**
  String get l6RepeatedContentValidationFailuresDeliveryPausedb04cc;

  /// No description provided for @l6NetworkIsUnstableRetrying2aaa3.
  ///
  /// In zh, this message translates to:
  /// **'网络不稳定，正在重试'**
  String get l6NetworkIsUnstableRetrying2aaa3;

  /// No description provided for @l6Resyncing0f8da.
  ///
  /// In zh, this message translates to:
  /// **'正在重新同步'**
  String get l6Resyncing0f8da;

  /// No description provided for @l6DeliveredToRelayWaitingForPeer2bf3e.
  ///
  /// In zh, this message translates to:
  /// **'已送达中继，等对方确认'**
  String get l6DeliveredToRelayWaitingForPeer2bf3e;

  /// No description provided for @l6Idle837e7.
  ///
  /// In zh, this message translates to:
  /// **'空闲'**
  String get l6Idle837e7;

  /// No description provided for @l6RetryingSoone5ae3.
  ///
  /// In zh, this message translates to:
  /// **'即将重试'**
  String get l6RetryingSoone5ae3;

  /// No description provided for @l6DeleteEmotionAnnotationa51b5.
  ///
  /// In zh, this message translates to:
  /// **'删除情绪标注'**
  String get l6DeleteEmotionAnnotationa51b5;

  /// No description provided for @l6DeleteRelationship16bbd.
  ///
  /// In zh, this message translates to:
  /// **'删除关系'**
  String get l6DeleteRelationship16bbd;

  /// No description provided for @l6NoRelationshipsOrEmotionAnnotationsYetbecb5.
  ///
  /// In zh, this message translates to:
  /// **'还没有积累。对话或快录里提到的人物与项目，经你确认（或 @/# 标注的权威组合）后会在这里长成一份可查可删的关系账本；有客观情绪反应的记录会给出简短文字标注（无数值、不是情绪分析报告）。删掉任何一条都不会动原始记录。'**
  String get l6NoRelationshipsOrEmotionAnnotationsYetbecb5;

  /// No description provided for @l6LastArg0187f4.
  ///
  /// In zh, this message translates to:
  /// **'最近 {arg0}'**
  String l6LastArg0187f4(String arg0);

  /// No description provided for @l6DeleteThisRelationshipPreserveOriginalRecord6cc81.
  ///
  /// In zh, this message translates to:
  /// **'删除这条关系（原始记录不动）'**
  String get l6DeleteThisRelationshipPreserveOriginalRecord6cc81;

  /// No description provided for @l6EmotionAnnotationsab8ab.
  ///
  /// In zh, this message translates to:
  /// **'情绪标注'**
  String get l6EmotionAnnotationsab8ab;

  /// No description provided for @l6AboutThisEventbc042.
  ///
  /// In zh, this message translates to:
  /// **'对这件事'**
  String get l6AboutThisEventbc042;

  /// No description provided for @l6AboutArg0171b0.
  ///
  /// In zh, this message translates to:
  /// **'对 {arg0}'**
  String l6AboutArg0171b0(String arg0);

  /// No description provided for @l6DeleteThisEmotionAnnotationPreserveOriginal8d148.
  ///
  /// In zh, this message translates to:
  /// **'删除这条情绪标注（原始记录不动）'**
  String get l6DeleteThisEmotionAnnotationPreserveOriginal8d148;

  /// No description provided for @l6OtherDimensionsCommitmentsReflectionPaceBoundaries873e8.
  ///
  /// In zh, this message translates to:
  /// **'其余维度（承诺、复盘、节奏、偏好红线、健康、财务、地点、身份、创作线）以知识库页面承载，在知识库中查看与编辑。'**
  String get l6OtherDimensionsCommitmentsReflectionPaceBoundaries873e8;

  /// No description provided for @l6EventRemindersaa957.
  ///
  /// In zh, this message translates to:
  /// **'事件提醒'**
  String get l6EventRemindersaa957;

  /// No description provided for @l6ImmediateAndInReviewsd066b.
  ///
  /// In zh, this message translates to:
  /// **'即时+回顾'**
  String get l6ImmediateAndInReviewsd066b;

  /// No description provided for @l6ReviewsOnly71ead.
  ///
  /// In zh, this message translates to:
  /// **'仅回顾'**
  String get l6ReviewsOnly71ead;

  /// No description provided for @l6SaveReminderPreferencese0188.
  ///
  /// In zh, this message translates to:
  /// **'保存提醒偏好'**
  String get l6SaveReminderPreferencese0188;

  /// No description provided for @l6EmotionalCheckIns74925.
  ///
  /// In zh, this message translates to:
  /// **'情感回访'**
  String get l6EmotionalCheckIns74925;

  /// No description provided for @l63Daysb30a9.
  ///
  /// In zh, this message translates to:
  /// **'3天'**
  String get l63Daysb30a9;

  /// No description provided for @l67Daysecfe0.
  ///
  /// In zh, this message translates to:
  /// **'7天'**
  String get l67Daysecfe0;

  /// No description provided for @l614Days1e982.
  ///
  /// In zh, this message translates to:
  /// **'14天'**
  String get l614Days1e982;

  /// No description provided for @l630Daysff788.
  ///
  /// In zh, this message translates to:
  /// **'30天'**
  String get l630Daysff788;

  /// No description provided for @l6SaveRevisitPreferences8123a.
  ///
  /// In zh, this message translates to:
  /// **'保存回访偏好'**
  String get l6SaveRevisitPreferences8123a;

  /// No description provided for @l6RecordReminderFeedbackcfbf1.
  ///
  /// In zh, this message translates to:
  /// **'记录提醒反馈'**
  String get l6RecordReminderFeedbackcfbf1;

  /// No description provided for @l6Assessingd7b92.
  ///
  /// In zh, this message translates to:
  /// **'判定中'**
  String get l6Assessingd7b92;

  /// No description provided for @l6FollowedInConversationa3d02.
  ///
  /// In zh, this message translates to:
  /// **'已在对话遵守'**
  String get l6FollowedInConversationa3d02;

  /// No description provided for @l6NotApplicableeec46.
  ///
  /// In zh, this message translates to:
  /// **'不适用'**
  String get l6NotApplicableeec46;

  /// No description provided for @l6RuleContextsInConversationsEvaluatedSilently95c0c.
  ///
  /// In zh, this message translates to:
  /// **'对话中的规则场景（后台静默判定，不打扰对话）'**
  String get l6RuleContextsInConversationsEvaluatedSilently95c0c;

  /// No description provided for @l6RecentReminderse3fba.
  ///
  /// In zh, this message translates to:
  /// **'最近提醒'**
  String get l6RecentReminderse3fba;

  /// No description provided for @l6HelpfulArg0NotRelevantArg18e65c.
  ///
  /// In zh, this message translates to:
  /// **'有用 {arg0} · 不相关 {arg1}'**
  String l6HelpfulArg0NotRelevantArg18e65c(String arg0, String arg1);

  /// No description provided for @l6TriggerConditionsf5c95.
  ///
  /// In zh, this message translates to:
  /// **'触发条件'**
  String get l6TriggerConditionsf5c95;

  /// No description provided for @l6TriggerKeywordsCommaSeparatedBlankDisablesdae9a.
  ///
  /// In zh, this message translates to:
  /// **'触发关键词（逗号分隔，留空 = 不预筛）'**
  String get l6TriggerKeywordsCommaSeparatedBlankDisablesdae9a;

  /// No description provided for @l6ForExampleProjectXDeadlinef216a.
  ///
  /// In zh, this message translates to:
  /// **'如：X项目, deadline'**
  String get l6ForExampleProjectXDeadlinef216a;

  /// No description provided for @l6TriggerContext02570.
  ///
  /// In zh, this message translates to:
  /// **'触发场景'**
  String get l6TriggerContext02570;

  /// No description provided for @l6AllContexts39fdd.
  ///
  /// In zh, this message translates to:
  /// **'全场景'**
  String get l6AllContexts39fdd;

  /// No description provided for @l6QuickCapturecfe6c.
  ///
  /// In zh, this message translates to:
  /// **'快速捕获'**
  String get l6QuickCapturecfe6c;

  /// No description provided for @l6DailyReview6bea3.
  ///
  /// In zh, this message translates to:
  /// **'每日回顾'**
  String get l6DailyReview6bea3;

  /// No description provided for @l6SaveTriggerConditions8e04c.
  ///
  /// In zh, this message translates to:
  /// **'保存触发条件'**
  String get l6SaveTriggerConditions8e04c;

  /// No description provided for @l6DeleteRulef9ad3.
  ///
  /// In zh, this message translates to:
  /// **'删除规则'**
  String get l6DeleteRulef9ad3;

  /// No description provided for @l6PersonalRules03b44.
  ///
  /// In zh, this message translates to:
  /// **'个人规则库'**
  String get l6PersonalRules03b44;

  /// No description provided for @l6UserModel88c56.
  ///
  /// In zh, this message translates to:
  /// **'用户模型'**
  String get l6UserModel88c56;

  /// No description provided for @l6NoRulesYetWhenYouDiscuss96049.
  ///
  /// In zh, this message translates to:
  /// **'还没有规则。在对话里分享踩坑或心得时，AI 会建议把其中的经验沉淀成规则，你在审阅卡接受具体规则后就会出现在这里，以后遇到类似情况 AI 会主动引用并提醒你。'**
  String get l6NoRulesYetWhenYouDiscuss96049;

  /// No description provided for @l6TriggerConditionsConfigureda3704.
  ///
  /// In zh, this message translates to:
  /// **'触发条件（已配置）'**
  String get l6TriggerConditionsConfigureda3704;

  /// No description provided for @l6ConfigureTriggerConditions168ff.
  ///
  /// In zh, this message translates to:
  /// **'配置触发条件'**
  String get l6ConfigureTriggerConditions168ff;

  /// No description provided for @l6PlaceEachSkillFolderWithIts94f37.
  ///
  /// In zh, this message translates to:
  /// **'把技能目录放进 <数据目录>/skills（每个技能一个子目录，含 SKILL.md），对话中由模型按需加载；脚本执行前会单独向你确认。把某个技能标为「信任」后，它的脚本调用直接执行、不再审批——脚本可在本机执行任意操作，开启即视为你了解并自担风险。'**
  String get l6PlaceEachSkillFolderWithIts94f37;

  /// No description provided for @l6InstallFromFolder22a87.
  ///
  /// In zh, this message translates to:
  /// **'从文件夹安装'**
  String get l6InstallFromFolder22a87;

  /// No description provided for @l6InstallFromZip906f1.
  ///
  /// In zh, this message translates to:
  /// **'从 zip 安装'**
  String get l6InstallFromZip906f1;

  /// No description provided for @l6InstallFromGit0882e.
  ///
  /// In zh, this message translates to:
  /// **'从 Git 安装'**
  String get l6InstallFromGit0882e;

  /// No description provided for @l6OpenSkillsFoldera4437.
  ///
  /// In zh, this message translates to:
  /// **'打开技能目录'**
  String get l6OpenSkillsFoldera4437;

  /// No description provided for @l6SelectedArg0Arg1fd75c.
  ///
  /// In zh, this message translates to:
  /// **'已选 {arg0} / {arg1}'**
  String l6SelectedArg0Arg1fd75c(String arg0, String arg1);

  /// No description provided for @l6TotalArg01e668.
  ///
  /// In zh, this message translates to:
  /// **'共 {arg0} 个'**
  String l6TotalArg01e668(String arg0);

  /// No description provided for @l6ExitSelectione23ed.
  ///
  /// In zh, this message translates to:
  /// **'退出选择'**
  String get l6ExitSelectione23ed;

  /// No description provided for @l6DeleteMultiple362ae.
  ///
  /// In zh, this message translates to:
  /// **'批量删除'**
  String get l6DeleteMultiple362ae;

  /// No description provided for @l6DeselectAllba20e.
  ///
  /// In zh, this message translates to:
  /// **'全不选'**
  String get l6DeselectAllba20e;

  /// No description provided for @l6SelectAll3e44b.
  ///
  /// In zh, this message translates to:
  /// **'全选'**
  String get l6SelectAll3e44b;

  /// No description provided for @l6DeleteSelectedArg0def95.
  ///
  /// In zh, this message translates to:
  /// **'删除所选 ({arg0})'**
  String l6DeleteSelectedArg0def95(String arg0);

  /// No description provided for @l6LoadingSkills8d716.
  ///
  /// In zh, this message translates to:
  /// **'正在读取技能列表…'**
  String get l6LoadingSkills8d716;

  /// No description provided for @l6NoUserSkillsInstalledInstallAe8aea.
  ///
  /// In zh, this message translates to:
  /// **'尚未安装任何用户技能\n从文件夹 / zip / Git 仓库安装第一个技能'**
  String get l6NoUserSkillsInstalledInstallAe8aea;

  /// No description provided for @l6LoadUserSkillsfb79f.
  ///
  /// In zh, this message translates to:
  /// **'读取用户技能'**
  String get l6LoadUserSkillsfb79f;

  /// No description provided for @l6ChooseASkillZipFile0e485.
  ///
  /// In zh, this message translates to:
  /// **'选择技能 zip 包'**
  String get l6ChooseASkillZipFile0e485;

  /// No description provided for @l6ChooseASkillFolderContainingSkill8789f.
  ///
  /// In zh, this message translates to:
  /// **'选择包含 SKILL.md 的技能目录'**
  String get l6ChooseASkillFolderContainingSkill8789f;

  /// No description provided for @l6InstallFromZip37d9a.
  ///
  /// In zh, this message translates to:
  /// **'zip 安装'**
  String get l6InstallFromZip37d9a;

  /// No description provided for @l6InstallFromFolder71cd7.
  ///
  /// In zh, this message translates to:
  /// **'目录安装'**
  String get l6InstallFromFolder71cd7;

  /// No description provided for @l6InstallASkillFromGitfa921.
  ///
  /// In zh, this message translates to:
  /// **'从 Git 仓库安装技能'**
  String get l6InstallASkillFromGitfa921;

  /// No description provided for @l6RepositoryUrlHttpsOrLocalPath3ae69.
  ///
  /// In zh, this message translates to:
  /// **'仓库地址（https 或本地路径）'**
  String get l6RepositoryUrlHttpsOrLocalPath3ae69;

  /// No description provided for @l6Install087db.
  ///
  /// In zh, this message translates to:
  /// **'安装'**
  String get l6Install087db;

  /// No description provided for @l6InstallFromGit6d266.
  ///
  /// In zh, this message translates to:
  /// **'Git 安装'**
  String get l6InstallFromGit6d266;

  /// No description provided for @l6InstalledArg0EnabledByDefaultdd2c2.
  ///
  /// In zh, this message translates to:
  /// **'已安装 {arg0}，默认启用'**
  String l6InstalledArg0EnabledByDefaultdd2c2(String arg0);

  /// No description provided for @l6UninstallSkill053a6.
  ///
  /// In zh, this message translates to:
  /// **'卸载技能'**
  String get l6UninstallSkill053a6;

  /// No description provided for @l6Arg0WillBeRemovedFromThis1a7c6.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 将从本机删除；若已同步，其他设备也会一并删除。'**
  String l6Arg0WillBeRemovedFromThis1a7c6(String arg0);

  /// No description provided for @l6Uninstall200cf.
  ///
  /// In zh, this message translates to:
  /// **'卸载'**
  String get l6Uninstall200cf;

  /// No description provided for @l6UninstallMultipleSkills13309.
  ///
  /// In zh, this message translates to:
  /// **'批量卸载技能'**
  String get l6UninstallMultipleSkills13309;

  /// No description provided for @l6SkillsToRemoveArg0SyncedCopiesaa41a.
  ///
  /// In zh, this message translates to:
  /// **'共 {arg0} 个技能将从本机删除；若已同步，其他设备也会一并删除。'**
  String l6SkillsToRemoveArg0SyncedCopiesaa41a(String arg0);

  /// No description provided for @l6UninstallSkillArg0a3819.
  ///
  /// In zh, this message translates to:
  /// **'卸载技能 {arg0}'**
  String l6UninstallSkillArg0a3819(String arg0);

  /// No description provided for @l6SkillsUninstalledArg08db74.
  ///
  /// In zh, this message translates to:
  /// **'已卸载 {arg0} 个技能'**
  String l6SkillsUninstalledArg08db74(String arg0);

  /// No description provided for @l6SucceededArg0FailedArg1Arg2Arg3a2cde.
  ///
  /// In zh, this message translates to:
  /// **'成功 {arg0} 个，失败 {arg1} 个：{arg2}{arg3}'**
  String l6SucceededArg0FailedArg1Arg2Arg3a2cde(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  );

  /// No description provided for @l6ToggleUserSkill2de88.
  ///
  /// In zh, this message translates to:
  /// **'切换用户技能'**
  String get l6ToggleUserSkill2de88;

  /// No description provided for @l6MarkedAsTrustedThisSkillS1cf36.
  ///
  /// In zh, this message translates to:
  /// **'已标为信任：该技能的脚本调用不再弹审批卡'**
  String get l6MarkedAsTrustedThisSkillS1cf36;

  /// No description provided for @l6ChangeTrustSetting7ce93.
  ///
  /// In zh, this message translates to:
  /// **'切换信任状态'**
  String get l6ChangeTrustSetting7ce93;

  /// No description provided for @l6KnowledgeExport26466.
  ///
  /// In zh, this message translates to:
  /// **'知识导出'**
  String get l6KnowledgeExport26466;

  /// No description provided for @l6DatabaseMaintenance60286.
  ///
  /// In zh, this message translates to:
  /// **'数据库维护'**
  String get l6DatabaseMaintenance60286;

  /// No description provided for @l6DailyTokenUsage5a651.
  ///
  /// In zh, this message translates to:
  /// **'每日 Token 使用'**
  String get l6DailyTokenUsage5a651;

  /// No description provided for @l6TrustedNoApproval69302.
  ///
  /// In zh, this message translates to:
  /// **'信任·免审批'**
  String get l6TrustedNoApproval69302;

  /// No description provided for @l6RequireApprovalEachTime673dd.
  ///
  /// In zh, this message translates to:
  /// **'改为逐次审批'**
  String get l6RequireApprovalEachTime673dd;

  /// No description provided for @l6TrustNoApproval12269.
  ///
  /// In zh, this message translates to:
  /// **'标为信任（免审批）'**
  String get l6TrustNoApproval12269;

  /// No description provided for @l6EnabledAPasswordIsRequiredWhen1aee3.
  ///
  /// In zh, this message translates to:
  /// **'已开启：启动 App 时需要输入密码'**
  String get l6EnabledAPasswordIsRequiredWhen1aee3;

  /// No description provided for @l6DisabledTheAppOpensDirectlycbfe4.
  ///
  /// In zh, this message translates to:
  /// **'未开启：启动 App 直接进入主界面'**
  String get l6DisabledTheAppOpensDirectlycbfe4;

  /// No description provided for @l6SetAnAppPassword72f45.
  ///
  /// In zh, this message translates to:
  /// **'设置打开密码'**
  String get l6SetAnAppPassword72f45;

  /// No description provided for @l6ChangePassword0ae20.
  ///
  /// In zh, this message translates to:
  /// **'修改密码'**
  String get l6ChangePassword0ae20;

  /// No description provided for @l6TurnOffPassword7f7cb.
  ///
  /// In zh, this message translates to:
  /// **'关闭密码'**
  String get l6TurnOffPassword7f7cb;

  /// No description provided for @l6OnlyASaltedPasswordHashIsd5644.
  ///
  /// In zh, this message translates to:
  /// **'密码只以加盐哈希形式保存在本机系统安全存储，不会上传、不会随同步带走。遗忘密码后无法找回，只能清除应用数据重置。'**
  String get l6OnlyASaltedPasswordHashIsd5644;

  /// No description provided for @l6FingerprintOrFaceUnlock1b96c.
  ///
  /// In zh, this message translates to:
  /// **'指纹/面容解锁'**
  String get l6FingerprintOrFaceUnlock1b96c;

  /// No description provided for @l6UseSystemBiometricsForQuickUnlocking22994.
  ///
  /// In zh, this message translates to:
  /// **'开启后可用系统生物验证快速解锁，密码始终保留为兜底'**
  String get l6UseSystemBiometricsForQuickUnlocking22994;

  /// No description provided for @l6ThisDeviceDoesNotSupportBiometrics40ff0.
  ///
  /// In zh, this message translates to:
  /// **'当前设备不支持或未录入指纹/面容'**
  String get l6ThisDeviceDoesNotSupportBiometrics40ff0;

  /// No description provided for @l6EnableFingerprintOrFaceUnlockb2a8f.
  ///
  /// In zh, this message translates to:
  /// **'开启指纹/面容解锁'**
  String get l6EnableFingerprintOrFaceUnlockb2a8f;

  /// No description provided for @l6TheLockScreenWillFirstOffer49b2b.
  ///
  /// In zh, this message translates to:
  /// **'开启后锁屏页会优先调起系统指纹/面容验证。请输入当前打开密码确认。'**
  String get l6TheLockScreenWillFirstOffer49b2b;

  /// No description provided for @l6CurrentPassword33abf.
  ///
  /// In zh, this message translates to:
  /// **'当前密码'**
  String get l6CurrentPassword33abf;

  /// No description provided for @l6ConfirmEnable263d4.
  ///
  /// In zh, this message translates to:
  /// **'确认开启'**
  String get l6ConfirmEnable263d4;

  /// No description provided for @l6IncorrectCurrentPasswordNotEnabled691f1.
  ///
  /// In zh, this message translates to:
  /// **'当前密码不正确，未开启'**
  String get l6IncorrectCurrentPasswordNotEnabled691f1;

  /// No description provided for @l6BiometricUnlockEnablede15eb.
  ///
  /// In zh, this message translates to:
  /// **'指纹/面容解锁已开启'**
  String get l6BiometricUnlockEnablede15eb;

  /// No description provided for @l6BiometricsAreCurrentlyUnavailableNotEnabled78c56.
  ///
  /// In zh, this message translates to:
  /// **'设备当前不可用生物验证，未开启'**
  String get l6BiometricsAreCurrentlyUnavailableNotEnabled78c56;

  /// No description provided for @l6AppPasswordSeta5390.
  ///
  /// In zh, this message translates to:
  /// **'打开密码已设置'**
  String get l6AppPasswordSeta5390;

  /// No description provided for @l6ChangeAppPassword25e9a.
  ///
  /// In zh, this message translates to:
  /// **'修改打开密码'**
  String get l6ChangeAppPassword25e9a;

  /// No description provided for @l6AppPasswordChanged0493a.
  ///
  /// In zh, this message translates to:
  /// **'打开密码已修改'**
  String get l6AppPasswordChanged0493a;

  /// No description provided for @l6TurnOffAppPassworda80d7.
  ///
  /// In zh, this message translates to:
  /// **'关闭打开密码'**
  String get l6TurnOffAppPassworda80d7;

  /// No description provided for @l6TheAppWillNoLongerRequire00cc0.
  ///
  /// In zh, this message translates to:
  /// **'关闭后启动 App 将不再要求输入密码。请输入当前密码确认。'**
  String get l6TheAppWillNoLongerRequire00cc0;

  /// No description provided for @l6ConfirmDisable059b4.
  ///
  /// In zh, this message translates to:
  /// **'确认关闭'**
  String get l6ConfirmDisable059b4;

  /// No description provided for @l6AppPasswordTurnedOff9f4c1.
  ///
  /// In zh, this message translates to:
  /// **'打开密码已关闭'**
  String get l6AppPasswordTurnedOff9f4c1;

  /// No description provided for @l6IncorrectCurrentPasswordNotDisabled0660c.
  ///
  /// In zh, this message translates to:
  /// **'当前密码不正确，未关闭'**
  String get l6IncorrectCurrentPasswordNotDisabled0660c;

  /// No description provided for @l6EnterTheCurrentPassword4b0b0.
  ///
  /// In zh, this message translates to:
  /// **'请输入当前密码'**
  String get l6EnterTheCurrentPassword4b0b0;

  /// No description provided for @l6EnterANewPassword689e5.
  ///
  /// In zh, this message translates to:
  /// **'新密码不能为空'**
  String get l6EnterANewPassword689e5;

  /// No description provided for @l6TheAppPasswordMustHave41cb25.
  ///
  /// In zh, this message translates to:
  /// **'打开密码固定为 4 位'**
  String get l6TheAppPasswordMustHave41cb25;

  /// No description provided for @l6TheNewPasswordsDoNotMatchd0b9c.
  ///
  /// In zh, this message translates to:
  /// **'两次输入的新密码不一致'**
  String get l6TheNewPasswordsDoNotMatchd0b9c;

  /// No description provided for @l6TheCurrentPasswordIsIncorrectb0dab.
  ///
  /// In zh, this message translates to:
  /// **'当前密码不正确'**
  String get l6TheCurrentPasswordIsIncorrectb0dab;

  /// No description provided for @l6CouldNotSaveTryAgainae0b2.
  ///
  /// In zh, this message translates to:
  /// **'设置失败，请重试'**
  String get l6CouldNotSaveTryAgainae0b2;

  /// No description provided for @l6NewPassword4Digits51018.
  ///
  /// In zh, this message translates to:
  /// **'新密码（4 位）'**
  String get l6NewPassword4Digits51018;

  /// No description provided for @l6ConfirmNewPassworde0327.
  ///
  /// In zh, this message translates to:
  /// **'再次输入新密码'**
  String get l6ConfirmNewPassworde0327;

  /// No description provided for @l6Confirmf526c.
  ///
  /// In zh, this message translates to:
  /// **'确定'**
  String get l6Confirmf526c;

  /// No description provided for @l6ExportKnowledge863fb.
  ///
  /// In zh, this message translates to:
  /// **'导出知识库'**
  String get l6ExportKnowledge863fb;

  /// No description provided for @l6LoadDatabaseSize69850.
  ///
  /// In zh, this message translates to:
  /// **'读取库体量'**
  String get l6LoadDatabaseSize69850;

  /// No description provided for @l6OptimizeDatabase12519.
  ///
  /// In zh, this message translates to:
  /// **'整理数据库'**
  String get l6OptimizeDatabase12519;

  /// No description provided for @l6FetchingServicebfb3a.
  ///
  /// In zh, this message translates to:
  /// **'抓取服务'**
  String get l6FetchingServicebfb3a;

  /// No description provided for @l6PostFetchingServiceUpdated1c8a1.
  ///
  /// In zh, this message translates to:
  /// **'推文抓取服务已更新'**
  String get l6PostFetchingServiceUpdated1c8a1;

  /// No description provided for @l6SavePostFetchingService23d23.
  ///
  /// In zh, this message translates to:
  /// **'保存推文抓取服务'**
  String get l6SavePostFetchingService23d23;

  /// No description provided for @l6ChooseASecretReference6a79c.
  ///
  /// In zh, this message translates to:
  /// **'选择密钥引用'**
  String get l6ChooseASecretReference6a79c;

  /// No description provided for @l6AddASecretInSecuritySecretb6b03.
  ///
  /// In zh, this message translates to:
  /// **'请先在安全 → 密钥管理中添加密钥'**
  String get l6AddASecretInSecuritySecretb6b03;

  /// No description provided for @l6PostFetching22fc8.
  ///
  /// In zh, this message translates to:
  /// **'推文抓取'**
  String get l6PostFetching22fc8;

  /// No description provided for @l6WebSearch678ed.
  ///
  /// In zh, this message translates to:
  /// **'外网搜索'**
  String get l6WebSearch678ed;

  /// No description provided for @l6OutboundProxyabb5e.
  ///
  /// In zh, this message translates to:
  /// **'出站代理'**
  String get l6OutboundProxyabb5e;

  /// No description provided for @l6EncryptedDnsDohc262a.
  ///
  /// In zh, this message translates to:
  /// **'DoH 加密 DNS'**
  String get l6EncryptedDnsDohc262a;

  /// No description provided for @l6UserSkills748e7.
  ///
  /// In zh, this message translates to:
  /// **'用户技能'**
  String get l6UserSkills748e7;

  /// No description provided for @l6McpServersad57a.
  ///
  /// In zh, this message translates to:
  /// **'MCP 服务器'**
  String get l6McpServersad57a;

  /// No description provided for @l6ConnectModelContextProtocolServersTo6e5c7.
  ///
  /// In zh, this message translates to:
  /// **'连接 Model Context Protocol 服务器，把外部工具带入对话。工具以 mcp__<服务器>__<工具名> 出现，模型按需调用；stdio 服务器以指定命令作为子进程启动（首期只支持 stdio 传输）。'**
  String get l6ConnectModelContextProtocolServersTo6e5c7;

  /// No description provided for @l6EachToolCallRequiresApprovalBycbb3e.
  ///
  /// In zh, this message translates to:
  /// **'默认每次工具调用都会弹出审批卡逐次确认。把某个服务器标为「信任」后，它的调用将直接执行、不再审批——MCP 服务器是第三方代码，模型可能被诱导发起破坏性调用，开启即视为你了解并自担风险。若只想豁免确定只读的查询，可开每台服务器的「只读免审批」（仅对上游标了 readOnlyHint 的工具生效，其余照旧审批）。'**
  String get l6EachToolCallRequiresApprovalBycbb3e;

  /// No description provided for @l6EnabledArg0Arg1d02a1.
  ///
  /// In zh, this message translates to:
  /// **'已启用 {arg0} / {arg1}'**
  String l6EnabledArg0Arg1d02a1(String arg0, String arg1);

  /// No description provided for @l6AddServer35085.
  ///
  /// In zh, this message translates to:
  /// **'添加服务器'**
  String get l6AddServer35085;

  /// No description provided for @l6LoadingServers3023e.
  ///
  /// In zh, this message translates to:
  /// **'正在读取服务器列表…'**
  String get l6LoadingServers3023e;

  /// No description provided for @l6NoMcpServersAddedChooseAddac3f0.
  ///
  /// In zh, this message translates to:
  /// **'尚未添加任何 MCP 服务器\n点右上角「添加服务器」接入第一个外部工具源'**
  String get l6NoMcpServersAddedChooseAddac3f0;

  /// No description provided for @l6LoadMcpServers1900c.
  ///
  /// In zh, this message translates to:
  /// **'读取 MCP 服务器'**
  String get l6LoadMcpServers1900c;

  /// No description provided for @l6MarkedAsTrustedThisServerS74676.
  ///
  /// In zh, this message translates to:
  /// **'已标为信任：该服务器的工具调用不再弹审批卡'**
  String get l6MarkedAsTrustedThisServerS74676;

  /// No description provided for @l6ReadOnlyExemptionEnabledForTools0d43a.
  ///
  /// In zh, this message translates to:
  /// **'已开只读免审批：仅标了 readOnlyHint 的工具不再弹审批卡；仍以上游标注为准，写类工具照旧逐次审批'**
  String get l6ReadOnlyExemptionEnabledForTools0d43a;

  /// No description provided for @l6ChangeReadOnlyApprovalExemptione8168.
  ///
  /// In zh, this message translates to:
  /// **'切换只读免审批'**
  String get l6ChangeReadOnlyApprovalExemptione8168;

  /// No description provided for @l6ToggleMcpServer14dd0.
  ///
  /// In zh, this message translates to:
  /// **'切换 MCP 服务器'**
  String get l6ToggleMcpServer14dd0;

  /// No description provided for @l6DeleteMultipleServers13274.
  ///
  /// In zh, this message translates to:
  /// **'批量删除服务器'**
  String get l6DeleteMultipleServers13274;

  /// No description provided for @l6ServersToRemoveArg0SyncedCopiesccae5.
  ///
  /// In zh, this message translates to:
  /// **'共 {arg0} 台服务器将从本机删除；若已同步，其他设备也会一并删除。'**
  String l6ServersToRemoveArg0SyncedCopiesccae5(String arg0);

  /// No description provided for @l6DeleteMcpServerArg0f17d5.
  ///
  /// In zh, this message translates to:
  /// **'删除 MCP 服务器 {arg0}'**
  String l6DeleteMcpServerArg0f17d5(String arg0);

  /// No description provided for @l6ServersDeletedArg05fc2a.
  ///
  /// In zh, this message translates to:
  /// **'已删除 {arg0} 台服务器'**
  String l6ServersDeletedArg05fc2a(String arg0);

  /// No description provided for @l6SucceededArg0FailedArg1Arg2Arg312d85.
  ///
  /// In zh, this message translates to:
  /// **'成功 {arg0} 台，失败 {arg1} 台：{arg2}{arg3}'**
  String l6SucceededArg0FailedArg1Arg2Arg312d85(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  );

  /// No description provided for @l6DeleteMcpServer34624.
  ///
  /// In zh, this message translates to:
  /// **'删除 MCP 服务器'**
  String get l6DeleteMcpServer34624;

  /// No description provided for @l6AddMcpServerc9d22.
  ///
  /// In zh, this message translates to:
  /// **'添加 MCP 服务器'**
  String get l6AddMcpServerc9d22;

  /// No description provided for @l6EditMcpServer4c8df.
  ///
  /// In zh, this message translates to:
  /// **'编辑 MCP 服务器'**
  String get l6EditMcpServer4c8df;

  /// No description provided for @l6NameLettersDigitsHyphensOrUnderscores0dc18.
  ///
  /// In zh, this message translates to:
  /// **'名称（字母/数字/短横线/下划线）'**
  String get l6NameLettersDigitsHyphensOrUnderscores0dc18;

  /// No description provided for @l6RelatedIntentKeywordsOptionalb6b1f.
  ///
  /// In zh, this message translates to:
  /// **'关联意图关键词（可选）'**
  String get l6RelatedIntentKeywordsOptionalb6b1f;

  /// No description provided for @l6ForExampleProductReviewsRecommendationsSales606d3.
  ///
  /// In zh, this message translates to:
  /// **'小红书, 种草, 卖货'**
  String get l6ForExampleProductReviewsRecommendationsSales606d3;

  /// No description provided for @l6SeparateKeywordsWithCommasToHelp6c10b.
  ///
  /// In zh, this message translates to:
  /// **'用逗号分隔，帮助 AI 在相关任务中优先选择此工具（最多 256 字）'**
  String get l6SeparateKeywordsWithCommasToHelp6c10b;

  /// No description provided for @l6LocalCommandaa556.
  ///
  /// In zh, this message translates to:
  /// **'本机命令'**
  String get l6LocalCommandaa556;

  /// No description provided for @l6RemoteServer1b481.
  ///
  /// In zh, this message translates to:
  /// **'远程服务器'**
  String get l6RemoteServer1b481;

  /// No description provided for @l6Commandb114b.
  ///
  /// In zh, this message translates to:
  /// **'命令'**
  String get l6Commandb114b;

  /// No description provided for @l6ArgumentsJsonArray5ffa9.
  ///
  /// In zh, this message translates to:
  /// **'参数（JSON 数组）'**
  String get l6ArgumentsJsonArray5ffa9;

  /// No description provided for @l6InsertASecretReference6edc7.
  ///
  /// In zh, this message translates to:
  /// **'插入密钥引用'**
  String get l6InsertASecretReference6edc7;

  /// No description provided for @l6EnvironmentVariablesJsonObject364cf.
  ///
  /// In zh, this message translates to:
  /// **'环境变量（JSON 对象）'**
  String get l6EnvironmentVariablesJsonObject364cf;

  /// No description provided for @l6EndpointUrl28548.
  ///
  /// In zh, this message translates to:
  /// **'端点 URL'**
  String get l6EndpointUrl28548;

  /// No description provided for @l6CustomHeadersJsonObjectAuthenticationEtcd9233.
  ///
  /// In zh, this message translates to:
  /// **'自定义 Header（JSON 对象，鉴权等）'**
  String get l6CustomHeadersJsonObjectAuthenticationEtcd9233;

  /// No description provided for @l6TrustedCallsSkipApproval6254b.
  ///
  /// In zh, this message translates to:
  /// **'信任（调用免审批）'**
  String get l6TrustedCallsSkipApproval6254b;

  /// No description provided for @l6ToolsFromThisServerExecuteWithoutf61a4.
  ///
  /// In zh, this message translates to:
  /// **'开启后该服务器的工具调用直接执行，不再逐次弹审批卡。MCP 服务器是第三方代码，请只对你信任的服务器开启。'**
  String get l6ToolsFromThisServerExecuteWithoutf61a4;

  /// No description provided for @l6TestConnection10b7d.
  ///
  /// In zh, this message translates to:
  /// **'测试连接'**
  String get l6TestConnection10b7d;

  /// No description provided for @l6SaveMcpServer25c8a.
  ///
  /// In zh, this message translates to:
  /// **'保存 MCP 服务器'**
  String get l6SaveMcpServer25c8a;

  /// No description provided for @l6EnterACommandBeforeTesting41aae.
  ///
  /// In zh, this message translates to:
  /// **'先填命令再测试'**
  String get l6EnterACommandBeforeTesting41aae;

  /// No description provided for @l6EnterAnHttpSEndpointBeforec28cd.
  ///
  /// In zh, this message translates to:
  /// **'先填 http(s) 端点 URL 再测试'**
  String get l6EnterAnHttpSEndpointBeforec28cd;

  /// No description provided for @l6ConnectedButTheServerProvidedNod0d33.
  ///
  /// In zh, this message translates to:
  /// **'连接成功，但该服务器未提供工具'**
  String get l6ConnectedButTheServerProvidedNod0d33;

  /// No description provided for @l6ConnectedArg0Arg1c4ea4.
  ///
  /// In zh, this message translates to:
  /// **'连接成功：{arg0}{arg1}'**
  String l6ConnectedArg0Arg1c4ea4(String arg0, String arg1);

  /// No description provided for @l6TestMcpConnection1eed1.
  ///
  /// In zh, this message translates to:
  /// **'MCP 测试连接'**
  String get l6TestMcpConnection1eed1;

  /// No description provided for @l6Remote80194.
  ///
  /// In zh, this message translates to:
  /// **'远程'**
  String get l6Remote80194;

  /// No description provided for @l6AllPlatforms52e2a.
  ///
  /// In zh, this message translates to:
  /// **'全部平台'**
  String get l6AllPlatforms52e2a;

  /// No description provided for @l6OnlyArg064396.
  ///
  /// In zh, this message translates to:
  /// **'仅 {arg0}'**
  String l6OnlyArg064396(String arg0);

  /// No description provided for @l6ReadOnlyCallsSkipApproval63678.
  ///
  /// In zh, this message translates to:
  /// **'只读免审批'**
  String get l6ReadOnlyCallsSkipApproval63678;

  /// No description provided for @l6RelatedIntentsArg0dff5f.
  ///
  /// In zh, this message translates to:
  /// **'关联意图：{arg0}'**
  String l6RelatedIntentsArg0dff5f(String arg0);

  /// No description provided for @l6DisableReadOnlyApprovalExemptioned6aa.
  ///
  /// In zh, this message translates to:
  /// **'关闭只读免审批'**
  String get l6DisableReadOnlyApprovalExemptioned6aa;

  /// No description provided for @l6EnableReadOnlyApprovalExemption95d1c.
  ///
  /// In zh, this message translates to:
  /// **'开只读免审批'**
  String get l6EnableReadOnlyApprovalExemption95d1c;

  /// No description provided for @l6AiAnalysisec6c3.
  ///
  /// In zh, this message translates to:
  /// **'AI 分析'**
  String get l6AiAnalysisec6c3;

  /// No description provided for @l6BranchesArg079379.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 个分支'**
  String l6BranchesArg079379(String arg0);

  /// No description provided for @l6Arg0MinutesAgo6b0c6.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 分钟前'**
  String l6Arg0MinutesAgo6b0c6(String arg0);

  /// No description provided for @l6Arg0HoursAgoaf702.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 小时前'**
  String l6Arg0HoursAgoaf702(String arg0);

  /// No description provided for @l6Arg0DaysAgo70636.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 天前'**
  String l6Arg0DaysAgo70636(String arg0);

  /// No description provided for @l6ReadingSettings9033d.
  ///
  /// In zh, this message translates to:
  /// **'正文阅读设置'**
  String get l6ReadingSettings9033d;

  /// No description provided for @l6TheThreeSettingsAreIndependentFollow172b0.
  ///
  /// In zh, this message translates to:
  /// **'三项独立生效；「跟随全局」恢复继承，两个滑块和字体互不影响。'**
  String get l6TheThreeSettingsAreIndependentFollow172b0;

  /// No description provided for @l6Fontb50d4.
  ///
  /// In zh, this message translates to:
  /// **'字体'**
  String get l6Fontb50d4;

  /// No description provided for @l6InheritGlobal2a499.
  ///
  /// In zh, this message translates to:
  /// **'继承全局'**
  String get l6InheritGlobal2a499;

  /// No description provided for @l6OverrideGlobal93d3e.
  ///
  /// In zh, this message translates to:
  /// **'覆盖全局'**
  String get l6OverrideGlobal93d3e;

  /// No description provided for @l6FontSize576cc.
  ///
  /// In zh, this message translates to:
  /// **'字号'**
  String get l6FontSize576cc;

  /// No description provided for @l6LineSpacing514b5.
  ///
  /// In zh, this message translates to:
  /// **'行距'**
  String get l6LineSpacing514b5;

  /// No description provided for @l6ChooseAReadingFont8189e.
  ///
  /// In zh, this message translates to:
  /// **'选择正文阅读字体'**
  String get l6ChooseAReadingFont8189e;

  /// No description provided for @l6SetUpAnAiProviderToc8ad4.
  ///
  /// In zh, this message translates to:
  /// **'首次使用提示：尚未配置 AI Provider，AI 对话与自动分析暂不可用。'**
  String get l6SetUpAnAiProviderToc8ad4;

  /// No description provided for @l6Configurea44df.
  ///
  /// In zh, this message translates to:
  /// **'去配置'**
  String get l6Configurea44df;

  /// No description provided for @l6Queuecd354.
  ///
  /// In zh, this message translates to:
  /// **'队列'**
  String get l6Queuecd354;

  /// No description provided for @l6NoRecords09f60.
  ///
  /// In zh, this message translates to:
  /// **'没有记录'**
  String get l6NoRecords09f60;

  /// No description provided for @l6FailedCoolingDown4a20e.
  ///
  /// In zh, this message translates to:
  /// **'失败（冷却中）'**
  String get l6FailedCoolingDown4a20e;

  /// No description provided for @l6RecordedArg085217.
  ///
  /// In zh, this message translates to:
  /// **'记录于 {arg0}'**
  String l6RecordedArg085217(String arg0);

  /// No description provided for @l6AttemptsArg0fcc38.
  ///
  /// In zh, this message translates to:
  /// **'尝试 {arg0} 次'**
  String l6AttemptsArg0fcc38(String arg0);

  /// No description provided for @l6AutomaticallyRequeuedArg0Times3363a.
  ///
  /// In zh, this message translates to:
  /// **'已自动回队 {arg0} 轮'**
  String l6AutomaticallyRequeuedArg0Times3363a(String arg0);

  /// No description provided for @l6NextAttemptArg0272da.
  ///
  /// In zh, this message translates to:
  /// **'下次处理 {arg0}'**
  String l6NextAttemptArg0272da(String arg0);

  /// No description provided for @l6EventsArg045727.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 条事件'**
  String l6EventsArg045727(String arg0);

  /// No description provided for @l6Updated9db0.
  ///
  /// In zh, this message translates to:
  /// **'更新'**
  String get l6Updated9db0;

  /// No description provided for @l6NoNewKnowledgeExtractedc38ab.
  ///
  /// In zh, this message translates to:
  /// **'没有提炼出新的知识'**
  String get l6NoNewKnowledgeExtractedc38ab;

  /// No description provided for @l6DohSettingsSavedeaeea.
  ///
  /// In zh, this message translates to:
  /// **'DoH 设置已保存'**
  String get l6DohSettingsSavedeaeea;

  /// No description provided for @l6EnterADohServiceUrlBeforeff97f.
  ///
  /// In zh, this message translates to:
  /// **'请先填写 DoH 服务地址再测试'**
  String get l6EnterADohServiceUrlBeforeff97f;

  /// No description provided for @l6ActivationPolicyfc94d.
  ///
  /// In zh, this message translates to:
  /// **'启用策略'**
  String get l6ActivationPolicyfc94d;

  /// No description provided for @l6DohServiceUrlae8b4.
  ///
  /// In zh, this message translates to:
  /// **'DoH 服务地址'**
  String get l6DohServiceUrlae8b4;

  /// No description provided for @l6OffTheUrlIsSavedButa536d.
  ///
  /// In zh, this message translates to:
  /// **'当前为「关闭」：地址仅保存不生效，外网搜索走系统直连。'**
  String get l6OffTheUrlIsSavedButa536d;

  /// No description provided for @l6RequiredADohFailureReturnsAnaabc1.
  ///
  /// In zh, this message translates to:
  /// **'当前为「强制」：DoH 失败将直接报错，不会回退直连。'**
  String get l6RequiredADohFailureReturnsAnaabc1;

  /// No description provided for @l6AutomaticPreferDohAndEchFallingfc42f.
  ///
  /// In zh, this message translates to:
  /// **'当前为「自动」：优先 DoH+ECH，连不上自动回退直连。'**
  String get l6AutomaticPreferDohAndEchFallingfc42f;

  /// No description provided for @l6Testingcf7e9.
  ///
  /// In zh, this message translates to:
  /// **'测试中'**
  String get l6Testingcf7e9;

  /// No description provided for @l6RecordWhatIsHappeningUseName925ca.
  ///
  /// In zh, this message translates to:
  /// **'记录此刻发生的事情…（可用 @人名 标注联系人、#事情 标注项目）'**
  String get l6RecordWhatIsHappeningUseName925ca;

  /// No description provided for @l6ConversationFormat9d903.
  ///
  /// In zh, this message translates to:
  /// **'对话格式'**
  String get l6ConversationFormat9d903;

  /// No description provided for @l6HowToRecord2bcbc.
  ///
  /// In zh, this message translates to:
  /// **'怎么记录'**
  String get l6HowToRecord2bcbc;

  /// No description provided for @l6UseNameToIdentifyAContact6d759.
  ///
  /// In zh, this message translates to:
  /// **'@人名：明确标注联系人\n#事情：明确标注项目、事项或主题\n\n例如：@张伟 正在负责 #付款流程\n\n记一下：明确保存一条经历或进展\n保存到知识库：把结论沉淀下来\n帮我建待办：创建后续行动\n\n联系人关系、知识页、待办和规则会先给你看草稿。请打开“审阅提议”，查看具体内容后接受或拒绝；普通聊天回复不会执行草稿。'**
  String get l6UseNameToIdentifyAContact6d759;

  /// No description provided for @l6Arg0Revoked9671e.
  ///
  /// In zh, this message translates to:
  /// **'{arg0}（已撤销）'**
  String l6Arg0Revoked9671e(String arg0);

  /// No description provided for @l6DeviceArg0WillImmediatelyLoseSyncadbd3.
  ///
  /// In zh, this message translates to:
  /// **'设备 \"{arg0}\" 将立即失去再同步资格。\n\n丢失设备须知：撤销同步权限不会回收该设备已持有的 Provider API Key。请到对应 Provider 控制台吊销或轮换密钥，剩余设备会接收新版本。'**
  String l6DeviceArg0WillImmediatelyLoseSyncadbd3(String arg0);

  /// No description provided for @l6ChangesFromArg0710df.
  ///
  /// In zh, this message translates to:
  /// **'来自 {arg0} 的改动'**
  String l6ChangesFromArg0710df(String arg0);

  /// No description provided for @l6RenamedFromArg0e3178.
  ///
  /// In zh, this message translates to:
  /// **'由「{arg0}」改名而来'**
  String l6RenamedFromArg0e3178(String arg0);

  /// No description provided for @l6ContentLengthArg0CharactersUpdatedArg1f8788.
  ///
  /// In zh, this message translates to:
  /// **'正文 {arg0} 字 · 更新 {arg1}'**
  String l6ContentLengthArg0CharactersUpdatedArg1f8788(
    String arg0,
    String arg1,
  );

  /// No description provided for @l6ConditionsArg006edc.
  ///
  /// In zh, this message translates to:
  /// **'适用条件：{arg0}'**
  String l6ConditionsArg006edc(String arg0);

  /// No description provided for @l6WebSearchSettingsSaved30dee.
  ///
  /// In zh, this message translates to:
  /// **'外网搜索设置已保存'**
  String get l6WebSearchSettingsSaved30dee;

  /// No description provided for @l6SearchEnginee5837.
  ///
  /// In zh, this message translates to:
  /// **'搜索引擎'**
  String get l6SearchEnginee5837;

  /// No description provided for @l6SearxngInstanceUrla3267.
  ///
  /// In zh, this message translates to:
  /// **'SearxNG 实例地址'**
  String get l6SearxngInstanceUrla3267;

  /// No description provided for @l6ApiUrlacd8a.
  ///
  /// In zh, this message translates to:
  /// **'API 地址'**
  String get l6ApiUrlacd8a;

  /// No description provided for @l6HttpLocalhost8080OrYourSearxngba754.
  ///
  /// In zh, this message translates to:
  /// **'http://localhost:8080 或你的 SearxNG 地址'**
  String get l6HttpLocalhost8080OrYourSearxngba754;

  /// No description provided for @l6SearxngIsSelfHostedMetasearchNoe6c57.
  ///
  /// In zh, this message translates to:
  /// **'SearxNG 是自托管元搜索，无需 API key。需在你的实例开启 JSON 输出格式。'**
  String get l6SearxngIsSelfHostedMetasearchNoe6c57;

  /// No description provided for @l6EnableWebSearch9ee9a.
  ///
  /// In zh, this message translates to:
  /// **'启用外网搜索'**
  String get l6EnableWebSearch9ee9a;

  /// No description provided for @l6AllowAiToSearchTheWeb1ea6a.
  ///
  /// In zh, this message translates to:
  /// **'开启后，AI 可在你询问最新/新闻/热点类问题时联网检索'**
  String get l6AllowAiToSearchTheWeb1ea6a;

  /// No description provided for @l6TvlyFreeAllowanceAtTavilyCom279ec.
  ///
  /// In zh, this message translates to:
  /// **'tvly-...（tavily.com 免费注册即有额度）'**
  String get l6TvlyFreeAllowanceAtTavilyCom279ec;

  /// No description provided for @l6BsaApplyAtBraveComSearch6c59c.
  ///
  /// In zh, this message translates to:
  /// **'BSA...（brave.com/search/api 申请）'**
  String get l6BsaApplyAtBraveComSearch6c59c;

  /// No description provided for @l6ConfiguredEnterAValueToReplace1d9d7.
  ///
  /// In zh, this message translates to:
  /// **'已配置（输入新值覆盖，留空保持不变）'**
  String get l6ConfiguredEnterAValueToReplace1d9d7;

  /// No description provided for @l6ProxySettingsSavedOutboundRequestsUseac39b.
  ///
  /// In zh, this message translates to:
  /// **'代理设置已保存，出站请求即时生效'**
  String get l6ProxySettingsSavedOutboundRequestsUseac39b;

  /// No description provided for @l6EnterAProxyUrlBeforeTesting89cfb.
  ///
  /// In zh, this message translates to:
  /// **'请先填写代理地址再测试'**
  String get l6EnterAProxyUrlBeforeTesting89cfb;

  /// No description provided for @l6UseHttpsProxyOrAllProxy33ed0.
  ///
  /// In zh, this message translates to:
  /// **'跟随时下环境：HTTPS_PROXY / ALL_PROXY，未设置则直连'**
  String get l6UseHttpsProxyOrAllProxy33ed0;

  /// No description provided for @l6ConfigureAProxyea2d0.
  ///
  /// In zh, this message translates to:
  /// **'手动配置代理'**
  String get l6ConfigureAProxyea2d0;

  /// No description provided for @l6UseTheUrlBelowInsteadOf96914.
  ///
  /// In zh, this message translates to:
  /// **'以下方地址为准，覆盖环境变量；挂代理期间 DoH 隧道自动让位'**
  String get l6UseTheUrlBelowInsteadOf96914;

  /// No description provided for @l6NoProxy4f187.
  ///
  /// In zh, this message translates to:
  /// **'不使用代理'**
  String get l6NoProxy4f187;

  /// No description provided for @l6ConnectDirectlyIgnoringProxyEnvironmentVariablesa4dbf.
  ///
  /// In zh, this message translates to:
  /// **'强制直连，环境变量中的代理也被忽略'**
  String get l6ConnectDirectlyIgnoringProxyEnvironmentVariablesa4dbf;

  /// No description provided for @l6Http1270017890a9d59.
  ///
  /// In zh, this message translates to:
  /// **'http://127.0.0.1:7890 或 socks5://…'**
  String get l6Http1270017890a9d59;

  /// No description provided for @l6SelectAnEventToViewItsd5e60.
  ///
  /// In zh, this message translates to:
  /// **'左侧选择事件查看详情'**
  String get l6SelectAnEventToViewItsd5e60;

  /// No description provided for @l6EventDoesNotExistf5542.
  ///
  /// In zh, this message translates to:
  /// **'事件不存在'**
  String get l6EventDoesNotExistf5542;

  /// No description provided for @l6EventRecordsbabe0.
  ///
  /// In zh, this message translates to:
  /// **'事件记录'**
  String get l6EventRecordsbabe0;

  /// No description provided for @l6RecordedAt650b3.
  ///
  /// In zh, this message translates to:
  /// **'记录时间'**
  String get l6RecordedAt650b3;

  /// No description provided for @l6NoAnalysisYetPendingOrNo90172.
  ///
  /// In zh, this message translates to:
  /// **'尚无可用分析（等待处理或未配置分析器）。'**
  String get l6NoAnalysisYetPendingOrNo90172;

  /// No description provided for @l6AnalysisSummary83a0f.
  ///
  /// In zh, this message translates to:
  /// **'分析摘要'**
  String get l6AnalysisSummary83a0f;

  /// No description provided for @l6Peopleb755b.
  ///
  /// In zh, this message translates to:
  /// **'人物'**
  String get l6Peopleb755b;

  /// No description provided for @l6NoRelatedRuleReminders24596.
  ///
  /// In zh, this message translates to:
  /// **'无相关规则提醒'**
  String get l6NoRelatedRuleReminders24596;

  /// No description provided for @l6CouldNotLoadEvent7ba1f.
  ///
  /// In zh, this message translates to:
  /// **'事件加载失败'**
  String get l6CouldNotLoadEvent7ba1f;

  /// No description provided for @l6ConversationsWithAiAreRecordedAs6eb67.
  ///
  /// In zh, this message translates to:
  /// **'和 AI 的对话会被记录为事件，用于回顾与知识沉淀。'**
  String get l6ConversationsWithAiAreRecordedAs6eb67;

  /// No description provided for @l6TodaySRecordse0105.
  ///
  /// In zh, this message translates to:
  /// **'今天记录'**
  String get l6TodaySRecordse0105;

  /// No description provided for @l6Last7Days2a0a0.
  ///
  /// In zh, this message translates to:
  /// **'最近 7 天'**
  String get l6Last7Days2a0a0;

  /// No description provided for @l6NoEventsRecordedYetConversationsWithb4ac9.
  ///
  /// In zh, this message translates to:
  /// **'还没有记录过事件\n和 AI 的对话会自动作为事件记录在这里'**
  String get l6NoEventsRecordedYetConversationsWithb4ac9;

  /// No description provided for @l6NoRecentlyRecordedEventsf0c62.
  ///
  /// In zh, this message translates to:
  /// **'最近没有记录的事件'**
  String get l6NoRecentlyRecordedEventsf0c62;

  /// No description provided for @l6TryAgainLaterb8e0d.
  ///
  /// In zh, this message translates to:
  /// **'稍后重试'**
  String get l6TryAgainLaterb8e0d;

  /// No description provided for @l6Arg0CompletedArg1Items30653.
  ///
  /// In zh, this message translates to:
  /// **'{arg0}完成 · {arg1} 项'**
  String l6Arg0CompletedArg1Items30653(String arg0, String arg1);

  /// No description provided for @l6AutomaticRetryArg06d99e.
  ///
  /// In zh, this message translates to:
  /// **'自动重试：{arg0}'**
  String l6AutomaticRetryArg06d99e(String arg0);

  /// No description provided for @l6Arg0FromThisPageSSource92ed3.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} · 来自本页原料'**
  String l6Arg0FromThisPageSSource92ed3(String arg0);

  /// No description provided for @l6AcceptChangesArg0a09f4.
  ///
  /// In zh, this message translates to:
  /// **'采纳修改 {arg0}'**
  String l6AcceptChangesArg0a09f4(String arg0);

  /// No description provided for @l6RemoveThisParagraphe26cb.
  ///
  /// In zh, this message translates to:
  /// **'删除这一段'**
  String get l6RemoveThisParagraphe26cb;

  /// No description provided for @l6HistoricalSnapshotsArg0cbbd3.
  ///
  /// In zh, this message translates to:
  /// **'共有 {arg0} 个历史快照'**
  String l6HistoricalSnapshotsArg0cbbd3(String arg0);

  /// No description provided for @l6LatestArg0Arg1aa89a.
  ///
  /// In zh, this message translates to:
  /// **'最新：{arg0} · {arg1}'**
  String l6LatestArg0Arg1aa89a(String arg0, String arg1);

  /// No description provided for @l6DeviceNameArg0a02e4.
  ///
  /// In zh, this message translates to:
  /// **'设备名：{arg0}'**
  String l6DeviceNameArg0a02e4(String arg0);

  /// No description provided for @l6RoleArg0f3803.
  ///
  /// In zh, this message translates to:
  /// **'角色：{arg0}'**
  String l6RoleArg0f3803(String arg0);

  /// No description provided for @l6NoRelayConfiguredSetItWhenfc098.
  ///
  /// In zh, this message translates to:
  /// **'尚未配置中继（首次邀请成员时填写）'**
  String get l6NoRelayConfiguredSetItWhenfc098;

  /// No description provided for @l6StartANewChapterHereEarliere67c8.
  ///
  /// In zh, this message translates to:
  /// **'翻篇到此——此前历史收进章节摘要'**
  String get l6StartANewChapterHereEarliere67c8;

  /// No description provided for @l6ThisChapterMarkerNoLongerExistsec974.
  ///
  /// In zh, this message translates to:
  /// **'翻篇标记已不存在，可能已被撤销'**
  String get l6ThisChapterMarkerNoLongerExistsec974;

  /// No description provided for @l6NewChapterSummarycf20d.
  ///
  /// In zh, this message translates to:
  /// **'翻篇 · 章节摘要'**
  String get l6NewChapterSummarycf20d;

  /// No description provided for @l6GeneratingChapterSummary277ba.
  ///
  /// In zh, this message translates to:
  /// **'正在生成章节摘要…'**
  String get l6GeneratingChapterSummary277ba;

  /// No description provided for @l6DiscardChanges1cead.
  ///
  /// In zh, this message translates to:
  /// **'放弃修改'**
  String get l6DiscardChanges1cead;

  /// No description provided for @l6ConfirmNewChapter12069.
  ///
  /// In zh, this message translates to:
  /// **'确认翻篇'**
  String get l6ConfirmNewChapter12069;

  /// No description provided for @l6RemoveThisEntry52142.
  ///
  /// In zh, this message translates to:
  /// **'删除这一条'**
  String get l6RemoveThisEntry52142;

  /// No description provided for @l6AddArg03e9a8.
  ///
  /// In zh, this message translates to:
  /// **'加一条{arg0}'**
  String l6AddArg03e9a8(String arg0);

  /// No description provided for @l6AfterStartingANewChapterThis0bfa3.
  ///
  /// In zh, this message translates to:
  /// **'翻篇后这段历史不再原文进入请求，仅以本摘要常驻；需要精确数值或原话时 AI 会回读原始消息。可编辑或删减条目后确认。'**
  String get l6AfterStartingANewChapterThis0bfa3;

  /// No description provided for @l6NoChapterHistoryInThisConversation36eaa.
  ///
  /// In zh, this message translates to:
  /// **'本会话还没有翻篇记录'**
  String get l6NoChapterHistoryInThisConversation36eaa;

  /// No description provided for @l6Day15917.
  ///
  /// In zh, this message translates to:
  /// **'日'**
  String get l6Day15917;

  /// No description provided for @l6Week451b8.
  ///
  /// In zh, this message translates to:
  /// **'周'**
  String get l6Week451b8;

  /// No description provided for @l6Monthd9b59.
  ///
  /// In zh, this message translates to:
  /// **'月'**
  String get l6Monthd9b59;

  /// No description provided for @l6PreviousDaydede7.
  ///
  /// In zh, this message translates to:
  /// **'前一天'**
  String get l6PreviousDaydede7;

  /// No description provided for @l6NextDaya4161.
  ///
  /// In zh, this message translates to:
  /// **'后一天'**
  String get l6NextDaya4161;

  /// No description provided for @l6CouldNotLoadReview55475.
  ///
  /// In zh, this message translates to:
  /// **'回顾加载失败'**
  String get l6CouldNotLoadReview55475;

  /// No description provided for @l6AiReviewbb899.
  ///
  /// In zh, this message translates to:
  /// **'AI 回顾'**
  String get l6AiReviewbb899;

  /// No description provided for @l6NoAiReviewForThisDay286ea.
  ///
  /// In zh, this message translates to:
  /// **'当天还没有 AI 回顾'**
  String get l6NoAiReviewForThisDay286ea;

  /// No description provided for @l6EventsForThisDayfbec1.
  ///
  /// In zh, this message translates to:
  /// **'当日事件'**
  String get l6EventsForThisDayfbec1;

  /// No description provided for @l6NoEventsRecordedOnThisDay6a37a.
  ///
  /// In zh, this message translates to:
  /// **'当天没有记录事件'**
  String get l6NoEventsRecordedOnThisDay6a37a;

  /// No description provided for @l6ThisDayLastWeekfc29d.
  ///
  /// In zh, this message translates to:
  /// **'上周的今天'**
  String get l6ThisDayLastWeekfc29d;

  /// No description provided for @l6EarlierReviewsArg0f4cc0.
  ///
  /// In zh, this message translates to:
  /// **'更早的盘点（{arg0}）'**
  String l6EarlierReviewsArg0f4cc0(String arg0);

  /// No description provided for @l6PeriodReviewArg0ToArg1Arg269c4d.
  ///
  /// In zh, this message translates to:
  /// **'周期盘点 · {arg0} ~ {arg1}{arg2}'**
  String l6PeriodReviewArg0ToArg1Arg269c4d(
    String arg0,
    String arg1,
    String arg2,
  );

  /// No description provided for @l6Events550e3.
  ///
  /// In zh, this message translates to:
  /// **'事件'**
  String get l6Events550e3;

  /// No description provided for @l6ToDosCompletede1bfa.
  ///
  /// In zh, this message translates to:
  /// **'待办完成'**
  String get l6ToDosCompletede1bfa;

  /// No description provided for @l6DeviationChecksPending648aa.
  ///
  /// In zh, this message translates to:
  /// **'偏差检测待进行'**
  String get l6DeviationChecksPending648aa;

  /// No description provided for @l6NoDeviationsToReport28ee2.
  ///
  /// In zh, this message translates to:
  /// **'未发现待说出的偏差'**
  String get l6NoDeviationsToReport28ee2;

  /// No description provided for @l6MomentsThatDivergedFromYourGoals39f82.
  ///
  /// In zh, this message translates to:
  /// **'偏离目标的时刻——只陈述，不评判：'**
  String get l6MomentsThatDivergedFromYourGoals39f82;

  /// No description provided for @l6Arg0Dismissede66fc.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} · 已忽略'**
  String l6Arg0Dismissede66fc(String arg0);

  /// No description provided for @l6GoalArg06fb2b.
  ///
  /// In zh, this message translates to:
  /// **'目标：{arg0}'**
  String l6GoalArg06fb2b(String arg0);

  /// No description provided for @l6CouldNotLoadStatistics88ae7.
  ///
  /// In zh, this message translates to:
  /// **'统计加载失败'**
  String get l6CouldNotLoadStatistics88ae7;

  /// No description provided for @l6NoRecordsThisWeeka75a5.
  ///
  /// In zh, this message translates to:
  /// **'本周没有记录'**
  String get l6NoRecordsThisWeeka75a5;

  /// No description provided for @l6NoRecordsThisMonthd57d9.
  ///
  /// In zh, this message translates to:
  /// **'本月没有记录'**
  String get l6NoRecordsThisMonthd57d9;

  /// No description provided for @l6EventTypes40b68.
  ///
  /// In zh, this message translates to:
  /// **'事类'**
  String get l6EventTypes40b68;

  /// No description provided for @l6Activitiesb2548.
  ///
  /// In zh, this message translates to:
  /// **'活动'**
  String get l6Activitiesb2548;

  /// No description provided for @l6KnowledgeProcessingArg0ActivePagesArg179c73.
  ///
  /// In zh, this message translates to:
  /// **'知识管道 · {arg0} 页活跃{arg1}{arg2} · 摘要 30d 内 {arg3} 成{arg4}'**
  String l6KnowledgeProcessingArg0ActivePagesArg179c73(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  );

  /// No description provided for @l6KnowledgeProcessingHealthLast30Days3bf5c.
  ///
  /// In zh, this message translates to:
  /// **'知识管道健康（近 30 天）'**
  String get l6KnowledgeProcessingHealthLast30Days3bf5c;

  /// No description provided for @l6Compilation8669e.
  ///
  /// In zh, this message translates to:
  /// **'编译'**
  String get l6Compilation8669e;

  /// No description provided for @l6PendingProposalsArg030DaysAcceptedf2e76.
  ///
  /// In zh, this message translates to:
  /// **'待审建议 {arg0}（30d 采纳 {arg1} / 拒绝 {arg2}）· 后台任务失败 {arg3}/{arg4}'**
  String l6PendingProposalsArg030DaysAcceptedf2e76(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  );

  /// No description provided for @l6Librarydfd6b.
  ///
  /// In zh, this message translates to:
  /// **'库存'**
  String get l6Librarydfd6b;

  /// No description provided for @l6ActiveArg0ArchivedArg1NoSummaryb4539.
  ///
  /// In zh, this message translates to:
  /// **'活跃 {arg0} · 归档 {arg1} · 无摘要 {arg2} · 长期未更新 {arg3} · 从未投递 {arg4}'**
  String l6ActiveArg0ArchivedArg1NoSummaryb4539(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
  );

  /// No description provided for @l6Delivery412a9.
  ///
  /// In zh, this message translates to:
  /// **'投递'**
  String get l6Delivery412a9;

  /// No description provided for @l6AttachedArg0TimesArg1PagesUsageabc07.
  ///
  /// In zh, this message translates to:
  /// **'挂载 {arg0} 次（{arg1} 页）· 使用记录 {arg2} · 被引用 {arg3} 页'**
  String l6AttachedArg0TimesArg1PagesUsageabc07(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  );

  /// No description provided for @l6Retrieval1c037.
  ///
  /// In zh, this message translates to:
  /// **'检索'**
  String get l6Retrieval1c037;

  /// No description provided for @l6AiSearchesArg0PageReadsArg15216f.
  ///
  /// In zh, this message translates to:
  /// **'AI 检索 {arg0} 次 · 取页 {arg1} 次'**
  String l6AiSearchesArg0PageReadsArg15216f(String arg0, String arg1);

  /// No description provided for @l6SummaryPipeline00b38.
  ///
  /// In zh, this message translates to:
  /// **'摘要管线'**
  String get l6SummaryPipeline00b38;

  /// No description provided for @l6CompletedArg0UnavailableArg1Arg2ee56d.
  ///
  /// In zh, this message translates to:
  /// **'完成 {arg0} · 不可用 {arg1}{arg2}'**
  String l6CompletedArg0UnavailableArg1Arg2ee56d(
    String arg0,
    String arg1,
    String arg2,
  );

  /// No description provided for @l6SourceReviewGate877bd.
  ///
  /// In zh, this message translates to:
  /// **'孵化闸门'**
  String get l6SourceReviewGate877bd;

  /// No description provided for @l6SourcesAwaitingADecisionArg0Rejected9c019.
  ///
  /// In zh, this message translates to:
  /// **'待定论 {arg0} 页 · 负知识 {arg1} 条'**
  String l6SourcesAwaitingADecisionArg0Rejected9c019(String arg0, String arg1);

  /// No description provided for @l6SummaryPreparationHistoryProcessedInBackground2648e.
  ///
  /// In zh, this message translates to:
  /// **'摘要预热账本（背景 tick 多批推进，事故 2026-10-19 P1(c)）'**
  String get l6SummaryPreparationHistoryProcessedInBackground2648e;

  /// No description provided for @l6NoPreparationHistoryYetBackgroundBatchescc092.
  ///
  /// In zh, this message translates to:
  /// **'暂无预热记录（大树冷启动时由背景分批追平）。'**
  String get l6NoPreparationHistoryYetBackgroundBatchescc092;

  /// No description provided for @l6LoadingHandoffStatus19264.
  ///
  /// In zh, this message translates to:
  /// **'正在读取移交状态…'**
  String get l6LoadingHandoffStatus19264;

  /// No description provided for @l6TransferAiAnalysisResponsibilityToArg05bc00.
  ///
  /// In zh, this message translates to:
  /// **'将把 AI 分析职责移交给 \"{arg0}\"（{arg1}）。\n\n本机会立即停止认领后台任务；目标设备需同步到移交边界并显式确认后才开始 AI 分析。确认后本机不能单方面恢复。'**
  String l6TransferAiAnalysisResponsibilityToArg05bc00(
    String arg0,
    String arg1,
  );

  /// No description provided for @l6EnrollmentUsesASeparateStagingDatabase0ddf7.
  ///
  /// In zh, this message translates to:
  /// **'使用独立暂存库，原记录和 Provider 不会覆盖。明确批准并安装后，自动协调正式通道、成员名册及初始快照；当前数据集不会切换。'**
  String get l6EnrollmentUsesASeparateStagingDatabase0ddf7;

  /// No description provided for @l6UsingKey99422.
  ///
  /// In zh, this message translates to:
  /// **'通过 Key'**
  String get l6UsingKey99422;

  /// No description provided for @l6InvitingDeviceArg0d440c.
  ///
  /// In zh, this message translates to:
  /// **'邀请设备：{arg0}'**
  String l6InvitingDeviceArg0d440c(String arg0);

  /// No description provided for @l6DatasetArg085f96.
  ///
  /// In zh, this message translates to:
  /// **'数据集：{arg0}'**
  String l6DatasetArg085f96(String arg0);

  /// No description provided for @l6InvitationExpiresArg0abd8a.
  ///
  /// In zh, this message translates to:
  /// **'邀请到期：{arg0}'**
  String l6InvitationExpiresArg0abd8a(String arg0);

  /// No description provided for @l6DatasetArg0OnlyApprovedCheckpointsCand6435.
  ///
  /// In zh, this message translates to:
  /// **'数据集：{arg0}\n仅恢复已审批检查点；未审批状态重启后需新邀请。'**
  String l6DatasetArg0OnlyApprovedCheckpointsCand6435(String arg0);

  /// No description provided for @l6StagedDataAndDeviceKeysFor0f456.
  ///
  /// In zh, this message translates to:
  /// **'「{arg0}」的暂存数据和本机密钥会被删除，且不可恢复。如果还想加入这个数据集，之后需要重新扫码并经对方批准。'**
  String l6StagedDataAndDeviceKeysFor0f456(String arg0);

  /// No description provided for @l6ThisDeviceArg0Arg1ab420.
  ///
  /// In zh, this message translates to:
  /// **'本机：{arg0} · {arg1}'**
  String l6ThisDeviceArg0Arg1ab420(String arg0, String arg1);

  /// No description provided for @l6ThisDeviceSSyncIdentityIs46884.
  ///
  /// In zh, this message translates to:
  /// **'本机的同步身份不完整（常见于配对过程中断、掉电或恢复备份）。可以清除本机同步身份后重新配对 —— 笔记、对话、知识库都会保留。'**
  String get l6ThisDeviceSSyncIdentityIs46884;

  /// No description provided for @l6SyncOperationsAwaitingConfirmationArg0224a8.
  ///
  /// In zh, this message translates to:
  /// **'待确认同步操作：{arg0}'**
  String l6SyncOperationsAwaitingConfirmationArg0224a8(String arg0);

  /// No description provided for @l6DeviceArg0d7706.
  ///
  /// In zh, this message translates to:
  /// **'设备：{arg0}'**
  String l6DeviceArg0d7706(String arg0);

  /// No description provided for @l6PlatformArg0e4ee7.
  ///
  /// In zh, this message translates to:
  /// **'平台：{arg0}'**
  String l6PlatformArg0e4ee7(String arg0);

  /// No description provided for @l6ExpiresArg0a7daf.
  ///
  /// In zh, this message translates to:
  /// **'到期时间：{arg0}'**
  String l6ExpiresArg0a7daf(String arg0);

  /// No description provided for @l6AwaitingReviewArg0Arg17fa78.
  ///
  /// In zh, this message translates to:
  /// **'待审阅：{arg0}（{arg1}）'**
  String l6AwaitingReviewArg0Arg17fa78(String arg0, String arg1);

  /// No description provided for @l6RevisionFromArg0beef3.
  ///
  /// In zh, this message translates to:
  /// **'来自 {arg0} 的修订'**
  String l6RevisionFromArg0beef3(String arg0);

  /// No description provided for @l6AddressHintArg03f1aa.
  ///
  /// In zh, this message translates to:
  /// **'地址提示：{arg0}'**
  String l6AddressHintArg03f1aa(String arg0);

  /// No description provided for @l6LastSyncUploadedArg0ReceivedArg120f78.
  ///
  /// In zh, this message translates to:
  /// **'最近同步：上行 {arg0} · 到达 {arg1}{arg2}{arg3}{arg4}{arg5}'**
  String l6LastSyncUploadedArg0ReceivedArg120f78(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
    String arg5,
  );

  /// No description provided for @l6Sendingd948e.
  ///
  /// In zh, this message translates to:
  /// **'发送中'**
  String get l6Sendingd948e;

  /// No description provided for @l6RetryingLaterArg0baf89.
  ///
  /// In zh, this message translates to:
  /// **'稍后重试：{arg0}'**
  String l6RetryingLaterArg0baf89(String arg0);

  /// No description provided for @l6UnknownStateArg0c8bbc.
  ///
  /// In zh, this message translates to:
  /// **'未知状态：{arg0}'**
  String l6UnknownStateArg0c8bbc(String arg0);

  /// No description provided for @l6RetryInArg0Secondsccd79.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 秒后重试'**
  String l6RetryInArg0Secondsccd79(String arg0);

  /// No description provided for @l6RetryInArg0Minutesef5e1.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 分钟后重试'**
  String l6RetryInArg0Minutesef5e1(String arg0);

  /// No description provided for @l6DeliveryArg0QueuedArg1AwaitingConfirmationa5864.
  ///
  /// In zh, this message translates to:
  /// **'投递 {arg0}（待发 {arg1} · 待确认 {arg2}）：{arg3}'**
  String l6DeliveryArg0QueuedArg1AwaitingConfirmationa5864(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
  );

  /// No description provided for @l6PeerHasNotConfirmedArg0Itemsc4ccb.
  ///
  /// In zh, this message translates to:
  /// **'对端未确认 {arg0} 条 · {arg1}'**
  String l6PeerHasNotConfirmedArg0Itemsc4ccb(String arg0, String arg1);

  /// No description provided for @l6EnterTheTargetDeviceIdb0da4.
  ///
  /// In zh, this message translates to:
  /// **'请填写目标设备 ID。'**
  String get l6EnterTheTargetDeviceIdb0da4;

  /// No description provided for @l6ThisDeviceIsNotTheProcessor82939.
  ///
  /// In zh, this message translates to:
  /// **'本机当前不是加工设备，无需让出。'**
  String get l6ThisDeviceIsNotTheProcessor82939;

  /// No description provided for @l6OnlyTheDatasetOwnerCanApprovec5d36.
  ///
  /// In zh, this message translates to:
  /// **'只有 owner 设备才能审批移交证明。'**
  String get l6OnlyTheDatasetOwnerCanApprovec5d36;

  /// No description provided for @l6WaitForThisDeviceSActive1e382.
  ///
  /// In zh, this message translates to:
  /// **'本机仍有进行中的后台任务，必须等它们完成后才能让出 AI 分析职责。'**
  String get l6WaitForThisDeviceSActive1e382;

  /// No description provided for @l6WaitForSyncToReachThe58a6b.
  ///
  /// In zh, this message translates to:
  /// **'本机尚未同步到移交边界，请先等待同步追平再确认接管。'**
  String get l6WaitForSyncToReachThe58a6b;

  /// No description provided for @l6NoProcessorHandoffAwaitsThisDevice12c2e.
  ///
  /// In zh, this message translates to:
  /// **'本机没有待确认的加工接管。'**
  String get l6NoProcessorHandoffAwaitsThisDevice12c2e;

  /// No description provided for @l6TheHandoffProofSignatureIsInvalid82d09.
  ///
  /// In zh, this message translates to:
  /// **'移交证明签名无效，可能已被篡改。'**
  String get l6TheHandoffProofSignatureIsInvalid82d09;

  /// No description provided for @l6HandoffProofInformationIsOutsideIts3c0a0.
  ///
  /// In zh, this message translates to:
  /// **'移交证明信息超出有效范围。'**
  String get l6HandoffProofInformationIsOutsideIts3c0a0;

  /// No description provided for @l6TheProofDoesNotMatchThe90664.
  ///
  /// In zh, this message translates to:
  /// **'移交证明版本与当前名册不一致，请重新在原加工设备上让出。'**
  String get l6TheProofDoesNotMatchThe90664;

  /// No description provided for @l6TheProofDoesNotMatchThe7ec43.
  ///
  /// In zh, this message translates to:
  /// **'移交证明与当前加工设备不符，请重新在原加工设备上让出。'**
  String get l6TheProofDoesNotMatchThe7ec43;

  /// No description provided for @l6TheTargetIsNotAnActivea00bf.
  ///
  /// In zh, this message translates to:
  /// **'移交目标不是当前名册中的活跃设备。'**
  String get l6TheTargetIsNotAnActivea00bf;

  /// No description provided for @l6TheOriginalProcessorIsNoLongerd44b7.
  ///
  /// In zh, this message translates to:
  /// **'原加工设备已不在名册中，无法审批此证明。'**
  String get l6TheOriginalProcessorIsNoLongerd44b7;

  /// No description provided for @l6TheHandoffProofBelongsToA7b34b.
  ///
  /// In zh, this message translates to:
  /// **'移交证明与当前数据集不符。'**
  String get l6TheHandoffProofBelongsToA7b34b;

  /// No description provided for @l6AiAnalysisCannotBeTransferredTod5ffa.
  ///
  /// In zh, this message translates to:
  /// **'AI 分析职责不能移交给本机自己。'**
  String get l6AiAnalysisCannotBeTransferredTod5ffa;

  /// No description provided for @l6TheHandoffProofDoesNotMatch31e8f.
  ///
  /// In zh, this message translates to:
  /// **'移交证明与本机状态不符。'**
  String get l6TheHandoffProofDoesNotMatch31e8f;

  /// No description provided for @l6HandoffProofIsMissingTheDevice668d5.
  ///
  /// In zh, this message translates to:
  /// **'缺少移交证明，无法应用此设备变更。'**
  String get l6HandoffProofIsMissingTheDevice668d5;

  /// No description provided for @l6TheProofSSourceDoesNot02b44.
  ///
  /// In zh, this message translates to:
  /// **'移交证明来源与本机状态不符。'**
  String get l6TheProofSSourceDoesNot02b44;

  /// No description provided for @l6TheHandoffProofHasNoSource9502a.
  ///
  /// In zh, this message translates to:
  /// **'移交证明缺少来源设备。'**
  String get l6TheHandoffProofHasNoSource9502a;

  /// No description provided for @l6TheHandoffProofTargetsADifferent2a815.
  ///
  /// In zh, this message translates to:
  /// **'移交证明目标与本机不符。'**
  String get l6TheHandoffProofTargetsADifferent2a815;

  /// No description provided for @l6ProcessorHandoffProofIsMissingThis0e9df.
  ///
  /// In zh, this message translates to:
  /// **'缺少加工移交证明，本设备无法采纳此变更。'**
  String get l6ProcessorHandoffProofIsMissingThis0e9df;

  /// No description provided for @l6TheRosterIncludesHandoffProofWithout363f4.
  ///
  /// In zh, this message translates to:
  /// **'名册未变更加工设备却携带移交证明。'**
  String get l6TheRosterIncludesHandoffProofWithout363f4;

  /// No description provided for @l6UnlockThisDeviceSSyncKeyse19a6.
  ///
  /// In zh, this message translates to:
  /// **'本机同步密钥已锁定，请先解锁后再操作。'**
  String get l6UnlockThisDeviceSSyncKeyse19a6;

  /// No description provided for @l6ThisDeviceSSyncIdentityIs5c640.
  ///
  /// In zh, this message translates to:
  /// **'本机尚未准备同步身份。'**
  String get l6ThisDeviceSSyncIdentityIs5c640;

  /// No description provided for @l6TransferThisDeviceSAiAnalysis9e01d.
  ///
  /// In zh, this message translates to:
  /// **'该设备当前负责 AI 分析，必须先移交职责才能撤销。'**
  String get l6TransferThisDeviceSAiAnalysis9e01d;

  /// No description provided for @l6AnotherDeviceUpdatedTheRosterRefreshb7381.
  ///
  /// In zh, this message translates to:
  /// **'名册刚被其他设备更新，请刷新后重新审阅撤销。'**
  String get l6AnotherDeviceUpdatedTheRosterRefreshb7381;

  /// No description provided for @l6OnlyTheDatasetOwnerCanRevoke2ce42.
  ///
  /// In zh, this message translates to:
  /// **'只有 owner 设备才能撤销成员。'**
  String get l6OnlyTheDatasetOwnerCanRevoke2ce42;

  /// No description provided for @l6ThisDeviceSAccessHasAlreadyb76c9.
  ///
  /// In zh, this message translates to:
  /// **'该设备已被撤销。'**
  String get l6ThisDeviceSAccessHasAlreadyb76c9;

  /// No description provided for @l6ThisDeviceIsNotInThe3f9c7.
  ///
  /// In zh, this message translates to:
  /// **'该设备不在名册中。'**
  String get l6ThisDeviceIsNotInThe3f9c7;

  /// No description provided for @l6InvalidRosterRevisione21c2.
  ///
  /// In zh, this message translates to:
  /// **'名册修订号无效。'**
  String get l6InvalidRosterRevisione21c2;

  /// No description provided for @l6TheRosterHasReachedItsMemberee48e.
  ///
  /// In zh, this message translates to:
  /// **'名册成员数已达上限。'**
  String get l6TheRosterHasReachedItsMemberee48e;

  /// No description provided for @l6DeviceIdentitiesConflictInTheRosterea7bc.
  ///
  /// In zh, this message translates to:
  /// **'名册设备身份冲突（重复密钥或流）。'**
  String get l6DeviceIdentitiesConflictInTheRosterea7bc;

  /// No description provided for @l6TheLocalPageChangedRefreshAnd4e980.
  ///
  /// In zh, this message translates to:
  /// **'本端页面已变化，请刷新后重新审阅。'**
  String get l6TheLocalPageChangedRefreshAnd4e980;

  /// No description provided for @l6ChannelAuthorizationOrRevisionChangedAutomatic779c0.
  ///
  /// In zh, this message translates to:
  /// **'正式通道的授权或修订已改变，已停止自动交接。请检查原设备与中继授权；不会读取新修订后覆盖权限。'**
  String get l6ChannelAuthorizationOrRevisionChangedAutomatic779c0;

  /// No description provided for @l6TheHandoffRevisionOrIdentityCould284a1.
  ///
  /// In zh, this message translates to:
  /// **'交接的系统修订或身份未能确认，已暂停。请解锁本机或恢复原暂存项；不会重置密钥。'**
  String get l6TheHandoffRevisionOrIdentityCould284a1;

  /// No description provided for @l6TheRevisionAndReleaseOfThebd873.
  ///
  /// In zh, this message translates to:
  /// **'系统修订未确认，且无法确认本机会话已释放。请保留数据并关闭、重开应用后恢复。'**
  String get l6TheRevisionAndReleaseOfThebd873;

  /// No description provided for @l6TheHandoffStepIsIncompleteThe70ae7.
  ///
  /// In zh, this message translates to:
  /// **'交接步骤暂未完成，稍后按原请求重试；不会重新审批或更换密钥。'**
  String get l6TheHandoffStepIsIncompleteThe70ae7;

  /// No description provided for @l6Arg0Secondse612a.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 秒'**
  String l6Arg0Secondse612a(String arg0);

  /// No description provided for @l6Arg0Milliseconds5f522.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 毫秒'**
  String l6Arg0Milliseconds5f522(String arg0);

  /// No description provided for @l6TheSyncStepDidNotRespond28e88.
  ///
  /// In zh, this message translates to:
  /// **'同步步骤超过 {arg0}没有响应，已重新排队重试；数据没有丢失。'**
  String l6TheSyncStepDidNotRespond28e88(String arg0);

  /// No description provided for @l6TheApprovedHandoffIsBeingCoordinated7e9e2.
  ///
  /// In zh, this message translates to:
  /// **'已批准的交接正在自动协调；初始快照、回执和当前数据集切换是不同阶段。'**
  String get l6TheApprovedHandoffIsBeingCoordinated7e9e2;

  /// No description provided for @l6TheNativeHandoffRevisionCouldNot775d0.
  ///
  /// In zh, this message translates to:
  /// **'交接的原生修订未能确认，已停止本机会话。请解锁恢复原暂存项，不要重新加入。'**
  String get l6TheNativeHandoffRevisionCouldNot775d0;

  /// No description provided for @l6PreviewDidNotConnectToThed4c19.
  ///
  /// In zh, this message translates to:
  /// **'预览未连接中继。请核对邀请来源、地址和设备指纹。'**
  String get l6PreviewDidNotConnectToThed4c19;

  /// No description provided for @l6EnterADeviceNameAndExplicitly802c8.
  ///
  /// In zh, this message translates to:
  /// **'填写设备名称，并明确同意连接显示的中继、接收 Provider 配置和 API Key。'**
  String get l6EnterADeviceNameAndExplicitly802c8;

  /// No description provided for @l6JoinRequestDeliveredVerifyThisDevice8e98f.
  ///
  /// In zh, this message translates to:
  /// **'加入请求已投递。请在已有设备核对本机指纹并明确批准。'**
  String get l6JoinRequestDeliveredVerifyThisDevice8e98f;

  /// No description provided for @l6NoUnfinishedEnrollmentStagingEntriesaa853.
  ///
  /// In zh, this message translates to:
  /// **'没有未完成的加入暂存项。'**
  String get l6NoUnfinishedEnrollmentStagingEntriesaa853;

  /// No description provided for @l6RecoveryReadsTheOriginalInstallationS770c9.
  ///
  /// In zh, this message translates to:
  /// **'恢复只读取原安装的密钥，不会创建替代密钥。'**
  String get l6RecoveryReadsTheOriginalInstallationS770c9;

  /// No description provided for @l6StagingEntryDeletedButKeyCleanup3ce20.
  ///
  /// In zh, this message translates to:
  /// **'暂存项已删除，但本机密钥清理失败，可稍后在密钥健康里检查。'**
  String get l6StagingEntryDeletedButKeyCleanup3ce20;

  /// No description provided for @l6StagingEntryArg0Deleted0f067.
  ///
  /// In zh, this message translates to:
  /// **'已删除暂存项「{arg0}」。'**
  String l6StagingEntryArg0Deleted0f067(String arg0);

  /// No description provided for @l6OriginalInstallationProofResentWaitingForee8e7.
  ///
  /// In zh, this message translates to:
  /// **'已重发原安装证明，仍等待成员授权和初始同步；没有重新批准或重新生成密钥。'**
  String get l6OriginalInstallationProofResentWaitingForee8e7;

  /// No description provided for @l6OriginalRequestResentWaitingForExplicitb70c0.
  ///
  /// In zh, this message translates to:
  /// **'已按原请求重发，等待已有设备明确批准。'**
  String get l6OriginalRequestResentWaitingForExplicitb70c0;

  /// No description provided for @l6InstallationProofDeliveredWaitingForAuthorization69cc4.
  ///
  /// In zh, this message translates to:
  /// **'安装证明已投递；仍等待成员授权和初始同步，当前数据集没有切换。'**
  String get l6InstallationProofDeliveredWaitingForAuthorization69cc4;

  /// No description provided for @l6NoUsableApprovalYetVerifyAnd0b6c4.
  ///
  /// In zh, this message translates to:
  /// **'尚未收到可用审批，请在已有设备核对并批准。'**
  String get l6NoUsableApprovalYetVerifyAnd0b6c4;

  /// No description provided for @l6TheSameInstallationProofWasResent1abc4.
  ///
  /// In zh, this message translates to:
  /// **'已重发同一安装证明；投递不表示设备已加入或同步完成。'**
  String get l6TheSameInstallationProofWasResent1abc4;

  /// No description provided for @l6EnrollmentStoppedStagedDataAndKeys9b648.
  ///
  /// In zh, this message translates to:
  /// **'已停止本机加入流程，暂存库和密钥保留。临时投递权限会到期；本操作不撤销已发出的请求或可能已获批准的成员资格。'**
  String get l6EnrollmentStoppedStagedDataAndKeys9b648;

  /// No description provided for @l6DeviceKeysOrProtectedRevisionsAre5de96.
  ///
  /// In zh, this message translates to:
  /// **'缺少本机密钥或受保护修订，不能恢复；不会生成替代密钥。'**
  String get l6DeviceKeysOrProtectedRevisionsAre5de96;

  /// No description provided for @l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35.
  ///
  /// In zh, this message translates to:
  /// **'受保护修订发生回退，已停止加入；原库和暂存数据保留。'**
  String get l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35;

  /// No description provided for @l6DeviceKeyRecordsAreDamagedEnrollment76c86.
  ///
  /// In zh, this message translates to:
  /// **'本机密钥记录损坏，已停止加入；不会覆盖已有密钥。'**
  String get l6DeviceKeyRecordsAreDamagedEnrollment76c86;

  /// No description provided for @l6SecureKeyStorageCouldNotSave6f761.
  ///
  /// In zh, this message translates to:
  /// **'系统密钥库未能安全保存或回读，已停止加入；解锁系统密钥库后恢复暂存项。'**
  String get l6SecureKeyStorageCouldNotSave6f761;

  /// No description provided for @l6ThisWorkspaceHasAnActiveSync69142.
  ///
  /// In zh, this message translates to:
  /// **'工作区已有同步会话或后台任务，请先结束任务或锁定当前同步，再发起加入。'**
  String get l6ThisWorkspaceHasAnActiveSync69142;

  /// No description provided for @l6TheInvitationExpiredOrHasNo6c654.
  ///
  /// In zh, this message translates to:
  /// **'本次邀请已过期或没有可恢复的已审批检查点。请停止本次流程并获取新邀请；已有数据保留。'**
  String get l6TheInvitationExpiredOrHasNo6c654;

  /// No description provided for @l6TheRelayStepIsUnconfirmedCheck92aea.
  ///
  /// In zh, this message translates to:
  /// **'中继步骤未确认。可检查审批或重发原请求/已保存的证明，不要重新生成密钥。'**
  String get l6TheRelayStepIsUnconfirmedCheck92aea;

  /// No description provided for @l6EnrollmentIsIncompleteAndHasStopped1c448.
  ///
  /// In zh, this message translates to:
  /// **'加入步骤未完成，已停止继续推进。请检查原邀请、解锁状态和暂存项；当前数据集未切换。'**
  String get l6EnrollmentIsIncompleteAndHasStopped1c448;

  /// No description provided for @l6ConfirmThatTheJoiningDeviceMay8297c.
  ///
  /// In zh, this message translates to:
  /// **'请确认加入设备将获得全部 Provider 配置和 API Key。'**
  String get l6ConfirmThatTheJoiningDeviceMay8297c;

  /// No description provided for @l6TheInvitationIsValidFor245be17.
  ///
  /// In zh, this message translates to:
  /// **'邀请码 24 小时内有效。新设备扫码发起加入后，这里会自动收到请求，核对设备与指纹后批准即可。'**
  String get l6TheInvitationIsValidFor245be17;

  /// No description provided for @l6NoJoinRequestReceivedYet10542.
  ///
  /// In zh, this message translates to:
  /// **'尚未收到加入请求。'**
  String get l6NoJoinRequestReceivedYet10542;

  /// No description provided for @l6VerifyTheDeviceAndFingerprintBefore5e3e6.
  ///
  /// In zh, this message translates to:
  /// **'核对设备及指纹后再明确批准。'**
  String get l6VerifyTheDeviceAndFingerprintBefore5e3e6;

  /// No description provided for @l6ExplicitlyConfirmSharingProviderApiKeysd0cdd.
  ///
  /// In zh, this message translates to:
  /// **'请明确确认向这台设备共享 Provider API Key。'**
  String get l6ExplicitlyConfirmSharingProviderApiKeysd0cdd;

  /// No description provided for @l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8.
  ///
  /// In zh, this message translates to:
  /// **'审批已投递，等待新设备安装确认；这不表示设备已加入或同步完成。'**
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8;

  /// No description provided for @l6ApprovalDeliveredWaitingForInstallationConfirmation9b218.
  ///
  /// In zh, this message translates to:
  /// **'审批已投递，等待新设备安装确认。'**
  String get l6ApprovalDeliveredWaitingForInstallationConfirmation9b218;

  /// No description provided for @l6InvitationCanceled35e1b.
  ///
  /// In zh, this message translates to:
  /// **'本次邀请已取消。'**
  String get l6InvitationCanceled35e1b;

  /// No description provided for @l6ApprovalDeliveredWaitingForTheNew361e9.
  ///
  /// In zh, this message translates to:
  /// **'审批已投递，等待新设备完成安装确认。'**
  String get l6ApprovalDeliveredWaitingForTheNew361e9;

  /// No description provided for @l6TheInvitationOrReviewVersionExpireda53ec.
  ///
  /// In zh, this message translates to:
  /// **'邀请或审阅版本已过期，请重新发起邀请并核对设备。'**
  String get l6TheInvitationOrReviewVersionExpireda53ec;

  /// No description provided for @l6TheInvitationWasCanceledLocallyRelay360d8.
  ///
  /// In zh, this message translates to:
  /// **'本机已取消邀请，但中继撤销尚未确认；临时发送权限会随短期邀请到期。'**
  String get l6TheInvitationWasCanceledLocallyRelay360d8;

  /// No description provided for @l6ApprovalIsSavedButHasNot99508.
  ///
  /// In zh, this message translates to:
  /// **'批准已保存，但暂时没送到新设备（网络或中继不可用）。系统会自动重试，无需重新批准。'**
  String get l6ApprovalIsSavedButHasNot99508;

  /// No description provided for @l6BackgroundSyncIsUsingTheRuntime0a21c.
  ///
  /// In zh, this message translates to:
  /// **'后台同步正在占用本机同步运行时，本次配对写入未执行；请稍候再点一次。没有自动授权新设备。'**
  String get l6BackgroundSyncIsUsingTheRuntime0a21c;

  /// No description provided for @l6PairingIsIncompleteCheckDeviceUnlock85108.
  ///
  /// In zh, this message translates to:
  /// **'配对步骤未完成，请检查本机解锁状态和中继地址；没有自动授权新设备。'**
  String get l6PairingIsIncompleteCheckDeviceUnlock85108;

  /// No description provided for @l6UnlockThisDeviceSSyncKeysfc734.
  ///
  /// In zh, this message translates to:
  /// **'本机同步密钥已锁定：请先在上方解锁后审阅冲突。'**
  String get l6UnlockThisDeviceSSyncKeysfc734;

  /// No description provided for @l6LocalSettingsChangedReviewTheUpdated3cd59.
  ///
  /// In zh, this message translates to:
  /// **'本地配置已变化，请重新审阅下方最新列表。'**
  String get l6LocalSettingsChangedReviewTheUpdated3cd59;

  /// No description provided for @l6TheConflictChangedReviewTheUpdated936b5.
  ///
  /// In zh, this message translates to:
  /// **'冲突内容已变化，请重新审阅下方最新列表。'**
  String get l6TheConflictChangedReviewTheUpdated936b5;

  /// No description provided for @l6CouldNotReadDeviceSyncState278eb.
  ///
  /// In zh, this message translates to:
  /// **'无法读取本机同步状态；原有记录未改动。'**
  String get l6CouldNotReadDeviceSyncState278eb;

  /// No description provided for @l6DeviceSyncIsNotReady3fc30.
  ///
  /// In zh, this message translates to:
  /// **'本机同步尚未就绪。'**
  String get l6DeviceSyncIsNotReady3fc30;

  /// No description provided for @l6DeviceNamesMustContain12561d9b2.
  ///
  /// In zh, this message translates to:
  /// **'设备名需为 1–256 个字符。'**
  String get l6DeviceNamesMustContain12561d9b2;

  /// No description provided for @l6AnotherOperationUpdatedTheStateRefresh39e75.
  ///
  /// In zh, this message translates to:
  /// **'状态刚被其他操作更新，请刷新后重试。'**
  String get l6AnotherOperationUpdatedTheStateRefresh39e75;

  /// No description provided for @l6UnlockDeviceKeysBeforeRenaming10735.
  ///
  /// In zh, this message translates to:
  /// **'本机密钥已锁定，请先解锁再改名。'**
  String get l6UnlockDeviceKeysBeforeRenaming10735;

  /// No description provided for @l6TheDeviceNameContainsInvalidCharacters438ed.
  ///
  /// In zh, this message translates to:
  /// **'设备名包含无效字符，请换一个。'**
  String get l6TheDeviceNameContainsInvalidCharacters438ed;

  /// No description provided for @l6EnterADeviceNameAndConfirm02f54.
  ///
  /// In zh, this message translates to:
  /// **'填写设备名称，并确认配对将共享 Provider API Key。'**
  String get l6EnterADeviceNameAndConfirm02f54;

  /// No description provided for @l6StopEnrollmentBeforePreparingOrUnlocking4420c.
  ///
  /// In zh, this message translates to:
  /// **'请先停止本机加入流程，再准备或解锁当前数据集。'**
  String get l6StopEnrollmentBeforePreparingOrUnlocking4420c;

  /// No description provided for @l6LockingIsUnconfirmedTryAgainKeys6bf44.
  ///
  /// In zh, this message translates to:
  /// **'锁定操作未确认，请重试；不会重新生成密钥。'**
  String get l6LockingIsUnconfirmedTryAgainKeys6bf44;

  /// No description provided for @l6DeviceKeysOrProtectedRevisionsAre845e9.
  ///
  /// In zh, this message translates to:
  /// **'缺少本机密钥或受保护的修订记录。请通过原设备恢复；不会生成替代密钥。'**
  String get l6DeviceKeysOrProtectedRevisionsAre845e9;

  /// No description provided for @l6KeyRevisionsRolledBackUnlockingStoppedc58f3.
  ///
  /// In zh, this message translates to:
  /// **'密钥修订发生回退，已停止解锁。请检查恢复的数据，原记录保留。'**
  String get l6KeyRevisionsRolledBackUnlockingStoppedc58f3;

  /// No description provided for @l6SystemKeyRecordsAreDamagedUnlocking23c95.
  ///
  /// In zh, this message translates to:
  /// **'系统密钥记录损坏，已停止解锁。原数据和密钥不会被覆盖。'**
  String get l6SystemKeyRecordsAreDamagedUnlocking23c95;

  /// No description provided for @l6TheSystemKeyStoreIsUnavailable8d23d.
  ///
  /// In zh, this message translates to:
  /// **'系统密钥库不可用或已锁定。解锁系统密钥库后重试，仍可保存本地记录。'**
  String get l6TheSystemKeyStoreIsUnavailable8d23d;

  /// No description provided for @l6AnotherWindowOrBackgroundTaskIs610c2.
  ///
  /// In zh, this message translates to:
  /// **'另一个窗口或后台任务正在使用此工作区，请等待结束后重试。'**
  String get l6AnotherWindowOrBackgroundTaskIs610c2;

  /// No description provided for @l6BackgroundJobsAreStillRunningPrepare56569.
  ///
  /// In zh, this message translates to:
  /// **'后台任务仍在执行，完成后再准备同步；本次未切换加工归属。'**
  String get l6BackgroundJobsAreStillRunningPrepare56569;

  /// No description provided for @l6TheDeviceSyncIdentityNeedsRecoveryd2a55.
  ///
  /// In zh, this message translates to:
  /// **'本机同步身份需要恢复，已停止自动初始化。请保留原库并通过授权设备重新加入。'**
  String get l6TheDeviceSyncIdentityNeedsRecoveryd2a55;

  /// No description provided for @l6SyncPreparationDidNotFinishAnd1c512.
  ///
  /// In zh, this message translates to:
  /// **'本机同步准备未完成，已保持锁定。请重试；原记录保留。'**
  String get l6SyncPreparationDidNotFinishAnd1c512;

  /// No description provided for @l6AndMore93ac7.
  ///
  /// In zh, this message translates to:
  /// **' 等'**
  String get l6AndMore93ac7;

  /// No description provided for @l6SupportedServiceFxtwitterConvertsXCom0989a.
  ///
  /// In zh, this message translates to:
  /// **'当前支持的抓取服务：fxTwitter —— 把 x.com 推文链接自动解析为长文，\n接口 https://api.fxtwitter.com/status/<推文id>'**
  String get l6SupportedServiceFxtwitterConvertsXCom0989a;

  /// No description provided for @l6AndArg0MoreTools52a1a.
  ///
  /// In zh, this message translates to:
  /// **' 等 {arg0} 个工具'**
  String l6AndArg0MoreTools52a1a(String arg0);

  /// No description provided for @l6Arg0YouRecordedArg1Arg2Ac3209.
  ///
  /// In zh, this message translates to:
  /// **'{arg0}你记录过：「{arg1}」{arg2}——还记得这件事。'**
  String l6Arg0YouRecordedArg1Arg2Ac3209(String arg0, String arg1, String arg2);

  /// No description provided for @l6SupplyYourOwnEndpointNoneIsd855e.
  ///
  /// In zh, this message translates to:
  /// **'仅自定义、无内置默认。填了地址后，外网搜索会经 DoH+ECH 传输，在国内网络下也能到达被 DNS 污染 / SNI 封锁的服务（如 x.com）。当前仅作用于外网搜索。{arg0}'**
  String l6SupplyYourOwnEndpointNoneIsd855e(String arg0);

  /// No description provided for @l6Check40ffb.
  ///
  /// In zh, this message translates to:
  /// **'检查'**
  String get l6Check40ffb;

  /// No description provided for @l6LoadingDevices5c6b5.
  ///
  /// In zh, this message translates to:
  /// **'正在读取设备列表…'**
  String get l6LoadingDevices5c6b5;

  /// No description provided for @l6Arg0ProposalArg1PagesArg2Pages2ee4f.
  ///
  /// In zh, this message translates to:
  /// **'{arg0}方案 · {arg1} 页 → {arg2} 页'**
  String l6Arg0ProposalArg1PagesArg2Pages2ee4f(
    String arg0,
    String arg1,
    String arg2,
  );

  /// No description provided for @l6SourcesArg0EventsArg10f288.
  ///
  /// In zh, this message translates to:
  /// **'{arg0} 份原料 · {arg1} 个事件'**
  String l6SourcesArg0EventsArg10f288(String arg0, String arg1);

  /// No description provided for @l6SwitchedToArg0QuitAndReopenc2285.
  ///
  /// In zh, this message translates to:
  /// **'已切换到「{arg0}」。请完全退出并重新打开应用，即可看到同步的数据。'**
  String l6SwitchedToArg0QuitAndReopenc2285(String arg0);

  /// No description provided for @l6JoinedDatasetArg0ItsDataIs7a081.
  ///
  /// In zh, this message translates to:
  /// **'已加入数据集「{arg0}」，其数据尚未在此窗口生效。'**
  String l6JoinedDatasetArg0ItsDataIs7a081(String arg0);

  /// No description provided for @l6DatasetsReadyToOpenArg0c5aa4.
  ///
  /// In zh, this message translates to:
  /// **'发现 {arg0} 个待进入的数据集'**
  String l6DatasetsReadyToOpenArg0c5aa4(String arg0);

  /// No description provided for @l6DevicesOnlineArg0Arg1c5eda.
  ///
  /// In zh, this message translates to:
  /// **'设备 {arg0}/{arg1} 在线'**
  String l6DevicesOnlineArg0Arg1c5eda(String arg0, String arg1);

  /// No description provided for @l6HeartbeatHealthyPendingArg05c133.
  ///
  /// In zh, this message translates to:
  /// **'最近心跳正常 · 待发 {arg0}'**
  String l6HeartbeatHealthyPendingArg05c133(String arg0);

  /// No description provided for @l6FollowUpSuggestions99ccc.
  ///
  /// In zh, this message translates to:
  /// **'跟进建议'**
  String get l6FollowUpSuggestions99ccc;

  /// No description provided for @l6Guidance79851.
  ///
  /// In zh, this message translates to:
  /// **'指导'**
  String get l6Guidance79851;

  /// No description provided for @l6RelatedRuleRemindersArg05615f.
  ///
  /// In zh, this message translates to:
  /// **'相关规则提醒（{arg0}）'**
  String l6RelatedRuleRemindersArg05615f(String arg0);

  /// No description provided for @l6AwaitingAnalysis605dc.
  ///
  /// In zh, this message translates to:
  /// **'待分析'**
  String get l6AwaitingAnalysis605dc;

  /// No description provided for @l6RecordedAwaitingAnalysis8e5e7.
  ///
  /// In zh, this message translates to:
  /// **'已记录，待分析'**
  String get l6RecordedAwaitingAnalysis8e5e7;

  /// No description provided for @l6CurrentRevisionArg0beb34.
  ///
  /// In zh, this message translates to:
  /// **'当前修订：{arg0}'**
  String l6CurrentRevisionArg0beb34(String arg0);

  /// No description provided for @l6ResolvedWithALinkedPagef44a7.
  ///
  /// In zh, this message translates to:
  /// **'已关联解决'**
  String get l6ResolvedWithALinkedPagef44a7;

  /// No description provided for @l6Dismissedbd172.
  ///
  /// In zh, this message translates to:
  /// **'已忽略'**
  String get l6Dismissedbd172;

  /// No description provided for @l6OriginalSuggestiondb86c.
  ///
  /// In zh, this message translates to:
  /// **'原始建议'**
  String get l6OriginalSuggestiondb86c;

  /// No description provided for @l6OpenLinkedKnowledgePage88f20.
  ///
  /// In zh, this message translates to:
  /// **'打开对应知识页'**
  String get l6OpenLinkedKnowledgePage88f20;

  /// No description provided for @l6LoadProcessingQueue079ca.
  ///
  /// In zh, this message translates to:
  /// **'读取整理队列'**
  String get l6LoadProcessingQueue079ca;

  /// No description provided for @l6TotalArg0PendingArg1RunningArg2a5909.
  ///
  /// In zh, this message translates to:
  /// **'共 {arg0} 项任务 · 待处理 {arg1} · 处理中 {arg2} · 等待 {arg3} · 重试 {arg4} · 跳过 {arg5} · 完成 {arg6}'**
  String l6TotalArg0PendingArg1RunningArg2a5909(
    String arg0,
    String arg1,
    String arg2,
    String arg3,
    String arg4,
    String arg5,
    String arg6,
  );

  /// No description provided for @l6RefreshListbd82e.
  ///
  /// In zh, this message translates to:
  /// **'刷新列表'**
  String get l6RefreshListbd82e;

  /// No description provided for @l6FullDocumentReadingc1da8.
  ///
  /// In zh, this message translates to:
  /// **'全文阅读'**
  String get l6FullDocumentReadingc1da8;

  /// No description provided for @l6SourceProcessingfbebb.
  ///
  /// In zh, this message translates to:
  /// **'原料整理'**
  String get l6SourceProcessingfbebb;

  /// No description provided for @l6CrossSourceProcessing5240c.
  ///
  /// In zh, this message translates to:
  /// **'跨资料整理'**
  String get l6CrossSourceProcessing5240c;

  /// No description provided for @l6KnowledgeReview4f5bb.
  ///
  /// In zh, this message translates to:
  /// **'知识复核'**
  String get l6KnowledgeReview4f5bb;

  /// No description provided for @l6SourceRefresh65f83.
  ///
  /// In zh, this message translates to:
  /// **'来源更新'**
  String get l6SourceRefresh65f83;

  /// No description provided for @l6ScheduledArg020b73.
  ///
  /// In zh, this message translates to:
  /// **'\n计划时间：{arg0}'**
  String l6ScheduledArg020b73(String arg0);

  /// No description provided for @l6LastProcessedArg0NewEvidenceArg124fb9.
  ///
  /// In zh, this message translates to:
  /// **'最近整理 {arg0} · 新增证据 {arg1} 条'**
  String l6LastProcessedArg0NewEvidenceArg124fb9(String arg0, String arg1);

  /// No description provided for @l6RecentProcessingAndInsights23cc8.
  ///
  /// In zh, this message translates to:
  /// **'最近自动整理与洞察记录'**
  String get l6RecentProcessingAndInsights23cc8;

  /// No description provided for @l6LoadInsights25b9e.
  ///
  /// In zh, this message translates to:
  /// **'读取洞察记录'**
  String get l6LoadInsights25b9e;

  /// No description provided for @l6CheckedAutomaticallyInTheBackgroundAs34c09.
  ///
  /// In zh, this message translates to:
  /// **'积累事件后由后台自动检查，无需手动触发。'**
  String get l6CheckedAutomaticallyInTheBackgroundAs34c09;

  /// No description provided for @l6SourceUpdateCheckc6668.
  ///
  /// In zh, this message translates to:
  /// **'来源更新检查'**
  String get l6SourceUpdateCheckc6668;

  /// No description provided for @l6TopicProcessingAndChecks872c6.
  ///
  /// In zh, this message translates to:
  /// **'主题整理与检查'**
  String get l6TopicProcessingAndChecks872c6;

  /// No description provided for @l6LoadSourceEventsaaf09.
  ///
  /// In zh, this message translates to:
  /// **'读取来源事件'**
  String get l6LoadSourceEventsaaf09;

  /// No description provided for @l6LoadKnowledgeOutputs54f00.
  ///
  /// In zh, this message translates to:
  /// **'读取知识产出'**
  String get l6LoadKnowledgeOutputs54f00;

  /// No description provided for @l6CrossSourceTopicIncludesThisPageae2b4.
  ///
  /// In zh, this message translates to:
  /// **'跨资料主题 · 包含本页及相关原料'**
  String get l6CrossSourceTopicIncludesThisPageae2b4;

  /// No description provided for @l6AwaitingReview28cc1.
  ///
  /// In zh, this message translates to:
  /// **'待审阅'**
  String get l6AwaitingReview28cc1;

  /// No description provided for @l6ConfirmAndSave706df.
  ///
  /// In zh, this message translates to:
  /// **'确认保存'**
  String get l6ConfirmAndSave706df;

  /// No description provided for @l6ReferenceKnowledgeSaved962eb.
  ///
  /// In zh, this message translates to:
  /// **'已保存参考知识'**
  String get l6ReferenceKnowledgeSaved962eb;

  /// No description provided for @l6RecordAnOpinionb7461.
  ///
  /// In zh, this message translates to:
  /// **'记录观点'**
  String get l6RecordAnOpinionb7461;

  /// No description provided for @l6LoadSourcesAndRevisionsd1d70.
  ///
  /// In zh, this message translates to:
  /// **'读取来源与修订'**
  String get l6LoadSourcesAndRevisionsd1d70;

  /// No description provided for @l6UnnamedDevice008d5.
  ///
  /// In zh, this message translates to:
  /// **'未命名设备'**
  String get l6UnnamedDevice008d5;

  /// No description provided for @l6Memberc1ee9.
  ///
  /// In zh, this message translates to:
  /// **'成员'**
  String get l6Memberc1ee9;

  /// No description provided for @l6RuleReminders79233.
  ///
  /// In zh, this message translates to:
  /// **'规则提醒'**
  String get l6RuleReminders79233;

  /// No description provided for @l6Latest9d501.
  ///
  /// In zh, this message translates to:
  /// **'（最近一份）'**
  String get l6Latest9d501;

  /// No description provided for @l6IncubatingArg07051b.
  ///
  /// In zh, this message translates to:
  /// **' · {arg0} 孵化中'**
  String l6IncubatingArg07051b(String arg0);

  /// No description provided for @l6RejectedArg02115d.
  ///
  /// In zh, this message translates to:
  /// **' · {arg0} 已否定'**
  String l6RejectedArg02115d(String arg0);

  /// No description provided for @l6Arg0Unavailable85bdd.
  ///
  /// In zh, this message translates to:
  /// **' {arg0} 不可用'**
  String l6Arg0Unavailable85bdd(String arg0);

  /// No description provided for @l6UnlockToVerifycd78c.
  ///
  /// In zh, this message translates to:
  /// **'解锁后核验'**
  String get l6UnlockToVerifycd78c;

  /// No description provided for @l6WaitingForDependenciesArg08ff9b.
  ///
  /// In zh, this message translates to:
  /// **' · 等待依赖 {arg0}'**
  String l6WaitingForDependenciesArg08ff9b(String arg0);

  /// No description provided for @l6RosterResentdba01.
  ///
  /// In zh, this message translates to:
  /// **' · 名册已补发'**
  String get l6RosterResentdba01;

  /// No description provided for @l6VersionMismatchesArg02d8cc.
  ///
  /// In zh, this message translates to:
  /// **' · 版本不匹配 {arg0}'**
  String l6VersionMismatchesArg02d8cc(String arg0);

  /// No description provided for @l6ReceiptFailedf7d93.
  ///
  /// In zh, this message translates to:
  /// **' · 回执失败'**
  String get l6ReceiptFailedf7d93;

  /// No description provided for @l6OffDirectConnection371c7.
  ///
  /// In zh, this message translates to:
  /// **'关闭（强制直连）'**
  String get l6OffDirectConnection371c7;

  /// No description provided for @l6AutomaticPreferDohFallBackTo02ddb.
  ///
  /// In zh, this message translates to:
  /// **'自动（优先 DoH，失败回退直连）'**
  String get l6AutomaticPreferDohFallBackTo02ddb;

  /// No description provided for @l6RequiredDohOnly763c2.
  ///
  /// In zh, this message translates to:
  /// **'强制（只用 DoH，不降级）'**
  String get l6RequiredDohOnly763c2;

  /// No description provided for @l6HostedSearchOptimizedForAiWith46b67.
  ///
  /// In zh, this message translates to:
  /// **'托管服务，面向 AI 检索优化，免费额度'**
  String get l6HostedSearchOptimizedForAiWith46b67;

  /// No description provided for @l6HostedSearchWithAnIndependentPrivacy51ea0.
  ///
  /// In zh, this message translates to:
  /// **'托管服务，隐私导向的独立索引'**
  String get l6HostedSearchWithAnIndependentPrivacy51ea0;

  /// No description provided for @l6SelfHostedMetasearchNoApiKey0926f.
  ///
  /// In zh, this message translates to:
  /// **'自托管元搜索，无需 API key'**
  String get l6SelfHostedMetasearchNoApiKey0926f;

  /// No description provided for @l6Factsb27b7.
  ///
  /// In zh, this message translates to:
  /// **'事实'**
  String get l6Factsb27b7;

  /// No description provided for @l6Decisions13750.
  ///
  /// In zh, this message translates to:
  /// **'决定'**
  String get l6Decisions13750;

  /// No description provided for @l6Unfinishedb61b0.
  ///
  /// In zh, this message translates to:
  /// **'未完成'**
  String get l6Unfinishedb61b0;

  /// No description provided for @l6IdeasAndDecisionsa2d17.
  ///
  /// In zh, this message translates to:
  /// **'想法与决定'**
  String get l6IdeasAndDecisionsa2d17;

  /// No description provided for @l6PeopleAndProjectse73dc.
  ///
  /// In zh, this message translates to:
  /// **'人与事'**
  String get l6PeopleAndProjectse73dc;

  /// No description provided for @l6FollowUps068c6.
  ///
  /// In zh, this message translates to:
  /// **'后续'**
  String get l6FollowUps068c6;

  /// No description provided for @l6Helpful2f55f.
  ///
  /// In zh, this message translates to:
  /// **'有用'**
  String get l6Helpful2f55f;

  /// No description provided for @l6NotRelevanta8361.
  ///
  /// In zh, this message translates to:
  /// **'不相关'**
  String get l6NotRelevanta8361;

  /// No description provided for @l6ContentContainingProtocolMarkersIsHidden50ad8.
  ///
  /// In zh, this message translates to:
  /// **'含协议标记的内容未在进度中展开，请查看原始记录。'**
  String get l6ContentContainingProtocolMarkersIsHidden50ad8;

  /// No description provided for @l6Excerptfba71.
  ///
  /// In zh, this message translates to:
  /// **'…（片段）'**
  String get l6Excerptfba71;

  /// No description provided for @l6ThisStepFailedCheckTheFinal1ac2e.
  ///
  /// In zh, this message translates to:
  /// **'此步骤未成功；请查看最终状态和审阅回执，不会自动重复执行。'**
  String get l6ThisStepFailedCheckTheFinal1ac2e;

  /// No description provided for @l6NoCompletionReceiptCouldBeVerifiede00f1.
  ///
  /// In zh, this message translates to:
  /// **'没有可核实的完成回执；请检查实际结果，不会自动重做。'**
  String get l6NoCompletionReceiptCouldBeVerifiede00f1;

  /// No description provided for @l6SeeTheReviewCardBelowFord5a60.
  ///
  /// In zh, this message translates to:
  /// **'具体内容、目标版本和接受/拒绝操作见下方审阅卡。'**
  String get l6SeeTheReviewCardBelowFord5a60;

  /// No description provided for @l6EvidenceAtRetrievalArg0cb747.
  ///
  /// In zh, this message translates to:
  /// **'检索时的证据片段 · {arg0}'**
  String l6EvidenceAtRetrievalArg0cb747(String arg0);

  /// No description provided for @l6SourceArg0VersionArg1SnapshotArg2d0032.
  ///
  /// In zh, this message translates to:
  /// **'来源：{arg0} · 版本 {arg1} · 快照 {arg2}'**
  String l6SourceArg0VersionArg1SnapshotArg2d0032(
    String arg0,
    String arg1,
    String arg2,
  );

  /// No description provided for @l6PageHashAtRetrievalArg05808a.
  ///
  /// In zh, this message translates to:
  /// **'检索时页面依据：{arg0}'**
  String l6PageHashAtRetrievalArg05808a(String arg0);

  /// No description provided for @l6UserRecordArg0Arg1902d4.
  ///
  /// In zh, this message translates to:
  /// **'用户记录 {arg0}：{arg1}'**
  String l6UserRecordArg0Arg1902d4(String arg0, String arg1);

  /// No description provided for @l6UnverifiedPageArg01a6ef.
  ///
  /// In zh, this message translates to:
  /// **'未核验页面 · {arg0}'**
  String l6UnverifiedPageArg01a6ef(String arg0);

  /// No description provided for @l6NoEvidenceExcerptsWereReturned013a1.
  ///
  /// In zh, this message translates to:
  /// **'没有返回可展开的证据片段。'**
  String get l6NoEvidenceExcerptsWereReturned013a1;

  /// No description provided for @l6TheseAreTheResultsReturnedBy46276.
  ///
  /// In zh, this message translates to:
  /// **'以上是本次查询返回的数据；来源可能随后变化，最终回答引用另行核验。'**
  String get l6TheseAreTheResultsReturnedBy46276;

  /// No description provided for @l6GeneratingAReplyItContinuesWhence0b9.
  ///
  /// In zh, this message translates to:
  /// **'正在生成回复；切走也能继续，完成后自动停止。'**
  String get l6GeneratingAReplyItContinuesWhence0b9;

  /// No description provided for @l6CouldNotLoadGenerationStatusReconnect2eabb.
  ///
  /// In zh, this message translates to:
  /// **'无法读取生成状态，请重新连接；不会自动重新生成。'**
  String get l6CouldNotLoadGenerationStatusReconnect2eabb;

  /// No description provided for @l6TheStartResultIsUnconfirmedReconnecting3cc5d.
  ///
  /// In zh, this message translates to:
  /// **'启动结果待核实。重新连接会查询同一次请求，不会重复保存输入。'**
  String get l6TheStartResultIsUnconfirmedReconnecting3cc5d;

  /// No description provided for @l6TheStopRequestIsUnconfirmedReconnectdaf8d.
  ///
  /// In zh, this message translates to:
  /// **'停止请求尚未确认，请重新连接查看状态。'**
  String get l6TheStopRequestIsUnconfirmedReconnectdaf8d;

  /// No description provided for @l6GenerationStatusIsTemporarilyUnavailableSaved71c5d.
  ///
  /// In zh, this message translates to:
  /// **'生成状态暂不可用；已保存的输入不会重发。'**
  String get l6GenerationStatusIsTemporarilyUnavailableSaved71c5d;

  /// No description provided for @biometricReason.
  ///
  /// In zh, this message translates to:
  /// **'验证指纹或面容以解锁 Elsewhen'**
  String get biometricReason;

  /// No description provided for @l6Code33246.
  ///
  /// In zh, this message translates to:
  /// **'代码'**
  String get l6Code33246;

  /// No description provided for @l6CodeCopied8df90.
  ///
  /// In zh, this message translates to:
  /// **'代码已复制'**
  String get l6CodeCopied8df90;

  /// No description provided for @l6HeadingArg01576c.
  ///
  /// In zh, this message translates to:
  /// **'标题 {arg0}'**
  String l6HeadingArg01576c(String arg0);

  /// No description provided for @l6Heading186e3d.
  ///
  /// In zh, this message translates to:
  /// **'一级标题'**
  String get l6Heading186e3d;

  /// No description provided for @l6Heading222dfe.
  ///
  /// In zh, this message translates to:
  /// **'二级标题'**
  String get l6Heading222dfe;

  /// No description provided for @l6Heading3516f9.
  ///
  /// In zh, this message translates to:
  /// **'三级标题'**
  String get l6Heading3516f9;

  /// No description provided for @l6Boldf670b.
  ///
  /// In zh, this message translates to:
  /// **'加粗'**
  String get l6Boldf670b;

  /// No description provided for @l6Italicaf5a2.
  ///
  /// In zh, this message translates to:
  /// **'斜体'**
  String get l6Italicaf5a2;

  /// No description provided for @l6Underline9bc18.
  ///
  /// In zh, this message translates to:
  /// **'下划线'**
  String get l6Underline9bc18;

  /// No description provided for @l6Strikethroughc85de.
  ///
  /// In zh, this message translates to:
  /// **'删除线'**
  String get l6Strikethroughc85de;

  /// No description provided for @l6InlineCode1a813.
  ///
  /// In zh, this message translates to:
  /// **'行内代码'**
  String get l6InlineCode1a813;

  /// No description provided for @l6BulletedListb9e9a.
  ///
  /// In zh, this message translates to:
  /// **'无序列表'**
  String get l6BulletedListb9e9a;

  /// No description provided for @l6NumberedList0deb8.
  ///
  /// In zh, this message translates to:
  /// **'有序列表'**
  String get l6NumberedList0deb8;

  /// No description provided for @l6Quote23c0e.
  ///
  /// In zh, this message translates to:
  /// **'引用'**
  String get l6Quote23c0e;

  /// No description provided for @l6Mergee5bad.
  ///
  /// In zh, this message translates to:
  /// **'合并'**
  String get l6Mergee5bad;

  /// No description provided for @l6Split726ca.
  ///
  /// In zh, this message translates to:
  /// **'拆分'**
  String get l6Split726ca;

  /// No description provided for @l6Reprocess796dd.
  ///
  /// In zh, this message translates to:
  /// **'重整理'**
  String get l6Reprocess796dd;

  /// No description provided for @l6WhyIsThisExcerptUnreliableOra83f2.
  ///
  /// In zh, this message translates to:
  /// **'这条摘录为什么不可信/不可用？原因会成为负知识一部分，下次同源导入会提前提示。'**
  String get l6WhyIsThisExcerptUnreliableOra83f2;

  /// No description provided for @l6Discardea489.
  ///
  /// In zh, this message translates to:
  /// **'丢弃'**
  String get l6Discardea489;

  /// No description provided for @l6Processing1a5c2.
  ///
  /// In zh, this message translates to:
  /// **'正在整理…'**
  String get l6Processing1a5c2;

  /// No description provided for @l6CheckForUpdatesa6df3.
  ///
  /// In zh, this message translates to:
  /// **'检查更新'**
  String get l6CheckForUpdatesa6df3;

  /// No description provided for @l6ReprocessThisPaged752f.
  ///
  /// In zh, this message translates to:
  /// **'重新整理此页'**
  String get l6ReprocessThisPaged752f;

  /// No description provided for @l6Unknownd9c32.
  ///
  /// In zh, this message translates to:
  /// **'未知'**
  String get l6Unknownd9c32;

  /// No description provided for @l6RetryProcessingf8833.
  ///
  /// In zh, this message translates to:
  /// **'重试整理'**
  String get l6RetryProcessingf8833;

  /// No description provided for @deviationNoProgress.
  ///
  /// In zh, this message translates to:
  /// **'无进展'**
  String get deviationNoProgress;

  /// No description provided for @deviationStalled.
  ///
  /// In zh, this message translates to:
  /// **'停滞'**
  String get deviationStalled;

  /// No description provided for @deviationConflict.
  ///
  /// In zh, this message translates to:
  /// **'相悖'**
  String get deviationConflict;

  /// No description provided for @deviationScopeDrift.
  ///
  /// In zh, this message translates to:
  /// **'目标偏移'**
  String get deviationScopeDrift;

  /// No description provided for @initialSnapshotTransfer.
  ///
  /// In zh, this message translates to:
  /// **'初始快照传输'**
  String get initialSnapshotTransfer;

  /// No description provided for @legacyPeopleProjects.
  ///
  /// In zh, this message translates to:
  /// **'联系人/项目'**
  String get legacyPeopleProjects;

  /// No description provided for @legacyArtifacts.
  ///
  /// In zh, this message translates to:
  /// **'派生产物'**
  String get legacyArtifacts;

  /// No description provided for @legacyPlanPending.
  ///
  /// In zh, this message translates to:
  /// **'待定'**
  String get legacyPlanPending;

  /// No description provided for @legacyPlanActive.
  ///
  /// In zh, this message translates to:
  /// **'推进中'**
  String get legacyPlanActive;

  /// No description provided for @legacyNewConversation.
  ///
  /// In zh, this message translates to:
  /// **'新对话'**
  String get legacyNewConversation;

  /// No description provided for @reprocessingResult.
  ///
  /// In zh, this message translates to:
  /// **'已重整：请查看处理结果'**
  String get reprocessingResult;

  /// No description provided for @processingDetails.
  ///
  /// In zh, this message translates to:
  /// **'处理说明'**
  String get processingDetails;

  /// No description provided for @errorKnowledgeSourceChanged.
  ///
  /// In zh, this message translates to:
  /// **'来源已更新或被拒绝，请重新生成建议'**
  String get errorKnowledgeSourceChanged;

  /// No description provided for @connectionSucceeded.
  ///
  /// In zh, this message translates to:
  /// **'连接成功'**
  String get connectionSucceeded;

  /// No description provided for @syncLockedBacklog.
  ///
  /// In zh, this message translates to:
  /// **'待解锁积压 {count} 条 · {size}，解开本机密钥后自动同步'**
  String syncLockedBacklog(int count, String size);

  /// No description provided for @syncAwaitingPeer.
  ///
  /// In zh, this message translates to:
  /// **'同步延迟：对端未确认 {count} 条 · {size}，正在重试'**
  String syncAwaitingPeer(int count, String size);

  /// No description provided for @syncGoUnlock.
  ///
  /// In zh, this message translates to:
  /// **'去解锁'**
  String get syncGoUnlock;

  /// No description provided for @legacyOperationFailed.
  ///
  /// In zh, this message translates to:
  /// **'{action}失败，请重试一次'**
  String legacyOperationFailed(String action);

  /// No description provided for @pageReprocessed.
  ///
  /// In zh, this message translates to:
  /// **'已重整：正文按全部 {count} 条来源事件增量修补'**
  String pageReprocessed(int count);

  /// No description provided for @pageHumanProtected.
  ///
  /// In zh, this message translates to:
  /// **'页面有人工编辑保护，正文不动；如需机器改写请走「提炼/建议」审阅通道'**
  String get pageHumanProtected;

  /// No description provided for @processingNotes.
  ///
  /// In zh, this message translates to:
  /// **'处理说明'**
  String get processingNotes;

  /// No description provided for @queueDetailPending.
  ///
  /// In zh, this message translates to:
  /// **'等待后台处理'**
  String get queueDetailPending;

  /// No description provided for @queueDetailRunning.
  ///
  /// In zh, this message translates to:
  /// **'正在处理'**
  String get queueDetailRunning;

  /// No description provided for @queueDetailWaiting.
  ///
  /// In zh, this message translates to:
  /// **'等待来源阅读或上游审阅完成'**
  String get queueDetailWaiting;

  /// No description provided for @queueDetailRetry.
  ///
  /// In zh, this message translates to:
  /// **'处理失败，等待自动重试'**
  String get queueDetailRetry;

  /// No description provided for @queueDetailSkipped.
  ///
  /// In zh, this message translates to:
  /// **'已跳过；可查看原因'**
  String get queueDetailSkipped;

  /// No description provided for @queueDetailCompleted.
  ///
  /// In zh, this message translates to:
  /// **'处理完成'**
  String get queueDetailCompleted;

  /// No description provided for @aiReplyFailedGeneric.
  ///
  /// In zh, this message translates to:
  /// **'AI 回复失败，请重试或查看错误详情。'**
  String get aiReplyFailedGeneric;

  /// No description provided for @manualLabel.
  ///
  /// In zh, this message translates to:
  /// **'手动'**
  String get manualLabel;

  /// No description provided for @automaticLabel.
  ///
  /// In zh, this message translates to:
  /// **'自动'**
  String get automaticLabel;

  /// No description provided for @activeProviderSuffix.
  ///
  /// In zh, this message translates to:
  /// **'（当前激活）'**
  String get activeProviderSuffix;

  /// No description provided for @nodesAdvanced.
  ///
  /// In zh, this message translates to:
  /// **'推进 {count} 节点'**
  String nodesAdvanced(int count);

  /// No description provided for @errorKnowledgeReviewChanged.
  ///
  /// In zh, this message translates to:
  /// **'页面、来源或规则设置已变化，请重新审阅。'**
  String get errorKnowledgeReviewChanged;

  /// No description provided for @digestReadingPhase.
  ///
  /// In zh, this message translates to:
  /// **'正在分段阅读'**
  String get digestReadingPhase;

  /// No description provided for @digestMergingPhase.
  ///
  /// In zh, this message translates to:
  /// **'正在合并已读片段'**
  String get digestMergingPhase;

  /// No description provided for @digestReadyPhase.
  ///
  /// In zh, this message translates to:
  /// **'全文已读完 · 等待最终整理'**
  String get digestReadyPhase;

  /// No description provided for @digestCancelledPhase.
  ///
  /// In zh, this message translates to:
  /// **'事件分类变化，阅读已暂停'**
  String get digestCancelledPhase;

  /// No description provided for @digestReadingProgress.
  ///
  /// In zh, this message translates to:
  /// **'已读 {read} / {total} 字 · {leaves} 个片段 · {merges} 次合并'**
  String digestReadingProgress(
    String read,
    String total,
    String leaves,
    String merges,
  );

  /// No description provided for @digestReadingBudgetHelp.
  ///
  /// In zh, this message translates to:
  /// **'等待具有足够上下文容量的渠道。请检查上下文窗口、输出预留和自定义指令。已读进度保留。'**
  String get digestReadingBudgetHelp;

  /// No description provided for @digestReadingRetryHelp.
  ///
  /// In zh, this message translates to:
  /// **'本步骤未完成，稍后自动重试；已读进度保留。'**
  String get digestReadingRetryHelp;

  /// No description provided for @digestReadingLimitHelp.
  ///
  /// In zh, this message translates to:
  /// **'事件或阅读缓存达到处理上限。原文和已读进度均已保留。'**
  String get digestReadingLimitHelp;

  /// No description provided for @promptCompassContract.
  ///
  /// In zh, this message translates to:
  /// **'航向评估协议'**
  String get promptCompassContract;

  /// No description provided for @compassEvidence.
  ///
  /// In zh, this message translates to:
  /// **'查看依据'**
  String get compassEvidence;

  /// No description provided for @compassStillProgressing.
  ///
  /// In zh, this message translates to:
  /// **'仍在推进'**
  String get compassStillProgressing;

  /// No description provided for @compassDirectionChanged.
  ///
  /// In zh, this message translates to:
  /// **'方向已变'**
  String get compassDirectionChanged;

  /// No description provided for @compassFeedbackSaved.
  ///
  /// In zh, this message translates to:
  /// **'已记下反馈，后续判断会参考你的纠正'**
  String get compassFeedbackSaved;

  /// No description provided for @compassReview.
  ///
  /// In zh, this message translates to:
  /// **'简单复盘'**
  String get compassReview;

  /// No description provided for @compassReviewSeed.
  ///
  /// In zh, this message translates to:
  /// **'我们简单复盘当前计划是否仍合适。提醒依据：'**
  String get compassReviewSeed;

  /// No description provided for @compassEvidenceNotice.
  ///
  /// In zh, this message translates to:
  /// **'以下是当时用于判断的用户记录节选；完整原始记录保持不变。'**
  String get compassEvidenceNotice;

  /// No description provided for @compassCorrectionNote.
  ///
  /// In zh, this message translates to:
  /// **'可以补充进展或变化（选填）'**
  String get compassCorrectionNote;

  /// No description provided for @compassGentleCheck.
  ///
  /// In zh, this message translates to:
  /// **'当前计划还合适吗？'**
  String get compassGentleCheck;

  /// No description provided for @wikiAddedEvidence.
  ///
  /// In zh, this message translates to:
  /// **'本次新增依据'**
  String get wikiAddedEvidence;

  /// No description provided for @wikiRetainedEvidence.
  ///
  /// In zh, this message translates to:
  /// **'原有依据会保留；确认时会重新核验新增依据。'**
  String get wikiRetainedEvidence;

  /// No description provided for @wikiApplicabilityCheck.
  ///
  /// In zh, this message translates to:
  /// **'请核对适用条件与限制是否仍然合适。'**
  String get wikiApplicabilityCheck;

  /// No description provided for @datasetCleanupTitle.
  ///
  /// In zh, this message translates to:
  /// **'清理旧数据集副本'**
  String get datasetCleanupTitle;

  /// No description provided for @datasetCleanupHelp.
  ///
  /// In zh, this message translates to:
  /// **'仅列出未使用且未选中的已完成副本。请先锁定同步再清理；当前数据集、默认库和加入暂存项受保护。'**
  String get datasetCleanupHelp;

  /// No description provided for @datasetCleanupRefresh.
  ///
  /// In zh, this message translates to:
  /// **'刷新副本列表'**
  String get datasetCleanupRefresh;

  /// No description provided for @datasetCleanupEmpty.
  ///
  /// In zh, this message translates to:
  /// **'没有可清理的旧副本。'**
  String get datasetCleanupEmpty;

  /// No description provided for @datasetCleanupReview.
  ///
  /// In zh, this message translates to:
  /// **'查看并清理'**
  String get datasetCleanupReview;

  /// No description provided for @datasetCleanupConfirmTitle.
  ///
  /// In zh, this message translates to:
  /// **'删除此本机数据集副本？'**
  String get datasetCleanupConfirmTitle;

  /// No description provided for @datasetCleanupWarning.
  ///
  /// In zh, this message translates to:
  /// **'将永久删除此副本的全部本机数据及对应的本机同步密钥，无法撤销。其他设备及远端数据不会被删除，设备名册也不会改变。如有未同步的内容，请先备份。清理中断后可重试。'**
  String get datasetCleanupWarning;

  /// No description provided for @datasetCleanupDelete.
  ///
  /// In zh, this message translates to:
  /// **'永久删除本机副本'**
  String get datasetCleanupDelete;

  /// No description provided for @datasetCleanupSize.
  ///
  /// In zh, this message translates to:
  /// **'本机文件：{bytes} 字节'**
  String datasetCleanupSize(String bytes);

  /// No description provided for @datasetCleanupPending.
  ///
  /// In zh, this message translates to:
  /// **'清理未完成（文件或本机密钥），请重试。'**
  String get datasetCleanupPending;

  /// No description provided for @datasetCleanupRetry.
  ///
  /// In zh, this message translates to:
  /// **'继续清理'**
  String get datasetCleanupRetry;

  /// No description provided for @datasetCleanupFailed.
  ///
  /// In zh, this message translates to:
  /// **'清理未完成。请先锁定同步、关闭使用此副本的其他应用进程，再刷新列表重试；未完成的文件或密钥清理会保留待办。'**
  String get datasetCleanupFailed;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'zh'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'zh':
      return AppLocalizationsZh();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
