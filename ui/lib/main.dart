import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:appflowy_editor/appflowy_editor.dart';

import 'theme/app_theme.dart';
import 'theme/content_font.dart';
import 'models/app_config.dart';
import 'models/settings.dart';
import 'screens/main_screen.dart';
import 'providers/app_provider.dart';
import 'providers/settings_provider.dart';
import 'utils/error_report.dart';
import 'utils/system_fonts.dart';

/// 移动端 / 通用入口：不可达任何 nativeapi 引用（其 FFI 结构会让
/// Android release AOT 崩溃）。桌面端窗口 chrome（无边框/尺寸/置顶）
/// 在 main_desktop.dart 里初始化，桌面构建用 --target=lib/main_desktop.dart。
void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  // 漏网异常（widget 构建失败、没 await 的 Future）也要进诊断日志，
  // 必须在 runApp 之前装。
  installGlobalErrorHandlers();

  // Android：系统状态栏保持显示（标题栏已按 SafeArea 避开）。
  // 图标配色跟随主题亮度：深色界面白图标、浅色界面深图标。
  // 这里只设一次初值；主题切换时由 ElsewhenApp.build 里的
  // AnnotatedRegion 再按当前 brightness 覆盖。
  if (!kIsWeb && Platform.isAndroid) {
    SystemChrome.setSystemUIOverlayStyle(
      const SystemUiOverlayStyle(statusBarIconBrightness: Brightness.light),
    );
  }

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

  const ElsewhenApp({
    super.key,
    required this.config,
    this.captureScreenBuilder,
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

    return AnnotatedRegion<SystemUiOverlayStyle>(
      // Android 状态栏图标跟随主题亮度：深色界面白图标、浅色界面深图标。
      // 只影响移动端（桌面端无系统状态栏概念），iOS 由系统自动处理。
      value: SystemUiOverlayStyle(
        statusBarIconBrightness: brightness == Brightness.dark
            ? Brightness.light
            : Brightness.dark,
        statusBarBrightness: brightness == Brightness.dark
            ? Brightness.dark
            : Brightness.light,
      ),
      child: ContentFont(
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
          // 桌面窗口不裁圆角（FlClash 纪律）：窗口不透明直角，圆角交给系统
          // （Win11 DWM 自动圆角、macOS 系统圆角、Linux 直角）。自裁圆角需要
          // 透明窗口背景，而透明像素在 Windows 上被 DWM 合成为白色留一圈白边。
          home: Builder(
            builder: (context) => initAsync.when(
              data: (initialized) {
                if (!initialized) {
                  return const Scaffold(
                    body: Center(child: Text('Initialization failed')),
                  );
                }

                final captureBuilder = widget.captureScreenBuilder;
                return KeyedSubtree(
                  key: themeKey,
                  child:
                      widget.config.mode == AppMode.capture &&
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
                          Text(
                            'Elsewhen',
                            style: TextStyle(
                              color: AppTheme.textPrimary,
                              fontSize: 22,
                              letterSpacing: 1.5,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 20),
                          SizedBox(
                            width: 20,
                            height: 20,
                            child: CircularProgressIndicator(
                              strokeWidth: 2,
                              color: AppTheme.accentPrimary,
                            ),
                          ),
                          const SizedBox(height: 14),
                          Text(
                            '正在启动…',
                            style: TextStyle(
                              color: AppTheme.textSecondary,
                              fontSize: 12,
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
      ),
    );
  }
}
