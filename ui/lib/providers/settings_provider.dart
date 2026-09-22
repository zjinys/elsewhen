import 'dart:async';
import 'dart:io';

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

  void updateFontName(String fontName) {
    state = state.copyWith(fontName: fontName);
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
        // 字体是自由字符串（内建值或系统字体族名），空值回退默认
        fontName: prefs.font.isEmpty ? 'inter' : prefs.font,
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
      font: state.fontName,
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

/// 系统字体族列表（Linux 经 `fc-list` 枚举，去重排序；其它平台/枚举失败返回空表）。
/// 设置页「外观 → 字体」下拉在内建四项之后追加这份列表。
final systemFontFamiliesProvider = FutureProvider<List<String>>((ref) async {
  if (!Platform.isLinux) return const [];
  try {
    final result = await Process.run('fc-list', [':', 'family']);
    if (result.exitCode != 0) return const [];
    final families = <String>{};
    for (final line in (result.stdout as String).split('\n')) {
      // 一行可能有多个别名（逗号分隔），取首个作为展示名
      final aliases = _splitFcListAliases(line);
      if (aliases.isEmpty) continue;
      final first = aliases.first;
      if (first.isNotEmpty) families.add(first);
    }
    return families.toList()
      ..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
  } catch (_) {
    return const [];
  }
});

/// 解析 fc-list 一行别名：逗号分隔但 `\,` 是转义；fontconfig 会用反斜杠
/// 转义家族名里的 `-` `,` `:` `\` 等字符（如 `FZSongS\-Extended`），需反转义。
List<String> _splitFcListAliases(String line) {
  final aliases = <String>[];
  final buf = StringBuffer();
  var escaped = false;
  for (final ch in line.split('')) {
    if (escaped) {
      buf.write(ch);
      escaped = false;
    } else if (ch == '\\') {
      escaped = true;
    } else if (ch == ',') {
      aliases.add(buf.toString().trim());
      buf.clear();
    } else {
      buf.write(ch);
    }
  }
  aliases.add(buf.toString().trim());
  return aliases;
}