import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart' as ui;
import 'package:nativeapi/nativeapi.dart' as na;

import 'window_geometry_store.dart';
import 'window_service.dart';

/// 桌面实现：window_manager / screen_retriever → nativeapi（仅 Linux/macOS/Windows
/// 参与编译；Android/iOS 走 window_service_stub.dart，cnativeapi FFI 会把
/// Android AOT 编译器打崩）。
class DesktopWindowService implements WindowService {
  @override
  WindowMode currentMode = WindowMode.main;

  na.ListenerId? _listenerId;

  /// 主窗口几何的本地配置存储（window.json，不落数据库）。
  final WindowGeometryStore _geometryStore = const WindowGeometryStore();
  Timer? _saveGeometryDebounce;

  // Window configurations
  // 主窗口默认 1440x900：三栏布局（侧栏+正文+440 聊天面板）下够用，
  // 在 1080p 屏和笔记本屏上四边留有边距，不顶满工作区、
  // 不给 compositor 自作主张（移动/最大化）的机会；小屏再由 clampedMainSize 兜底。
  static const ui.Size mainWindowSize = ui.Size(1440, 900);
  static const ui.Size mainWindowMinSize = ui.Size(800, 600);
  static const ui.Size captureWindowSize = ui.Size(500, 240);

  /// 当前平台窗口（Flutter 引擎窗口，nativeapi 侧以 GTK/NSWindow/HWND 枚举）。
  na.Window? get _window => na.WindowManager.instance.getCurrent();

  // nativeapi 0.4 uses its own geometry and color value types.
  static na.Size _naSize(ui.Size s) =>
      na.Size(width: s.width, height: s.height);
  static na.Rectangle _naRect(ui.Rect r) =>
      na.Rectangle(x: r.left, y: r.top, width: r.width, height: r.height);
  static ui.Rect _uiRect(na.Rectangle r) =>
      ui.Rect.fromLTWH(r.x, r.y, r.width, r.height);
  static const na.Color _transparent = na.Color(r: 0, g: 0, b: 0, a: 0);

