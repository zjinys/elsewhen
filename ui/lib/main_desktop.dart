import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'main.dart' show ElsewhenApp;
import 'models/app_config.dart';
import 'screens/capture_screen.dart';
import 'utils/window_service.dart';
import 'utils/window_service_desktop.dart';

/// 桌面端入口（Linux/macOS/Windows）：初始化窗口 chrome（无边框/尺寸/置顶/圆角），
/// 然后复用通用 [ElsewhenApp]。
///
/// 与 main.dart 分离的原因：桌面窗口服务依赖 nativeapi/cnativeapi，
/// 其 FFI 结构（native_window_event_t 等 union）会让 Android release AOT
/// 编译器崩溃。通过独立入口保证 Android 的 main.dart 可达图中完全
/// 没有 nativeapi；桌面构建用 --target=lib/main_desktop.dart。
void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  // 注册桌面窗口实现（nativeapi），必须在任何 WindowService() 使用之前。
  WindowService.register(DesktopWindowService());

  final config = AppConfig.fromArgs(args);

  // nativeapi（替代 window_manager）：没有 waitUntilReadyToShow + WindowOptions，
  // 改为启动期尽早应用无边框/尺寸/位置/置顶等配置——Flutter runner 首帧
  // 会自动显示窗口，这里抢在首帧前把外观与几何就位，避免原生标题栏/默认
  // 尺寸闪现。
  if (Platform.isLinux || Platform.isMacOS || Platform.isWindows) {
    final windowService = WindowService();
    if (config.mode == AppMode.capture) {
      await windowService.applyCaptureChrome();
    } else {
      await windowService.applyMainChrome();
    }
  }

  runApp(
    // Riverpod 3 默认对失败 provider 指数退避自动重试；桥接/DB 失败多为
    // 确定性错误，重试只会刷日志，关闭以保持 v2 的失败即停行为。
    ProviderScope(
      retry: (retryCount, error) => null,
      child: ElsewhenApp(
        config: config,
        captureScreenBuilder: () => const CaptureScreen(),
      ),
    ),
  );
}
