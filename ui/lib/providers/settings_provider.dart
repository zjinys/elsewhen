import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../bridge/rust_bridge_repository.dart';
import '../models/settings.dart';

/// Settings state notifier
class SettingsNotifier extends StateNotifier<AppSettings> {
  SettingsNotifier(this._repo) : super(AppSettings.defaults());

  final RustBridgeRepository _repo;

  void updateAiProvider(AiProviderSettings provider) {
    state = state.copyWith(aiProvider: provider);
  }

  void updateMemory(MemorySettings memory) {
    state = state.copyWith(memory: memory);
  }

  void updateStorage(StorageSettings storage) {
    state = state.copyWith(storage: storage);
  }

  void updateTheme(AppThemeMode theme) {
    state = state.copyWith(themeMode: theme);
  }

  void updateThemePreset(AppThemePreset preset) {
    state = state.copyWith(themePreset: preset);
  }

  void updateLanguage(String? language) {
    state = state.copyWith(language: language);
  }

  /// 从 Rust app_meta 读取持久化的主题偏好（模式 + 配色预设）
  Future<void> loadThemeFromBridge() async {
    try {
      final prefs = await _repo.getThemePrefs();
      state = state.copyWith(
        themeMode: AppThemeMode.values.firstWhere(
          (m) => m.name == prefs.mode,
          orElse: () => AppThemeMode.dark,
        ),
        themePreset: AppThemePreset.fromName(prefs.preset),
      );
    } catch (e) {
      // 老库可能没有这两条 meta，保持默认即可
      debugPrint('loadThemeFromBridge failed: $e');
    }
  }

  /// 立即把当前主题偏好写入数据库（下拉变化即持久化）
  Future<void> saveTheme() async {
    await _repo.updateThemePrefs(
      mode: state.themeMode.name,
      preset: state.themePreset.name,
    );
  }

  /// 从 Rust 数据库读取当前生效的 AI provider 配置（首次从 .env 导入的那一份）
  Future<void> loadSettings() async {
    final cfg = await _repo.getAiProviderConfig();
    if (cfg != null) {
      state = state.copyWith(
        aiProvider: state.aiProvider.copyWith(
          providerType: cfg.providerType,
          baseUrl: cfg.baseUrl,
          model: cfg.model,
          apiKey: cfg.apiKey,
        ),
      );
    }
    // 主题偏好并行加载，不阻塞 AI 配置读取链路（main.dart 启动时也单独调用过）
    unawaited(loadThemeFromBridge());
  }
}

/// Global settings provider
final settingsProvider =
    StateNotifierProvider<SettingsNotifier, AppSettings>((ref) {
  return SettingsNotifier(
    ref.read(storageRepositoryProvider) as RustBridgeRepository,
  );
});