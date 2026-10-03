import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart' as ui;
import 'package:screen_retriever/screen_retriever.dart';
import 'package:window_manager/window_manager.dart';

import 'window_geometry_store.dart';
import 'window_service.dart';
import '../bridge/generated.dart/api/chat_runs.dart' as chat_runs;

/// 桌面实现：window_manager（仅 Linux/macOS/Windows 参与编译；
/// Android/iOS 走 window_service_stub.dart，本文件只在 main_desktop.dart
/// 注册，不进入 Android main 的可达图）。
///
/// 跨平台视觉一致的纪律（FlClash 生产验证）：
/// - **不设透明背景、不自裁圆角**——窗口不透明直角，圆角交给系统
///   （Win11 DWM 自动圆角、macOS 系统圆角、Linux 直角）。透明像素在
///   Windows 上被 DWM 合成为白色，会留一圈白边（踩过）。
/// - 无边框：macOS/Windows 用 `setTitleBarStyle(TitleBarStyle.hidden)`；
///   **Linux 在 `my_application.cc` 建窗期 `gtk_window_set_decorated(FALSE)`**
///   （window_manager/nativeapi 在 GL 上下文建好后再改会崩 GTK，踩过）。
class DesktopWindowService implements WindowService {
  @override
  WindowMode currentMode = WindowMode.main;

  /// 事件监听（window_manager 是 mixin 式回调，这里挂到 WindowListener）。
  _WindowEventRelay? _eventRelay;

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

  bool _isDesktop() {
    return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
  }

