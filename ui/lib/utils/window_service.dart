import 'package:flutter/material.dart';
import 'package:nativeapi/nativeapi.dart';

import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'window_geometry_store.dart';

enum WindowMode { main, capture, hidden }

/// 平台窗口封装（window_manager / screen_retriever → nativeapi）。
///
/// 迁移要点：
/// - nativeapi 无需 `ensureInitialized`，也无需 `waitUntilReadyToShow`：
///   Flutter runner 在首帧自动显示窗口，这里只负责尽早把无边框/尺寸/
///   位置/置顶等各项配置落到 `WindowManager.instance.getCurrent()`。
/// - 事件模型从 `WindowListener` mixin 换成 `addListener(WindowEvent)` 密封类
///   + ListenerId 记账。
/// - 显示器信息从 `DisplayManager.instance` 取（workArea 等价于旧
///   visiblePosition+visibleSize）。
/// - `startDragging()` 在 Linux 原生层已实现（window_linux.cpp 走
///   gdk_window_begin_move_drag_for_device），可用于标题栏拖拽。
class WindowService {
  static final WindowService _instance = WindowService._internal();
  factory WindowService() => _instance;
  WindowService._internal();

  WindowMode _currentMode = WindowMode.main;
  WindowMode get currentMode => _currentMode;

  ListenerId? _listenerId;

  /// 主窗口几何的本地配置存储（window.json，不落数据库）。
  final WindowGeometryStore _geometryStore = const WindowGeometryStore();
  Timer? _saveGeometryDebounce;

  // Window configurations
  // 主窗口默认 1440x900：三栏布局（侧栏+正文+440 聊天面板）下够用，
  // 在 1080p 屏和笔记本屏上四边留有边距，不顶满工作区、
  // 不给 compositor 自作主张（移动/最大化）的机会；小屏再由 clampedMainSize 兜底。
  static const Size mainWindowSize = Size(1440, 900);
  static const Size mainWindowMinSize = Size(800, 600);
  static const Size captureWindowSize = Size(500, 240);

  /// 当前平台窗口（Flutter 引擎窗口，nativeapi 侧以 GTK/NSWindow/HWND 枚举）。
  Window? get _window => WindowManager.instance.getCurrent();

  /// 等窗口可用（nativeapi 从现有原生窗口枚举，理论上 main() 里就绪；
  /// 轮询只是多平台保险）。超时返回 null，不阻塞启动。
  Future<Window?> _waitForWindow({
    Duration timeout = const Duration(seconds: 2),
  }) async {
    final deadline = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(deadline)) {
      final w = _window;
      if (w != null) return w;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    final w = _window;
    if (w != null) return w;
    debugPrint('nativeapi: 窗口未就绪（超时 ${timeout.inMilliseconds}ms）');
    return null;
  }

  /// 主窗口目标尺寸：默认 1440x900，但夹在可用区内。
  /// 优先级：主显示器工作区（DisplayManager.workArea，扣掉顶栏/dock）>
  /// 整屏逻辑尺寸（platformDispatcher）> 默认值。
  /// 小屏/分屏下强制超大尺寸会与 compositor 来回拉扯，
  /// 启动时表现为 OpenGL frame 尺寸 WARNING（无害但吵）。
  /// 取不到显示器信息时回退默认值。
  static Size clampedMainSize() {
    const want = mainWindowSize;
    const min = mainWindowMinSize;
    Size? cap;
    try {
      cap = DisplayManager.instance.getPrimary()?.workArea.size;
    } catch (_) {
      // 插件不可用时退到整屏尺寸
    }
    cap ??= _logicalDisplaySize();
    if (cap == null) return want;
    double clamp(double v, double lo, double hi) =>
        math.min(math.max(v, lo), math.max(hi, lo));
    return Size(
      clamp(want.width, min.width, cap.width),
      clamp(want.height, min.height, cap.height),
    );
  }

  /// 整屏逻辑尺寸兜底（Display.size 是物理像素，先除 dpr）。
  static Size? _logicalDisplaySize() {
    try {
      final displays = WidgetsBinding.instance.platformDispatcher.displays;
      if (displays.isEmpty) return null;
      final d = displays.first;
      return d.size / d.devicePixelRatio;
    } catch (_) {
      return null;
    }
  }

  /// 应用主窗口外观与落位（启动期调用，早于首帧以规避无边框闪烁）。
  /// - 无边框标题栏（隐藏原生标题栏）、最小尺寸、任务栏可见、标题；
  /// - 有保存几何则恢复大小/位置，否则默认尺寸+居中。
  /// 不主动 show()：Flutter runner 首帧会自动显示窗口。
  Future<void> applyMainChrome() async {
    if (!_isDesktop()) return;
    _currentMode = WindowMode.main;
    final w = await _waitForWindow();
    if (w == null) return;

    w.titleBarStyle = TitleBarStyle.hidden;
    w.minimumSize = mainWindowMinSize;
    w.isVisibleInTaskbar = true;
    w.isClosable = false; // 关闭即隐藏：拦截原生关闭，走自定义关闭按钮 hide()
    w.title = 'Elsewhen';
    w.backgroundColor = Colors.transparent;

    final restored = await restoreMainBounds();
    if (!restored) {
      // 首次启动（或显示器布局变了）：默认尺寸 + 居中。
      w.setSize(clampedMainSize(), false);
      w.center();
    }
    debugPrint('Applied main window chrome');
  }

