import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:appflowy_editor/appflowy_editor.dart';

import 'theme/app_theme.dart';
import 'theme/content_font.dart';
import 'models/app_config.dart';
import 'models/settings.dart';
import 'screens/main_screen.dart';
import 'providers/app_provider.dart';
import 'providers/settings_provider.dart';
import 'utils/system_fonts.dart';

/// 移动端 / 通用入口：不可达任何 nativeapi 引用（其 FFI 结构会让
/// Android release AOT 崩溃）。桌面端窗口 chrome（无边框/尺寸/置顶）
/// 在 main_desktop.dart 里初始化，桌面构建用 --target=lib/main_desktop.dart。
void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  final config = AppConfig.fromArgs(args);

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

  /// capture 模式的屏幕构造器。桌面端由 main_desktop.dart 传入 CaptureScreen
  /// （其依赖 window_service → nativeapi，不可进入 Android 的 main 可达图）；
  /// 移动端为 null（移动端无 capture 模式）。
  final Widget Function()? captureScreenBuilder;

  const ElsewhenApp({super.key, required this.config, this.captureScreenBuilder});

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
    final lightTheme = AppTheme.buildTheme(
      settings.themePreset,
      Brightness.light,
    );
    final darkTheme = AppTheme.buildTheme(
      settings.themePreset,
      Brightness.dark,
    );
    final activeTheme = brightness == Brightness.dark ? darkTheme : lightTheme;
    // 把当前色卡（含 accent）同步给自定义组件用的全局色板
    AppTheme.apply(brightness, accent: activeTheme.colorScheme.primary);

    // 深浅 / 配色变化时整棵子树重建，保证用 AppTheme.* 硬编码的自定义配色全部刷新。
    // 内容字体不进 key：只有内容区依赖 [ContentFont]，切换时按需重建即可。
    final themeKey = ValueKey(
      '${settings.themeMode.name}-${settings.themePreset.name}',
    );

    // 每次 build 重新解析：本地字体索引就绪后（loadThemeFromBridge 末尾的
    // state 刷新）同一存值可能从回退值变为真实家族名。
    final contentFamily = resolveFontFamily(
      settings.fontName,
      fallback: AppFonts.defaultFont,
    );

    return ContentFont(
      family: contentFamily,
      child: MaterialApp(
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
        home: ClipRRect(
          borderRadius: BorderRadius.circular(12),
          child: initAsync.when(
            data: (initialized) {
              if (!initialized) {
                return const Scaffold(
                  body: Center(child: Text('Initialization failed')),
                );
              }

              final captureBuilder = widget.captureScreenBuilder;
              return KeyedSubtree(
                key: themeKey,
                child: widget.config.mode == AppMode.capture &&
                        captureBuilder != null
                    ? captureBuilder()
                    : const MainScreen(),
              );
            },
            loading: () => Scaffold(
              backgroundColor: AppTheme.surface0,
              // LayoutBuilder + SingleChildScrollView：无边框/透明窗口首帧可能给出
              // 极小甚至为 0 的高度约束，Center+Column 直接溢出 580px。改为在
              // 约束高度内滚动，彻底消除任意高度下的 RenderFlex overflow。
              body: LayoutBuilder(
                builder: (context, constraints) => SingleChildScrollView(
                  child: ConstrainedBox(
                    constraints: BoxConstraints(
                      minHeight: constraints.maxHeight,
                    ),
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      mainAxisSize: MainAxisSize.min,
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
              ),
            ),
            error: (error, stack) => Scaffold(
              body: Center(
                // 错误消息可能很长：限宽 + 可滚动，避免小窗口下 Column 溢出。
                child: SingleChildScrollView(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 32),
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 560),
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        mainAxisSize: MainAxisSize.min,
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
                            textAlign: TextAlign.center,
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
            ),
          ),
        ),
      ),
    );
  }
}
