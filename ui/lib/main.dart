import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';
import 'dart:io';

import 'theme/app_theme.dart';
import 'models/app_config.dart';
import 'models/settings.dart';
import 'screens/main_screen.dart';
import 'screens/capture_screen.dart';
import 'providers/app_provider.dart';
import 'providers/settings_provider.dart';

void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  final config = AppConfig.fromArgs(args);

  // Initialize window manager for desktop platforms
  if (Platform.isLinux || Platform.isMacOS || Platform.isWindows) {
    await windowManager.ensureInitialized();

    WindowOptions windowOptions = config.mode == AppMode.capture
        ? WindowOptions(
            size: const Size(650, 180),
            center: true,
            backgroundColor: const Color(0xFF1C1C1E),
            skipTaskbar: false,
            titleBarStyle: TitleBarStyle.hidden,
            alwaysOnTop: true,
          )
        : WindowOptions(
            size: const Size(1920, 1080),
            minimumSize: const Size(800, 600),
            center: true,
            backgroundColor: Colors.transparent,
            skipTaskbar: false,
            title: 'Elsewhen',
            titleBarStyle: TitleBarStyle.hidden,
          );

    windowManager.waitUntilReadyToShow(windowOptions, () async {
      // Intercept the window close button (X): hide the window instead of
      // destroying it, so the app keeps running in the background.
      // Without this, GTK destroys the window and the `onWindowClose` →
      // `windowManager.hide()` path crashes with GTK critical assertions.
      await windowManager.setPreventClose(true);
      await windowManager.show();
      await windowManager.center(animate: true);
      await windowManager.focus();
    });
  }

  runApp(
    ProviderScope(
      child: ElsewhenApp(config: config),
    ),
  );
}

class ElsewhenApp extends ConsumerStatefulWidget {
  final AppConfig config;

  const ElsewhenApp({
    super.key,
    required this.config,
  });

  @override
  ConsumerState<ElsewhenApp> createState() => _ElsewhenAppState();
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
    final lightTheme = AppTheme.buildTheme(settings.themePreset, Brightness.light);
    final darkTheme = AppTheme.buildTheme(settings.themePreset, Brightness.dark);
    final activeTheme = brightness == Brightness.dark ? darkTheme : lightTheme;
    // 把当前色卡（含 accent）同步给自定义组件用的全局色板
    AppTheme.apply(brightness, accent: activeTheme.colorScheme.primary);

    // 深浅 / 配色变化时整棵子树重建，保证用 AppTheme.* 硬编码的自定义配色全部刷新
    final themeKey = ValueKey('${settings.themeMode.name}-${settings.themePreset.name}');

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
      home: initAsync.when(
        data: (initialized) {
          if (!initialized) {
            return const Scaffold(
              body: Center(
                child: Text('Initialization failed'),
              ),
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
                CircularProgressIndicator(
                  color: AppTheme.accentPrimary,
                ),
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
                Icon(
                  Icons.error_outline,
                  color: AppTheme.error,
                  size: 48,
                ),
                const SizedBox(height: 16),
                Text(
                  'Failed to initialize',
                  style: TextStyle(
                    color: AppTheme.error,
                    fontSize: 16,
                  ),
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
    );
  }
}