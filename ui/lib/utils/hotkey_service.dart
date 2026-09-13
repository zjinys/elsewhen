import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:hotkey_manager/hotkey_manager.dart';
import 'dart:io';

class HotkeyService {
  static final HotkeyService _instance = HotkeyService._internal();
  factory HotkeyService() => _instance;
  HotkeyService._internal();

  HotKey? _captureHotkey;
  bool _isRegistered = false;

  // Callback when hotkey is pressed
  Function()? onCaptureTriggered;

  /// Initialize hotkey manager
  Future<void> initialize() async {
    if (!_isHotkeySupported()) {
      debugPrint('Hotkeys not supported on this platform');
      return;
    }

    await hotKeyManager.unregisterAll();
  }

  /// Check if hotkeys are supported on this platform
  bool _isHotkeySupported() {
    // Hotkeys supported on desktop platforms
    return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
  }

  /// Register global hotkey for capture mode
  /// Default: Ctrl+Space (easier to trigger than double-tap Ctrl)
  Future<bool> registerCaptureHotkey() async {
    if (!_isHotkeySupported()) {
      debugPrint('Hotkeys not supported on this platform');
      return false;
    }

    try {
      // Unregister existing hotkey
      if (_isRegistered && _captureHotkey != null) {
        await hotKeyManager.unregister(_captureHotkey!);
      }

      // Create new hotkey: Ctrl+Space
      _captureHotkey = HotKey(
        key: LogicalKeyboardKey.space,
        modifiers: [HotKeyModifier.control],
        scope: HotKeyScope.system,
      );

      // Register with callback
      await hotKeyManager.register(
        _captureHotkey!,
        keyDownHandler: (hotKey) {
          debugPrint('Capture hotkey pressed: ${hotKey.toString()}');
          onCaptureTriggered?.call();
        },
      );

      _isRegistered = true;
      debugPrint('Registered capture hotkey: Ctrl+Space');
      return true;
    } catch (e) {
      debugPrint('Failed to register hotkey: $e');
      return false;
    }
  }

  /// Unregister all hotkeys
  Future<void> unregisterAll() async {
    if (!_isHotkeySupported()) return;

    try {
      await hotKeyManager.unregisterAll();
      _isRegistered = false;
      _captureHotkey = null;
      debugPrint('Unregistered all hotkeys');
    } catch (e) {
      debugPrint('Failed to unregister hotkeys: $e');
    }
  }

  /// Get current registered hotkey description
  String? getHotkeyDescription() {
    if (_captureHotkey == null) return null;
    return 'Ctrl+Space';
  }

  bool get isRegistered => _isRegistered;
}
