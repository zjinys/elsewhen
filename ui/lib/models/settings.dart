/// Settings models for the application
import '../utils/system_fonts.dart';

class AiProviderSettings {
  final String providerType;
  final String baseUrl;
  final String model;
  final String apiKey;
  final double temperature;
  final int? maxTokens;

  const AiProviderSettings({
    required this.providerType,
    required this.baseUrl,
    required this.model,
    required this.apiKey,
    this.temperature = 0.7,
    this.maxTokens,
  });

  AiProviderSettings copyWith({
    String? providerType,
    String? baseUrl,
    String? model,
    String? apiKey,
    double? temperature,
    int? maxTokens,
  }) {
    return AiProviderSettings(
      providerType: providerType ?? this.providerType,
      baseUrl: baseUrl ?? this.baseUrl,
      model: model ?? this.model,
      apiKey: apiKey ?? this.apiKey,
      temperature: temperature ?? this.temperature,
      maxTokens: maxTokens ?? this.maxTokens,
    );
  }
}

/// Memory strategy settings
class MemorySettings {
  final String strategyType;
  final int? maxMessages;
  final int? maxTokens;

  const MemorySettings({
    required this.strategyType,
    this.maxMessages,
    this.maxTokens,
  });

  MemorySettings copyWith({
    String? strategyType,
    int? maxMessages,
    int? maxTokens,
  }) {
    return MemorySettings(
      strategyType: strategyType ?? this.strategyType,
      maxMessages: maxMessages ?? this.maxMessages,
      maxTokens: maxTokens ?? this.maxTokens,
    );
  }
}

/// Storage adapter settings
class StorageSettings {
  final String adapterType;
  final String? databasePath;

  const StorageSettings({
    required this.adapterType,
    this.databasePath,
  });

  StorageSettings copyWith({
    String? adapterType,
    String? databasePath,
  }) {
    return StorageSettings(
      adapterType: adapterType ?? this.adapterType,
      databasePath: databasePath ?? this.databasePath,
    );
  }
}

/// Application theme settings
enum AppThemeMode {
  light,
  dark,
  system;

  String get displayName {
    switch (this) {
      case AppThemeMode.light:
        return '浅色';
      case AppThemeMode.dark:
        return '深色';
      case AppThemeMode.system:
        return '跟随系统';
    }
  }
}

/// 主题预设（flex_color_scheme 色卡）：决定配色观感，与深浅模式正交
enum AppThemePreset {
  amber('amber', '琥珀'),
  indigo('indigo', '靛蓝'),
  aqua('aqua', '青碧'),
  violet('violet', '郁金');

  const AppThemePreset(this.name, this.displayName);

  /// 存库用的小写机器名
  final String name;
  final String displayName;

  static AppThemePreset fromName(String name) {
    return AppThemePreset.values.firstWhere(
      (p) => p.name == name,
      orElse: () => AppThemePreset.amber,
    );
  }
}

/// 全局字体选择（外观 tab，自研选择框：系统默认 / fontconfig 本地字体 / Google Fonts）。
/// 存值（app_meta `theme_font`）格式见 [parseStoredFont]：`system`、`google:<家族>`、
/// `local:<家族>`；无前缀历史值按 Google 优先、本地兜底解析。monospace 场景不受影响。
final class AppFonts {
  AppFonts._();

  /// 特殊存值：跟随系统默认字体
  static const String system = 'system';

  /// 默认字体家族名
  static const String defaultFont = 'Inter';

  static const String systemDisplayName = '系统默认';

  /// 归一化历史存值：旧机器名与空值映射到家族名；其它原样透传
  ///（主题层对非 Google Fonts 的未知值回退 [defaultFont]）。
  static String normalize(String stored) => switch (stored) {
        '' || 'inter' => defaultFont,
        'notoSansSc' => 'Noto Sans SC',
        'notoSerifSc' => 'Noto Serif SC',
        _ => stored,
      };

  /// 显示名：system 取中文名，带前缀存值取家族名，历史裸值原样显示
  static String displayNameOf(String fontName) =>
      parseStoredFont(fontName).family.isEmpty
          ? systemDisplayName
          : parseStoredFont(fontName).family;

  /// 正文字号范围与默认值（仅知识库正文，见设置页「外观」字号滑块）
  static const double minFontSize = 12.0;
  static const double maxFontSize = 24.0;
  static const double defaultFontSize = 16.0;

  /// 把历史/越界存值收敛到合法区间
  static double clampFontSize(double size) =>
      size.clamp(minFontSize, maxFontSize);

  /// 正文行距范围与默认值（倍数；vendor `TextStyleConfiguration` 默认 1.5，
  /// 见编辑器「AA」浮层行距滑块）
  static const double minLineHeight = 1.0;
  static const double maxLineHeight = 2.5;
  static const double defaultLineHeight = 1.5;

  /// 把历史/越界行距收敛到合法区间
  static double clampLineHeight(double height) =>
      height.clamp(minLineHeight, maxLineHeight);
}

