import 'window_service_stub.dart';
import 'desktop_prefs.dart';

export 'desktop_prefs.dart' show CloseAction;

enum WindowMode { main, capture, hidden }

/// 平台窗口封装接口。
///
/// 桌面实现（window_service_desktop.dart，基于 window_manager）只在桌面入口
/// main_desktop.dart 里通过 [WindowService.register] 注册——**本文件不
/// import 任何 desktop 实现**：桌面窗口插件含原生 method channel 注册，
/// 不进入 Android 的 main 可达图（历史原因：前任 nativeapi 的 FFI 结构会让
/// Android release AOT 编译器崩溃；独立入口的隔离纪律保留）。
abstract class WindowService {
  static WindowService _instance = StubWindowService();

  /// 桌面入口（main_desktop.dart）在启动时注册 desktop 实现。
  /// 必须在任何 WindowService() 使用之前调用。
  static void register(WindowService impl) => _instance = impl;

  factory WindowService() => _instance;

  WindowMode get currentMode;

  Future<void> applyMainChrome();
  Future<void> applyCaptureChrome();
  Future<void> initialize();
  Future<void> switchToMainMode();
  Future<void> switchToCaptureMode();
  Future<void> hideWindow();
  Future<void> showWindow();
  Future<void> toggleMode();
  Future<bool> restoreMainBounds();
  Future<void> closeWindow();
  Future<void> quitApp();

  /// 「关闭窗口时」行为（桌面；移动端恒为 [CloseAction.minimizeToTray] 且无效果）。
  CloseAction get closeAction;
  Future<void> setCloseAction(CloseAction action);

  /// 系统托盘是否可用（非桌面、或托盘初始化失败时为 false）。
  /// 托盘不可用时「关闭」必须退化为退出，否则窗口藏起来再也唤不回。
  bool get trayAvailable;

  /// 统一的「关闭」入口：按 [closeAction] 决定隐藏到托盘还是退出。
  /// 原生关闭按钮（`onWindowClose`）与自绘标题栏 ✕ 都走这里。
  Future<void> requestClose();

  /// 标题栏窗口控制（移动端为 no-op）。
  void startDragging();
  void minimize();
  void toggleMaximize();

  void dispose();
}
