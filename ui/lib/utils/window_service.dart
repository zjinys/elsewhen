import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';
import 'dart:io';

enum WindowMode {
  main,
  capture,
  hidden,
}

class WindowService with WindowListener {
  static final WindowService _instance = WindowService._internal();
  factory WindowService() => _instance;
  WindowService._internal();

  WindowMode _currentMode = WindowMode.main;
  WindowMode get currentMode => _currentMode;

  // Window configurations
  static const Size mainWindowSize = Size(1920, 1080);
  static const Size mainWindowMinSize = Size(800, 600);
  static const Size captureWindowSize = Size(500, 240);

  Future<void> initialize() async {
    if (!_isDesktop()) return;

    await windowManager.ensureInitialized();
    windowManager.addListener(this);
  }

  bool _isDesktop() {
    return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
  }

  /// Switch to main application mode
  Future<void> switchToMainMode() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.main;

    await windowManager.setSize(mainWindowSize);
    await windowManager.setMinimumSize(mainWindowMinSize);
    await windowManager.setAlwaysOnTop(false);
    await windowManager.setTitleBarStyle(TitleBarStyle.hidden);
    await windowManager.setTitle('Elsewhen');
    await windowManager.center();
    await windowManager.show();
    await windowManager.focus();

    debugPrint('Switched to main mode');
  }

  /// Switch to capture mode
  Future<void> switchToCaptureMode() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.capture;

    await windowManager.setSize(captureWindowSize);
    await windowManager.setAlwaysOnTop(true);
    await windowManager.setTitleBarStyle(TitleBarStyle.hidden);
    await windowManager.center();
    await windowManager.show();
    await windowManager.focus();

    debugPrint('Switched to capture mode');
  }

  /// Hide window
  Future<void> hideWindow() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.hidden;
    await windowManager.hide();

    debugPrint('Window hidden');
  }

  /// Show window in current mode
  Future<void> showWindow() async {
    if (!_isDesktop()) return;

    await windowManager.show();
    await windowManager.focus();

    debugPrint('Window shown');
  }

  /// Toggle between main and capture mode
  Future<void> toggleMode() async {
    if (_currentMode == WindowMode.main) {
      await switchToCaptureMode();
    } else {
      await switchToMainMode();
    }
  }

  /// Close window (but keep app running in background)
  Future<void> closeWindow() async {
    if (!_isDesktop()) return;

    await windowManager.hide();
  }

  /// Quit application
  Future<void> quitApp() async {
    if (!_isDesktop()) return;

    await windowManager.destroy();
  }

  @override
  void onWindowClose() {
    // Override default close behavior - hide instead of quit
    windowManager.hide();
  }

  @override
  void onWindowFocus() {
    debugPrint('Window focused');
  }

  @override
  void onWindowBlur() {
    debugPrint('Window blurred');
  }

  @override
  void onWindowMinimize() {
    debugPrint('Window minimized');
  }

  @override
  void onWindowRestore() {
    debugPrint('Window restored');
  }

  void dispose() {
    windowManager.removeListener(this);
  }
}