  /// 等窗口可用（nativeapi 从现有原生窗口枚举，理论上 main() 里就绪；
  /// 轮询只是多平台保险）。超时返回 null，不阻塞启动。
  Future<na.Window?> _waitForWindow({
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
    ui.debugPrint('nativeapi: 窗口未就绪（超时 ${timeout.inMilliseconds}ms）');
    return null;
  }

  /// 主窗口目标尺寸：默认 1440x900，但夹在可用区内。
  /// 优先级：主显示器工作区（DisplayManager.workArea，扣掉顶栏/dock）>
  /// 整屏逻辑尺寸（platformDispatcher）> 默认值。
  /// 小屏/分屏下强制超大尺寸会与 compositor 来回拉扯，
  /// 启动时表现为 OpenGL frame 尺寸 WARNING（无害但吵）。
  /// 取不到显示器信息时回退默认值。
  static ui.Size clampedMainSize() {
    const want = mainWindowSize;
    const min = mainWindowMinSize;
    ui.Size? cap;
    try {
      final r = na.DisplayManager.instance.getPrimary()?.workArea;
      if (r != null) cap = ui.Size(r.width, r.height);
    } catch (_) {
      // 插件不可用时退到整屏尺寸
    }
    cap ??= _logicalDisplaySize();
    if (cap == null) return want;
    double clamp(double v, double lo, double hi) =>
        math.min(math.max(v, lo), math.max(hi, lo));
    return ui.Size(
      clamp(want.width, min.width, cap.width),
      clamp(want.height, min.height, cap.height),
    );
  }

  /// 整屏逻辑尺寸兜底（Display.size 是物理像素，先除 dpr）。
  static ui.Size? _logicalDisplaySize() {
    try {
      final displays = ui.WidgetsBinding.instance.platformDispatcher.displays;
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
  @override
  Future<void> applyMainChrome() async {
    if (!_isDesktop()) return;
    currentMode = WindowMode.main;
    final w = await _waitForWindow();
    if (w == null) return;

    _applyTitleBarHidden(w);
    w.minimumSize = _naSize(mainWindowMinSize);
    w.isVisibleInTaskbar = true;
    w.isClosable = false; // 关闭即隐藏：拦截原生关闭，走自定义关闭按钮 hide()
    w.title = 'Elsewhen';
    _applyWindowBackground(w);

    final restored = await restoreMainBounds();
    if (!restored) {
      // 首次启动（或显示器布局变了）：默认尺寸 + 居中。
      w.setSize(_naSize(clampedMainSize()), false);
      w.center();
    }
    ui.debugPrint('Applied main window chrome');
  }

  /// 应用捕获窗口外观（启动期以 --mode=capture 进入时调用）。
  @override
  Future<void> applyCaptureChrome() async {
    if (!_isDesktop()) return;
    currentMode = WindowMode.capture;
    final w = await _waitForWindow();
    if (w == null) return;

    w.setSize(_naSize(captureWindowSize), false);
    w.isAlwaysOnTop = true;
    _applyTitleBarHidden(w);
    w.isClosable = false; // 与主窗口一致：捕获窗点 X 走 hide()
    _applyWindowBackground(w);
    w.center();
    ui.debugPrint('Applied capture window chrome');
  }

  @override
  Future<void> initialize() async {
    if (!_isDesktop()) return;

    _listenerId = na.WindowManager.instance.addListener(_onWindowEvent);
  }

  bool _isDesktop() {
    return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
  }

  /// 隐藏原生标题栏（无边框）——**macOS/Windows 走 nativeapi，Linux 跳过**。
  ///
  /// Linux 上不能用 nativeapi 改：它在 Flutter 已建好 GL 上下文之后才执行，GTK
  /// 为此重建窗口 GdkVisual，与已有上下文不匹配，首帧即
  /// `Could not determine GL version` +
  /// `Failed to create platform view rendering surface` +
  /// `FlutterEngineRunTask returned kInvalidArguments`，表现为**窗口只有边框、
  /// 完全没有内容**。
  ///
  /// 逐项二分定位（正确入口 `lib/main_desktop.dart` 下实测）：跳过
  /// `titleBarStyle` → GL 错误 0；跳过 `backgroundColor` / `minimumSize` /
  /// `isVisibleInTaskbar` / `isClosable` / `title` / 尺寸落位 → 全部仍失败。
  ///
  /// 改由 `linux/runner/my_application.cc` 在 `fl_view_new` 之前用
  /// `gtk_window_set_decorated(FALSE)` 完成，视觉上等价且时序安全。
  void _applyTitleBarHidden(na.Window w) {
    if (Platform.isLinux) return;
    w.titleBarStyle = na.TitleBarStyle.hidden;
  }

  /// 设置窗口背景色（全平台透明）。
  ///
  /// 透明本身是安全的：Linux 上实测设透明背景 GL 错误 0（见下）。真正的坑是
  /// `titleBarStyle`，见 [_applyTitleBarHidden]。Linux 上的透明让窗口圆角能透出
  /// 桌面，故不再按平台区分。
  void _applyWindowBackground(na.Window w) {
    w.backgroundColor = _transparent;
  }

  /// Switch to main application mode
  @override
  Future<void> switchToMainMode() async {
    if (!_isDesktop()) return;

    currentMode = WindowMode.main;
    final w = _window;
    if (w == null) return;

    // 恢复上次的主窗口几何；无历史则默认尺寸+居中。
    final restored = await restoreMainBounds();
    if (!restored) {
      w.setSize(_naSize(clampedMainSize()), false);
      w.center();
    }
    w.minimumSize = _naSize(mainWindowMinSize);
    w.isAlwaysOnTop = false;
    _applyTitleBarHidden(w);  // Linux 跳过 titleBarStyle（GL 崩溃），见该方法注释
    w.title = 'Elsewhen';
    w.show();
    w.focus();

    ui.debugPrint('Switched to main mode');
  }

  /// 恢复上次关闭时保存的主窗口大小/位置（本地 window.json，不落库）。
  /// 返回 true 表示已按保存几何落位，调用方无需再 center。
  /// 任何失败（非桌面 / 无历史 / 窗口未就绪 / 显示器布局变了）都返回 false，
  /// 由调用方退回默认居中，绝不把窗口摆到屏幕外或带着坏数据落位。
  @override
  Future<bool> restoreMainBounds() async {
    if (!_isDesktop()) return false;
    final saved = _geometryStore.load();
    if (saved == null) return false;
    final w = _window;
    if (w == null) return false;

    // 多显示器工作区：恢复的目标屏未必是主屏，全部参与判断。
    List<ui.Rect> visibleAreas;
    try {
      visibleAreas = na.DisplayManager.instance
          .getAll()
          .map((d) => _uiRect(d.workArea))
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

    w.bounds = _naRect(bounds);
    ui.debugPrint(
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
    if (!_isDesktop() || currentMode != WindowMode.main) return;
    _saveGeometryDebounce?.cancel();
    _saveGeometryDebounce = Timer(const Duration(milliseconds: 400), () {
      try {
        final w = _window;
        if (w == null) return;
        if (w.isMaximized) return;
        if (w.isFullScreen) return;
        final b = w.bounds;
        _geometryStore.save(
          WindowGeometry(x: b.x, y: b.y, width: b.width, height: b.height),
        );
      } catch (_) {
        // 落盘失败静默：窗口偏好丢了，下次启动退回默认落位。
      }
    });
  }

  /// Switch to capture mode
  @override
  Future<void> switchToCaptureMode() async {
    if (!_isDesktop()) return;

    currentMode = WindowMode.capture;
    final w = _window;
    if (w == null) return;

    w.setSize(_naSize(captureWindowSize), false);
    w.isAlwaysOnTop = true;
    _applyTitleBarHidden(w);  // Linux 跳过 titleBarStyle（GL 崩溃），见该方法注释
    w.center();
    w.show();
    w.focus();

    ui.debugPrint('Switched to capture mode');
  }

  /// Hide window
  @override
  Future<void> hideWindow() async {
    if (!_isDesktop()) return;

    currentMode = WindowMode.hidden;
    _window?.hide();

    ui.debugPrint('Window hidden');
  }

  /// Show window in current mode
  @override
  Future<void> showWindow() async {
    if (!_isDesktop()) return;

    _window?.show();
    _window?.focus();

    ui.debugPrint('Window shown');
  }

  /// Toggle between main and capture mode
  @override
  Future<void> toggleMode() async {
    if (currentMode == WindowMode.main) {
      await switchToCaptureMode();
    } else {
      await switchToMainMode();
    }
  }

  /// Close window (but keep app running in background)
  @override
  Future<void> closeWindow() async {
    if (!_isDesktop()) return;

    _window?.hide();
  }

  /// Quit application
  @override
  Future<void> quitApp() async {
    if (!_isDesktop()) return;

    na.Application.instance.quit(0);
  }

  @override
  void startDragging() => _window?.startDragging();

  @override
  void minimize() => _window?.minimize();

  @override
  void toggleMaximize() {
    final w = _window;
    if (w == null) return;
    if (w.isMaximized) {
      w.unmaximize();
    } else {
      w.maximize();
    }
  }

  /// nativeapi 事件统一入口（密封类 switch）。
  void _onWindowEvent(na.WindowEvent event) {
    switch (event) {
      case na.WindowClosedEvent():
        // 关闭即隐藏（isClosable=false 时正常不会走到这里，双保险）。
        _window?.hide();
      case na.WindowMovedEvent():
      case na.WindowResizedEvent():
        _scheduleGeometrySave();
      case na.WindowFocusedEvent():
      case na.WindowBlurredEvent():
      case na.WindowMinimizedEvent():
      case na.WindowMaximizedEvent():
      case na.WindowRestoredEvent():
      case na.WindowCreatedEvent():
        break;
    }
  }

  @override
  void dispose() {
    _saveGeometryDebounce?.cancel();
    final id = _listenerId;
    if (id != null) {
      na.WindowManager.instance.removeListener(id);
      _listenerId = null;
    }
  }
}
