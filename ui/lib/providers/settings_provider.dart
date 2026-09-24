import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../bridge/rust_bridge_repository.dart';
import '../models/settings.dart';
import '../utils/system_fonts.dart';

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

  void updateFontName(String fontName) {
    state = state.copyWith(fontName: fontName);
  }

  /// 全局字体切换：本地字体先预热 FontLoader 再落 state，避免主题闪一下默认字体。
  Future<void> setGlobalFont(String stored) async {
    await SystemFontService.instance.ensureLoadedForStored(stored);
    state = state.copyWith(fontName: stored);
  }

  void updateFontSize(double fontSize) {
    state = state.copyWith(fontSize: AppFonts.clampFontSize(fontSize));
  }

  /// 编辑器（内容区）覆盖层：三个参数各自可空，null = 跟随全局
  void updateEditorFont(String? fontName) {
    state = state.copyWithEditorSettings(font: fontName);
  }

  /// 编辑器字体覆盖切换：同全局，先预热再落 state。
  Future<void> setEditorFont(String? stored) async {
    await SystemFontService.instance.ensureLoadedForStored(stored);
    state = state.copyWithEditorSettings(font: stored);
  }

  void updateEditorFontSize(double? fontSize) {
    state = state.copyWithEditorSettings(
      fontSizeOverride:
          fontSize == null ? null : AppFonts.clampFontSize(fontSize),
    );
  }

  void updateEditorLineHeight(double? lineHeight) {
    state = state.copyWithEditorSettings(
      lineHeightOverride:
          lineHeight == null ? null : AppFonts.clampLineHeight(lineHeight),
    );
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
        // 归一化旧机器名存值（inter/notoSansSc/...）为 Google Fonts 家族名
        fontName: AppFonts.normalize(prefs.font),
        // 字号：老库无 theme_font_size 时 DTO 给默认 16.0，仍钳制越界值
        fontSize: AppFonts.clampFontSize(prefs.fontSize),
      );
      // 编辑器覆盖层：None = 跟随全局（保持 null），有值则归一化/钳制
      state = state.copyWithEditorSettings(
        font: prefs.editorFont == null
            ? null
            : AppFonts.normalize(prefs.editorFont!),
        fontSizeOverride: prefs.editorFontSize == null
            ? null
            : AppFonts.clampFontSize(prefs.editorFontSize!),
        lineHeightOverride: prefs.editorLineHeight == null
            ? null
            : AppFonts.clampLineHeight(prefs.editorLineHeight!),
      );
      // 本地字体预热：先建文件索引再落 state（避免主题闪默认字体），
      // 字体字节就绪后再触发一次重建（FontLoader 注册后需重建才生效）。
      await SystemFontService.instance.listFonts();
      await SystemFontService.instance.ensureLoadedForStored(state.fontName);
      await SystemFontService.instance
          .ensureLoadedForStored(state.editorFontName);
      state = state.copyWith();
    } catch (e) {
      // 老库可能没有这几条 meta，保持默认即可
      debugPrint('loadThemeFromBridge failed: $e');
    }
  }

  /// 立即把当前主题偏好写入数据库（下拉变化即持久化）
  Future<void> saveTheme() async {
    await _repo.updateThemePrefs(
      mode: state.themeMode.name,
      preset: state.themePreset.name,
      font: state.fontName,
      fontSize: state.fontSize,
      editorFont: state.editorFontName,
      editorFontSize: state.editorFontSize,
      editorLineHeight: state.editorLineHeight,
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
