/// Settings models for the application
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

/// Complete application settings
class AppSettings {
  final AiProviderSettings aiProvider;
  final MemorySettings memory;
  final StorageSettings storage;
  final AppThemeMode themeMode;
  final AppThemePreset themePreset;
  final String? language;

  const AppSettings({
    required this.aiProvider,
    required this.memory,
    required this.storage,
    this.themeMode = AppThemeMode.dark,
    this.themePreset = AppThemePreset.amber,
    this.language,
  });

  AppSettings copyWith({
    AiProviderSettings? aiProvider,
    MemorySettings? memory,
    StorageSettings? storage,
    AppThemeMode? themeMode,
    AppThemePreset? themePreset,
    String? language,
  }) {
    return AppSettings(
      aiProvider: aiProvider ?? this.aiProvider,
      memory: memory ?? this.memory,
      storage: storage ?? this.storage,
      themeMode: themeMode ?? this.themeMode,
      themePreset: themePreset ?? this.themePreset,
      language: language ?? this.language,
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
