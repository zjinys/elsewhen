import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../utils/window_service.dart';
import '../bridge/rust_bridge_repository.dart';
import 'conversation_provider.dart';
import 'settings_provider.dart';

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

  // Initialize window manager
  await windowService.initialize();

  // Initialize Rust bridge
  final rustBridge = ref.read(storageRepositoryProvider);
  await rustBridge.initialize();

  // Resolve display/editor/backend language before exposing the main screen.
  await ref.read(settingsProvider.notifier).loadLanguageFromBridge();

  // 主对话流在首次运行时还不存在：建会话/改名是**写操作**，只在这里显式做
  // 一次；mainConversationProvider 保持纯读，不会因 invalidate 重跑写路径（P18）。
  await ref.read(conversationRepositoryProvider).ensureMainConversation();

  // 启动时直接选中主对话流。导航抽屉里的「最近使用」面板挂载时也会顺带
  // 选中，但抽屉可以是收起的（模态抽屉、停靠后被用户收起），那时面板不在
  // 树上；不在这里选中的话，启动后消息区只会停在「选择一个对话开始聊天」
  // 空态。
  final main = await ref
      .read(conversationRepositoryProvider)
      .findMainConversation();
  if (main != null) {
    ref.read(selectedConversationIdProvider.notifier).set(main.id);
  }

  debugPrint('App initialized');

  return true;
});
