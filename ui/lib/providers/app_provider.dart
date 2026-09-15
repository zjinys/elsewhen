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
  final windowService = ref.read(windowServiceProvider);
  final hotkeyService = ref.read(hotkeyServiceProvider);

  // Initialize window manager
  await windowService.initialize();

  // Initialize Rust bridge
  final rustBridge = ref.read(storageRepositoryProvider);
  await rustBridge.initialize();

  // Initialize hotkey service (stub mode)
  await hotkeyService.initialize();

  debugPrint('App initialized');
  debugPrint('Session type: ${hotkeyService.sessionType}');
  debugPrint('\n${hotkeyService.getSetupInstructions()}');

  return true;
});
