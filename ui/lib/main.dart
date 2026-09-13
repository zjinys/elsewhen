import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';
import 'dart:io';

import 'theme/app_theme.dart';
import 'models/app_config.dart';
import 'screens/main_screen.dart';
import 'screens/capture_screen.dart';
import 'providers/app_provider.dart';

void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  final config = AppConfig.fromArgs(args);

  // Initialize window manager for desktop platforms
  if (Platform.isLinux || Platform.isMacOS || Platform.isWindows) {
    await windowManager.ensureInitialized();

    WindowOptions windowOptions = config.mode == AppMode.capture
        ? WindowOptions(
            size: const Size(500, 240),
            center: true,
            backgroundColor: Colors.transparent,
            skipTaskbar: false,
            titleBarStyle: TitleBarStyle.hidden,
            alwaysOnTop: true,
          )
        : WindowOptions(
            size: const Size(1000, 700),
            minimumSize: const Size(800, 600),
            center: true,
            backgroundColor: Colors.transparent,
            skipTaskbar: false,
            title: 'Elsewhen',
          );

    windowManager.waitUntilReadyToShow(windowOptions, () async {
      await windowManager.show();
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
    Future.microtask(() {
      ref.read(appInitializationProvider);
    });
  }

  @override
  Widget build(BuildContext context) {
    // Watch initialization status
    final initAsync = ref.watch(appInitializationProvider);

    return MaterialApp(
      title: 'Elsewhen',
      theme: AppTheme.darkTheme,
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

          return widget.config.mode == AppMode.capture
              ? const CaptureScreen()
              : const MainScreen();
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
