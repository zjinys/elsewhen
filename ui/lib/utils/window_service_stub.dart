import 'window_service.dart';

/// 移动端 no-op 实现：nativeapi/cnativeapi 只有桌面原生库，其 FFI 代码还会把
/// Android AOT 编译器打崩（Class with illegal cid），因此移动端不链接、全部空转。
class StubWindowService implements WindowService {
  @override
  WindowMode currentMode = WindowMode.main;

  @override
  Future<void> applyMainChrome() async {}

  @override
  Future<void> applyCaptureChrome() async {}

  @override
  Future<void> initialize() async {}

  @override
  Future<void> switchToMainMode() async {}

  @override
  Future<void> switchToCaptureMode() async {}

  @override
  Future<void> hideWindow() async {}

  @override
  Future<void> showWindow() async {}

  @override
  Future<void> toggleMode() async {}

  @override
  Future<bool> restoreMainBounds() async => false;

  @override
  Future<void> closeWindow() async {}

  @override
  Future<void> quitApp() async {}

  @override
  void startDragging() {}

  @override
  void minimize() {}

  @override
  void toggleMaximize() {}

  @override
  void dispose() {}
}