  /// 主窗口目标尺寸：默认 1440x900，但夹在可用区内。
  /// 优先级：主显示器工作区（screen_retriever visibleSize）>
  /// 整屏逻辑尺寸（platformDispatcher）> 默认值。
  static Future<ui.Size> clampedMainSize() async {
    const want = mainWindowSize;
    const min = mainWindowMinSize;
    ui.Size? cap;
    try {
      final d = await screenRetriever.getPrimaryDisplay();
      final v = d.visibleSize;
      if (v != null) cap = ui.Size(v.width, v.height);
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

  /// 应用主窗口外观与落位（启动期调用）。
  /// window_manager 的标准编排：ensureInitialized → 保存几何决定 size/center →
  /// WindowOptions → waitUntilReadyToShow → show。
  /// 注意：Flutter 桌面 runner 默认建窗即显示，原生标题栏闪现靠 runner 侧
  /// 隐藏窗口规避（Linux my_application.cc 已处理）；waitUntilReadyToShow
  /// 之前的 setTitleBarStyle 保证首帧就是无边框。
  @override
  Future<void> applyMainChrome() async {
    if (!_isDesktop()) return;
    currentMode = WindowMode.main;
    await windowManager.ensureInitialized();

    await _applyTitleBarHidden();
    await windowManager.setPreventClose(true); // 关闭即隐藏：走 onWindowClose → hide()

    final restored = await restoreMainBounds();
    final size = restored ? null : await clampedMainSize();
    await windowManager.waitUntilReadyToShow(
      WindowOptions(
        size: size,
        minimumSize: mainWindowMinSize,
        center: !restored,
        title: 'Elsewhen',
      ),
      () async {
        await windowManager.show();
        await windowManager.focus();
      },
    );
    ui.debugPrint('Applied main window chrome');
  }

  /// 应用捕获窗口外观（启动期以 --mode=capture 进入时调用）。
  @override
  Future<void> applyCaptureChrome() async {
    if (!_isDesktop()) return;
    currentMode = WindowMode.capture;
    await windowManager.ensureInitialized();

    await _applyTitleBarHidden();
    await windowManager.setPreventClose(true);

    await windowManager.waitUntilReadyToShow(
      const WindowOptions(
        size: captureWindowSize,
        center: true,
        alwaysOnTop: true,
        title: 'Elsewhen',
      ),
      () async {
        await windowManager.show();
        await windowManager.focus();
      },
    );
    ui.debugPrint('Applied capture window chrome');
  }

  @override
  Future<void> initialize() async {
    if (!_isDesktop()) return;
    _eventRelay ??= _WindowEventRelay(this);
    windowManager.addListener(_eventRelay!);
  }

  /// 隐藏原生标题栏（无边框）——**macOS/Windows 走 window_manager，Linux 跳过**。
  ///
  /// Linux 上不能在运行时改：GL 上下文已建好，此时改 GTK 装饰会重建
  /// GdkVisual 与已有上下文不匹配，首帧即 GL 错误、窗口只有边框没内容
  /// （nativeapi 时代逐项二分定位的坑，window_manager 同病）。
  /// 改由 `linux/runner/my_application.cc` 在 `fl_view_new` 之前
  /// `gtk_window_set_decorated(FALSE)` 完成。
  Future<void> _applyTitleBarHidden() async {
    if (Platform.isLinux) return;
    await windowManager.setTitleBarStyle(TitleBarStyle.hidden);
  }

  /// Switch to main application mode
  @override
  Future<void> switchToMainMode() async {
    if (!_isDesktop()) return;

    currentMode = WindowMode.main;

    // 恢复上次的主窗口几何；无历史则默认尺寸+居中。
    final restored = await restoreMainBounds();
    if (!restored) {
      await windowManager.setSize(await clampedMainSize());
      await windowManager.center();
    }
    await windowManager.setMinimumSize(mainWindowMinSize);
    await windowManager.setAlwaysOnTop(false);
    await _applyTitleBarHidden(); // Linux 跳过（GL 崩溃），见该方法注释
    await windowManager.setTitle('Elsewhen');
    await windowManager.show();
    await windowManager.focus();

    ui.debugPrint('Switched to main mode');
  }

  /// 恢复上次关闭时保存的主窗口大小/位置（本地 window.json，不落库）。
  /// 返回 true 表示已按保存几何落位，调用方无需再 center。
  /// 任何失败（非桌面 / 无历史 / 显示器布局变了）都返回 false，
  /// 由调用方退回默认居中，绝不把窗口摆到屏幕外或带着坏数据落位。
  @override
  Future<bool> restoreMainBounds() async {
    if (!_isDesktop()) return false;
    final saved = _geometryStore.load();
    if (saved == null) return false;

    // 多显示器工作区：恢复的目标屏未必是主屏，全部参与判断。
    List<ui.Rect> visibleAreas;
    try {
      final displays = await screenRetriever.getAllDisplays();
      visibleAreas = displays.map((d) {
        final pos = d.visiblePosition ?? ui.Offset.zero;
        final size = d.visibleSize ?? d.size;
        return ui.Rect.fromLTWH(pos.dx, pos.dy, size.width, size.height);
      }).toList();
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

    await windowManager.setBounds(null, size: bounds.size, position: bounds.topLeft);
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
    _saveGeometryDebounce = Timer(const Duration(milliseconds: 400), () async {
      try {
        if (await windowManager.isMaximized()) return;
        if (await windowManager.isFullScreen()) return;
        final b = await windowManager.getBounds();
        _geometryStore.save(
          WindowGeometry(x: b.left, y: b.top, width: b.width, height: b.height),
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

    await windowManager.setSize(captureWindowSize);
    await windowManager.setAlwaysOnTop(true);
    await _applyTitleBarHidden(); // Linux 跳过（GL 崩溃），见该方法注释
    await windowManager.center();
    await windowManager.show();
    await windowManager.focus();

    ui.debugPrint('Switched to capture mode');
  }

  /// Hide window
  @override
  Future<void> hideWindow() async {
    if (!_isDesktop()) return;

    currentMode = WindowMode.hidden;
    await windowManager.hide();

    ui.debugPrint('Window hidden');
  }

  /// Show window in current mode
  @override
  Future<void> showWindow() async {
    if (!_isDesktop()) return;

    await windowManager.show();
    await windowManager.focus();

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

    await windowManager.hide();
  }

  /// Quit application
  @override
  Future<void> quitApp() async {
    if (!_isDesktop()) return;

    // Only explicit quit stops generation. Closing to tray keeps work alive.
    // OS termination may skip this hook; durable leases remain the recovery path.
    try {
      await chat_runs.shutdownChatRuns().timeout(
        const Duration(milliseconds: 750),
      );
    } catch (_) {
      ui.debugPrint('Chat shutdown did not finish; durable recovery remains active.');
    }
    // 解除 close 拦截后真正退出
    await windowManager.setPreventClose(false);
    await windowManager.close();
  }

  @override
  void startDragging() {
    windowManager.startDragging();
  }

  @override
  void minimize() {
    windowManager.minimize();
  }

  @override
  void toggleMaximize() {
    windowManager.isMaximized().then((maximized) {
      if (maximized) {
        windowManager.unmaximize();
      } else {
        windowManager.maximize();
      }
    });
  }

  /// 事件回调入口（由 _WindowEventRelay 转发）。
  void _onClose() {
    // setPreventClose(true) 时拦截关闭：隐藏而非退出（托盘/后台运行）。
    windowManager.hide();
  }

  void _onMovedOrResized() => _scheduleGeometrySave();

  @override
  void dispose() {
    _saveGeometryDebounce?.cancel();
    final relay = _eventRelay;
    if (relay != null) {
      windowManager.removeListener(relay);
      _eventRelay = null;
    }
  }
}

/// window_manager 的 WindowListener 回调转发到 DesktopWindowService。
class _WindowEventRelay with WindowListener {
  _WindowEventRelay(this._service);
  final DesktopWindowService _service;

  @override
  void onWindowClose() => _service._onClose();

  @override
  void onWindowMoved() => _service._onMovedOrResized();

  @override
  void onWindowResized() => _service._onMovedOrResized();
}
