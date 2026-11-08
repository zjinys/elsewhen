import 'window_service.dart';

import 'package:flutter/widgets.dart' show Locale;

/// 移动端 no-op 实现：桌面窗口插件（window_manager）只有桌面原生库，
/// Android AOT 编译器打崩（Class with illegal cid），因此移动端不链接、全部空转。
class StubWindowService implements WindowService {
  @override
  WindowMode currentMode = WindowMode.main;

  @override
  Future<void> applyMainChrome() async {}

  @override
  Future<void> initialize() async {}
  @override
  Future<void> updateLocale(Locale locale) async {}

  @override
  Future<void> switchToMainMode() async {}

  @override
  Future<void> hideWindow() async {}

  @override
  Future<void> showWindow() async {}

  @override
  Future<bool> restoreMainBounds() async => false;

  @override
  Future<void> closeWindow() async {}

  @override
  Future<void> quitApp() async {}

  @override
  CloseAction get closeAction => CloseAction.minimizeToTray;

  @override
  Future<void> setCloseAction(CloseAction action) async {}

  @override
  bool get trayAvailable => false;

  @override
  Future<void> requestClose() async {}

  @override
  void startDragging() {}

  @override
  void minimize() {}

  @override
  void toggleMaximize() {}

  @override
  void dispose() {}
}
