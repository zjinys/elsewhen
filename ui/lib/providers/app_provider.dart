import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../utils/hotkey_service.dart';
import '../utils/window_service.dart';
import '../bridge/rust_bridge_repository.dart';

// Hotkey service provider
final hotkeyServiceProvider = Provider<HotkeyService>((ref) {
  return HotkeyService();
});

// Window service provider
final windowServiceProvider = Provider<WindowService>((ref) {
  return WindowService();
});

// Application initialization provider
final appInitializationProvider = FutureProvider<bool>((ref) async {
  // Initialize window manager
  final windowService = ref.read(windowServiceProvider);
  await windowService.initialize();

  // Initialize hotkey manager
  // TODO: Re-enable after hotkey_manager_linux plugin is fixed
  // final hotkeyService = ref.read(hotkeyServiceProvider);
  // await hotkeyService.initialize();

  // Initialize Rust bridge
  final rustBridge = ref.read(rustBridgeRepositoryProvider);
  await rustBridge.initialize();

  // Register capture hotkey (Ctrl+Space)
  // TODO: Re-enable after hotkey_manager_linux plugin is fixed
  // hotkeyService.onCaptureTriggered = () async {
  //   // Switch to capture mode when hotkey is pressed
  //   await windowService.switchToCaptureMode();
  // };
  //
  // final registered = await hotkeyService.registerCaptureHotkey();
  // if (registered) {
  //   debugPrint('Global hotkey registered: ${hotkeyService.getHotkeyDescription()}');
  // }

  debugPrint('App initialized (hotkey disabled due to plugin issue)');

  return true;
});
