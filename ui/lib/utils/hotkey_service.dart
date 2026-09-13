import 'package:flutter/foundation.dart';

// TODO: Re-enable after hotkey_manager_linux plugin is fixed
// Stub implementation to avoid compilation errors

class HotkeyService {
  static final HotkeyService _instance = HotkeyService._internal();
  factory HotkeyService() => _instance;
  HotkeyService._internal();

  // Callback when hotkey is pressed
  Function()? onCaptureTriggered;

  /// Initialize hotkey manager (currently disabled)
  Future<void> initialize() async {
    debugPrint('HotkeyService: disabled due to plugin compilation error');
  }

  /// Register global hotkey for capture mode (currently disabled)
  Future<bool> registerCaptureHotkey() async {
    debugPrint('HotkeyService: registerCaptureHotkey disabled');
    return false;
  }

  /// Unregister all hotkeys (currently disabled)
  Future<void> unregisterAll() async {
    debugPrint('HotkeyService: unregisterAll disabled');
  }

  /// Get current registered hotkey description
  String? getHotkeyDescription() {
    return null;
  }

  bool get isRegistered => false;
}
