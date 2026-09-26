import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:nativeapi/nativeapi.dart' hide Brightness;

import 'theme/app_theme.dart';
import 'models/app_config.dart';
import 'models/settings.dart';
import 'screens/main_screen.dart';
import 'screens/capture_screen.dart';
import 'providers/app_provider.dart';
import 'providers/settings_provider.dart';
import 'utils/window_service.dart';

void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

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
      child: ElsewhenApp(config: config),
    ),
  );
}

class ElsewhenApp extends ConsumerStatefulWidget {
  final AppConfig config;

  const ElsewhenApp({super.key, required this.config});

  @override
  ConsumerState<ElsewhenApp> createState() => _ElsewhenAppState();
}

/// 窗口圆角：非最大化时给整棵子树（MaterialApp 之上）套 ClipRRect。
/// 最大化/全屏时自动恢复直角（监听 nativeapi 的 maximized/restored 事件）。
class _WindowRoundedClipper extends StatefulWidget {
  const _WindowRoundedClipper({required this.child});
  final Widget child;

  static const double _radius = 12;

  @override
  State<_WindowRoundedClipper> createState() => _WindowRoundedClipperState();
}

class _WindowRoundedClipperState extends State<_WindowRoundedClipper> {
  bool _maximized = false;
  ListenerId? _listenerId;

  /// 测试环境（flutter test）下没有 libcnativeapi.so，WindowManager.addListener
  /// 会直接抛 ArgumentError；跳过注册，圆角仍生效、只是最大化时不会恢复直角
  /// （测试里也不会最大化）。
  static bool get _isTestEnv =>
      Platform.environment.containsKey('FLUTTER_TEST');

  @override
  void initState() {
    super.initState();
    if (!_isTestEnv &&
        (Platform.isLinux || Platform.isMacOS || Platform.isWindows)) {
      _listenerId = WindowManager.instance.addListener((event) {
        switch (event) {
          case WindowMaximizedEvent():
            _setMaximized(true);
          case WindowRestoredEvent():
            _setMaximized(false);
          default:
            break;
        }
      });
    }
  }

  void _setMaximized(bool v) {
    if (!mounted || _maximized == v) return;
    setState(() => _maximized = v);
  }

  @override
  void dispose() {
    final id = _listenerId;
    if (id != null) WindowManager.instance.removeListener(id);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_maximized) return widget.child;
    return ClipRRect(
      borderRadius: BorderRadius.circular(_WindowRoundedClipper._radius),
      child: widget.child,
    );
  }
}

class _ElsewhenAppState extends ConsumerState<ElsewhenApp> {
  @override
  void initState() {
    super.initState();
    // Trigger initialization
    Future.microtask(() async {
      try {
        await ref.read(appInitializationProvider.future);
        // 桥接初始化完成后再加载持久化的主题偏好（app_meta 里的 theme_mode / theme_preset），
        // 避免与 RustLib.init() 竞态
        ref.read(settingsProvider.notifier).loadThemeFromBridge();
      } catch (e) {
        debugPrint('loadThemeFromBridge skipped: $e');
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    // Watch initialization status
    final initAsync = ref.watch(appInitializationProvider);
    final settings = ref.watch(settingsProvider);

    final brightness = switch (settings.themeMode) {
      AppThemeMode.light => Brightness.light,
      AppThemeMode.dark => Brightness.dark,
      AppThemeMode.system => MediaQuery.platformBrightnessOf(context),
    };
    final lightTheme = AppTheme.buildTheme(
      settings.themePreset,
      Brightness.light,
      settings.fontName,
    );
    final darkTheme = AppTheme.buildTheme(
      settings.themePreset,
      Brightness.dark,
      settings.fontName,
    );
    final activeTheme = brightness == Brightness.dark ? darkTheme : lightTheme;
    // 把当前色卡（含 accent）同步给自定义组件用的全局色板
    AppTheme.apply(brightness, accent: activeTheme.colorScheme.primary);

    // 深浅 / 配色 / 字体变化时整棵子树重建，保证用 AppTheme.* 硬编码的自定义配色全部刷新
    final themeKey = ValueKey(
      '${settings.themeMode.name}-${settings.themePreset.name}-${settings.fontName}',
    );

    return MaterialApp(
      title: 'Elsewhen',
      theme: lightTheme,
      darkTheme: darkTheme,
      themeMode: switch (settings.themeMode) {
        AppThemeMode.light => ThemeMode.light,
        AppThemeMode.dark => ThemeMode.dark,
        AppThemeMode.system => ThemeMode.system,
      },
      debugShowCheckedModeBanner: false,
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: _WindowRoundedClipper(
        child: initAsync.when(
          data: (initialized) {
            if (!initialized) {
              return const Scaffold(
                body: Center(child: Text('Initialization failed')),
              );
            }

            return KeyedSubtree(
              key: themeKey,
              child: widget.config.mode == AppMode.capture
                  ? const CaptureScreen()
                  : const MainScreen(),
            );
          },
          loading: () => Scaffold(
            backgroundColor: AppTheme.surface0,
            body: Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  CircularProgressIndicator(color: AppTheme.accentPrimary),
                  const SizedBox(height: 16),
                  Text(
                    'Initializing Elsewhen...',
                    style: TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 14,
                    ),
                  ),
                ],
              ),
            ),
          ),
          error: (error, stack) => Scaffold(
            body: Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.error_outline, color: AppTheme.error, size: 48),
                  const SizedBox(height: 16),
                  Text(
                    'Failed to initialize',
                    style: TextStyle(color: AppTheme.error, fontSize: 16),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    error.toString(),
                    style: TextStyle(
                      color: AppTheme.textTertiary,
                      fontSize: 12,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