/// Complete application settings
class AppSettings {
  final AiProviderSettings aiProvider;
  final MemorySettings memory;
  final StorageSettings storage;
  final AppThemeMode themeMode;
  final AppThemePreset themePreset;

  /// 字体机器名：内建值见 [AppFontFamily]，其余按系统字体族名解析
  final String fontName;

  /// 知识库正文字号（px，仅作用于 wiki 内容编辑器），见 [AppFonts]
  final double fontSize;

  /// 编辑器（内容区）阅读参数覆盖层——null 即「跟随全局」。
  /// 两层覆盖模型见 docs/notes/proposed/product/2026-09-23-editor-reading-settings-layer.md：
  /// `实际值 = 编辑器值(可空) ?? 全局值`，三个参数各自独立可空。
  /// 存储 app_meta：theme_editor_font / _font_size / _line_height。
  final String? editorFontName;
  final double? editorFontSize;
  final double? editorLineHeight;

  /// 是否设置过编辑器字体覆盖（AA 浮层需要区分「未设置」与「已重置」）
  bool get hasEditorOverrides =>
      editorFontName != null || editorFontSize != null || editorLineHeight != null;

  /// 求值：编辑器字号覆盖 ?? 全局字号（知识库正文实际渲染字号）
  double get contentFontSize =>
      AppFonts.clampFontSize(editorFontSize ?? fontSize);

  /// 求值：编辑器行距覆盖 ?? 全局行距（vendor 默认 1.5）
  double get contentLineHeight =>
      AppFonts.clampLineHeight(editorLineHeight ?? AppFonts.defaultLineHeight);

  /// 编辑器实际字体来源：null / "system" 时跟随系统字体，不做家族注入
  /// （渲染层对 null 回退主题/系统字体）；具体家族名则覆盖全局。
  String? get contentFontName => editorFontName;

  final String? language;

  /// copyWith 的「显式置空」哨兵（null 会被 copyWith 语义吞掉；
  /// const 实例保证能作默认参数值）
  static const Object _unset = _EditorUnsetSentinel();

  const AppSettings({
    required this.aiProvider,
    required this.memory,
    required this.storage,
    this.themeMode = AppThemeMode.dark,
    this.themePreset = AppThemePreset.amber,
    this.fontName = 'inter',
    this.fontSize = AppFonts.defaultFontSize,
    this.editorFontName,
    this.editorFontSize,
    this.editorLineHeight,
    this.language,
  });

  AppSettings copyWith({
    AiProviderSettings? aiProvider,
    MemorySettings? memory,
    StorageSettings? storage,
    AppThemeMode? themeMode,
    AppThemePreset? themePreset,
    String? fontName,
    double? fontSize,
    String? language,
  }) {
    return AppSettings(
      aiProvider: aiProvider ?? this.aiProvider,
      memory: memory ?? this.memory,
      storage: storage ?? this.storage,
      themeMode: themeMode ?? this.themeMode,
      themePreset: themePreset ?? this.themePreset,
      fontName: fontName ?? this.fontName,
      fontSize: fontSize ?? this.fontSize,
      language: language ?? this.language,
      editorFontName: editorFontName,
      editorFontSize: editorFontSize,
      editorLineHeight: editorLineHeight,
    );
  }

  /// 只改编辑器覆盖层：参数为 [Object]，传入具体值即设置覆盖；
  /// 传入 `null`（非 [_unset]）即「跟随全局」显式清空覆盖。
  AppSettings copyWithEditorSettings({
    Object? font = _unset,
    Object? fontSizeOverride = _unset,
    Object? lineHeightOverride = _unset,
  }) {
    return AppSettings(
      aiProvider: aiProvider,
      memory: memory,
      storage: storage,
      themeMode: themeMode,
      themePreset: themePreset,
      fontName: fontName,
      fontSize: fontSize,
      language: language,
      editorFontName: identical(font, _unset)
          ? editorFontName
          : font as String?,
      editorFontSize: identical(fontSizeOverride, _unset)
          ? editorFontSize
          : (fontSizeOverride as num?) == null
              ? null
              : AppFonts.clampFontSize((fontSizeOverride as num).toDouble()),
      editorLineHeight: identical(lineHeightOverride, _unset)
          ? editorLineHeight
          : (lineHeightOverride as num?) == null
              ? null
              : AppFonts.clampLineHeight(
                  (lineHeightOverride as num).toDouble(),
                ),
    );
  }

  factory AppSettings.defaults() {
    return AppSettings(
      aiProvider: const AiProviderSettings(
        providerType: 'openai-compatible',
        baseUrl: 'https://api.openai.com/v1',
        model: 'gpt-3.5-turbo',
        apiKey: '',
        temperature: 0.7,
      ),
      memory: const MemorySettings(
        strategyType: 'simple',
        maxMessages: 20,
      ),
      storage: const StorageSettings(
        adapterType: 'sqlite',
      ),
      themeMode: AppThemeMode.dark,
      themePreset: AppThemePreset.amber,
    );
  }
}

/// copyWithEditorSettings 的「显式置空」哨兵类型（const 实例才能作参数默认值）
class _EditorUnsetSentinel {
  const _EditorUnsetSentinel();
}