  /// 应用捕获窗口外观（启动期以 --mode=capture 进入时调用）。
  Future<void> applyCaptureChrome() async {
    if (!_isDesktop()) return;
    _currentMode = WindowMode.capture;
    final w = await _waitForWindow();
    if (w == null) return;

    w.setSize(captureWindowSize, false);
    w.isAlwaysOnTop = true;
    w.titleBarStyle = TitleBarStyle.hidden;
    w.isClosable = false; // 与主窗口一致：捕获窗点 X 走 hide()
    w.backgroundColor = Colors.transparent;
    w.center();
    debugPrint('Applied capture window chrome');
  }

  Future<void> initialize() async {
    if (!_isDesktop()) return;

    _listenerId = WindowManager.instance.addListener(_onWindowEvent);
  }

  bool _isDesktop() {
    return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
  }

  /// Switch to main application mode
  Future<void> switchToMainMode() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.main;
    final w = _window;
    if (w == null) return;

    // 恢复上次的主窗口几何；无历史则默认尺寸+居中。
    final restored = await restoreMainBounds();
    if (!restored) {
      w.setSize(clampedMainSize(), false);
      w.center();
    }
    w.minimumSize = mainWindowMinSize;
    w.isAlwaysOnTop = false;
    w.titleBarStyle = TitleBarStyle.hidden;
    w.title = 'Elsewhen';
    w.show();
    w.focus();

    debugPrint('Switched to main mode');
  }

  /// 恢复上次关闭时保存的主窗口大小/位置（本地 window.json，不落库）。
  /// 返回 true 表示已按保存几何落位，调用方无需再 center。
  /// 任何失败（非桌面 / 无历史 / 窗口未就绪 / 显示器布局变了）都返回 false，
  /// 由调用方退回默认居中，绝不把窗口摆到屏幕外或带着坏数据落位。
  Future<bool> restoreMainBounds() async {
    if (!_isDesktop()) return false;
    final saved = _geometryStore.load();
    if (saved == null) return false;
    final w = _window;
    if (w == null) return false;

    // 多显示器工作区：恢复的目标屏未必是主屏，全部参与判断。
    List<Rect> visibleAreas;
    try {
      visibleAreas = DisplayManager.instance
          .getAll()
          .map((d) => d.workArea)
          .toList();
    } catch (_) {
      // 插件不可用时退到默认落位：位置可信度不高，不值得为此多折腾。
      return false;
    }

    final bounds = WindowGeometryStore.computeRestoreBounds(
      saved,
      visibleAreas,
      minSize: mainWindowMinSize,
    );
    if (bounds == null) return false;

    w.bounds = bounds;
    debugPrint(
      'Restored window bounds: '
      '(${bounds.left.toInt()}, ${bounds.top.toInt()}) '
      '${bounds.width.toInt()}x${bounds.height.toInt()}',
    );
    return true;
  }

  /// 把当前主窗口几何保存到本地配置（防抖）。
  /// 最大化/全屏时记录的是铺满状态，不是用户想要的「普通几何」，
  /// 跳过这类事件；还原时的 setBounds 也能把窗口从最大化拽回。
  void _scheduleGeometrySave() {
    if (!_isDesktop() || _currentMode != WindowMode.main) return;
    _saveGeometryDebounce?.cancel();
    _saveGeometryDebounce = Timer(const Duration(milliseconds: 400), () {
      try {
        final w = _window;
        if (w == null) return;
        if (w.isMaximized) return;
        if (w.isFullScreen) return;
        final b = w.bounds;
        _geometryStore.save(
          WindowGeometry(x: b.left, y: b.top, width: b.width, height: b.height),
        );
      } catch (_) {
        // 落盘失败静默：窗口偏好丢了，下次启动退回默认落位。
      }
    });
  }

  /// Switch to capture mode
  Future<void> switchToCaptureMode() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.capture;
    final w = _window;
    if (w == null) return;

    w.setSize(captureWindowSize, false);
    w.isAlwaysOnTop = true;
    w.titleBarStyle = TitleBarStyle.hidden;
    w.center();
    w.show();
    w.focus();

    debugPrint('Switched to capture mode');
  }

  /// Hide window
  Future<void> hideWindow() async {
    if (!_isDesktop()) return;

    _currentMode = WindowMode.hidden;
    _window?.hide();

    debugPrint('Window hidden');
  }

  /// Show window in current mode
  Future<void> showWindow() async {
    if (!_isDesktop()) return;

    _window?.show();
    _window?.focus();

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

    _window?.hide();
  }

  /// Quit application
  Future<void> quitApp() async {
    if (!_isDesktop()) return;

    Application.instance.quit(0);
  }

  /// nativeapi 事件统一入口（密封类 switch）。
  void _onWindowEvent(WindowEvent event) {
    switch (event) {
      case WindowClosedEvent():
        // 关闭即隐藏（isClosable=false 时正常不会走到这里，双保险）。
        _window?.hide();
      case WindowMovedEvent():
      case WindowResizedEvent():
        _scheduleGeometrySave();
      case WindowFocusedEvent():
      case WindowBlurredEvent():
      case WindowMinimizedEvent():
      case WindowMaximizedEvent():
      case WindowRestoredEvent():
      case WindowCreatedEvent():
        break;
    }
  }

  void dispose() {
    _saveGeometryDebounce?.cancel();
    final id = _listenerId;
    if (id != null) {
      WindowManager.instance.removeListener(id);
      _listenerId = null;
    }
  }
}
