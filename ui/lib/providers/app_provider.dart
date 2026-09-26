import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../utils/hotkey_service.dart';
import '../utils/window_service.dart';
import '../bridge/rust_bridge_repository.dart';
import 'conversation_provider.dart';

// Hotkey service provider
final hotkeyServiceProvider = Provider<HotkeyService>((ref) {
  return HotkeyService();
});

// Window service provider
final windowServiceProvider = Provider<WindowService>((ref) {
  return WindowService();
});

/// 是否已配置至少一个 AI provider（首次运行引导横幅的探测源）。
/// Rust 运行时在无任何配置时会直接失败（bail "No active AI provider
/// configuration"），AI 对话/自动分析全部不可用，故「配置列表非空」即视为就绪。
/// 设置页保存配置后由调用方 invalidate 刷新（见 ai_provider_setup_hint.dart）。
final aiProviderConfiguredProvider = FutureProvider<bool>((ref) async {
  final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  final configs = await repo.listAiProviderConfigs();
  return configs.isNotEmpty;
});

// Application initialization provider
final appInitializationProvider = FutureProvider<bool>((ref) async {
  final windowService = ref.read(windowServiceProvider);
  final hotkeyService = ref.read(hotkeyServiceProvider);

  // Initialize window manager
  await windowService.initialize();

  // Initialize Rust bridge
  final rustBridge = ref.read(storageRepositoryProvider);
  await rustBridge.initialize();

  // Initialize hotkey service (stub mode)
  await hotkeyService.initialize();

  // 主对话流在首次运行时还不存在：建会话/改名是**写操作**，只在这里显式做
  // 一次；mainConversationProvider 保持纯读，不会因 invalidate 重跑写路径（P18）。
  await ref.read(conversationRepositoryProvider).ensureMainConversation();

  debugPrint('App initialized');
  debugPrint('Session type: ${hotkeyService.sessionType}');
  debugPrint('\n${hotkeyService.getSetupInstructions()}');

  return true;
});
