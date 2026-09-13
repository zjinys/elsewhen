import 'dart:io';
import 'package:flutter/foundation.dart';

enum SessionType {
  x11,
  wayland,
  unknown,
}

/// Stub hotkey service - plugins have compilation issues on Linux
/// Users should configure system-level shortcuts instead
class HotkeyService {
  static final HotkeyService _instance = HotkeyService._internal();
  factory HotkeyService() => _instance;
  HotkeyService._internal();

  VoidCallback? onCaptureTriggered;

  SessionType _sessionType = SessionType.unknown;
  bool _isInitialized = false;

  SessionType get sessionType => _sessionType;
  bool get isSupported => false; // Always disabled due to plugin issues

  Future<void> initialize() async {
    if (_isInitialized) return;

    try {
      if (Platform.isLinux) {
        _sessionType = await _detectSessionType();
        debugPrint('Detected session type: $_sessionType');
      }

      _isInitialized = true;
      debugPrint('Hotkey service initialized (stub mode - plugins disabled)');
    } catch (e) {
      debugPrint('Failed to initialize hotkey service: $e');
      _isInitialized = true;
    }
  }

  Future<SessionType> _detectSessionType() async {
    try {
      final sessionTypeEnv = Platform.environment['XDG_SESSION_TYPE'];
      if (sessionTypeEnv != null) {
        if (sessionTypeEnv.toLowerCase() == 'wayland') {
          return SessionType.wayland;
        } else if (sessionTypeEnv.toLowerCase() == 'x11') {
          return SessionType.x11;
        }
      }

      final waylandDisplay = Platform.environment['WAYLAND_DISPLAY'];
      if (waylandDisplay != null && waylandDisplay.isNotEmpty) {
        return SessionType.wayland;
      }

      final display = Platform.environment['DISPLAY'];
      if (display != null && display.isNotEmpty) {
        return SessionType.x11;
      }

      return SessionType.unknown;
    } catch (e) {
      debugPrint('Failed to detect session type: $e');
      return SessionType.unknown;
    }
  }

  Future<bool> registerCaptureHotkey() async {
    if (!_isInitialized) {
      await initialize();
    }

    debugPrint('Global hotkeys disabled - use system shortcuts or launch scripts');
    return false;
  }

  String getHotkeyDescription() {
    return 'Ctrl+Space (需要系统级配置)';
  }

  String getSetupInstructions() {
    return '''
由于 Linux 插件兼容性问题，请使用以下替代方案：

方案 1：系统快捷键（推荐）
1. 打开系统设置 → 键盘 → 自定义快捷键
2. 添加新快捷键：
   - 名称: Elsewhen Capture
   - 命令: /path/to/elsewhen --mode=capture
   - 快捷键: Meta+Space 或 Ctrl+Alt+C

方案 2：启动脚本
运行项目根目录的脚本：
  ./elsewhen-capture.sh  # 启动 Capture 模式
  ./elsewhen.sh          # 启动主应用

方案 3：手动命令
  cd ui && fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"
''';
  }

  Future<void> dispose() async {
    // Nothing to clean up in stub mode
  }
}
