import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'main.dart' show ElsewhenApp;
import 'models/app_config.dart';
import 'screens/capture_screen.dart';
import 'utils/editor_localization.dart';
import 'utils/error_report.dart';
import 'utils/single_instance.dart';
import 'utils/window_geometry_store.dart';
import 'utils/window_service.dart';
import 'utils/window_service_desktop.dart';

/// 桌面端入口（Linux/macOS/Windows）：初始化窗口 chrome（无边框/尺寸/置顶/圆角），
/// 然后复用通用 [ElsewhenApp]。
///
/// 与 main.dart 分离的原因：桌面窗口服务（window_manager 插件，含原生
/// method channel 注册）只在桌面端需要。独立入口保证 Android 的 main.dart
/// 可达图干净；桌面构建用 --target=lib/main_desktop.dart。
void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  // AppFlowy editor 的占位符/斜杠菜单读静态 l10n，启动时先加载中文文案。
  await configureEditorLocalization();
  // 漏网异常也要进诊断日志；此处早于窗口 chrome 初始化，任何一步崩掉都能记到。
  installGlobalErrorHandlers();

  // 注册桌面窗口实现（window_manager），必须在任何 WindowService() 使用之前。
  WindowService.register(DesktopWindowService());

  final config = AppConfig.fromArgs(args);
  final isDesktop = Platform.isLinux || Platform.isMacOS || Platform.isWindows;

  // 单实例守卫：已有实例在跑时，把命令转发给它（capture → 进入捕获模式，
  // 否则 → 唤回主窗）后本进程退出。必须早于窗口 chrome / Rust bridge 初始化，
  // 否则第二个进程会先抢数据库。
  if (isDesktop) {
    final primary = await SingleInstance.ensurePrimary(
      identity: WindowGeometryStore.resolveDataDir() ?? 'elsewhen',
      command: config.mode == AppMode.capture ? 'capture' : 'show',
      onCommand: (command) {
        final windowService = WindowService();
        if (command == 'capture') {
          unawaited(windowService.switchToCaptureMode());
        } else {
          unawaited(windowService.switchToMainMode());
        }
      },
    );
    if (!primary) exit(0);
  }

  // window_manager 标准启动编排：ensureInitialized → setTitleBarStyle(hidden)
  // → WindowOptions → waitUntilReadyToShow → show（FlClash 同款纪律）。
  // 窗口不透明直角，圆角交给系统；不设透明背景（Windows DWM 白边坑）。
  if (isDesktop) {
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
