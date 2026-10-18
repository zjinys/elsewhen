import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';

import 'package:elsewhen_ui/main.dart';
import 'package:elsewhen_ui/models/app_config.dart';
import 'package:elsewhen_ui/providers/app_provider.dart';
import 'package:elsewhen_ui/screens/capture_screen.dart';

void main() {
  testWidgets('App launches successfully', (WidgetTester tester) async {
    // Override initialization to skip platform-specific services
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 桌面宽度（≥1024）渲染宽侧栏：底部导航带「对话 / 知识」文字。
    // 默认 800×600 测试窗口处于「平板/小窗」区间，侧栏是 64px 纯图标轨，
    // 没有文字可查——这不是回归，是响应式设计的预期形态。
    tester.view.physicalSize = const Size(1440, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(mode: AppMode.main, databasePath: ':memory:'),
        ),
      ),
    );

    // Wait for async operations to complete
    await tester.pumpAndSettle();

    expect(find.text('对话'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
    expect(find.text('编辑器测试'), findsNothing);
    expect(find.byType(SvgPicture), findsOneWidget);
  });

  testWidgets('Tablet window renders 64px icon rail sidebar', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 平板/小窗区间（600–1024）：侧栏收为纯图标轨，不出现导航文字。
    tester.view.physicalSize = const Size(900, 700);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(mode: AppMode.main, databasePath: ':memory:'),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 图标轨：域图标在、文字不在
    expect(find.byIcon(Icons.chat_bubble_outline), findsOneWidget);
    expect(find.text('对话'), findsNothing);
    expect(find.text('知识'), findsNothing);
  });

  testWidgets('Phone width uses drawer instead of sidebar', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 手机宽度（<600）：无内嵌侧栏，导航收进抽屉。
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(mode: AppMode.main, databasePath: ':memory:'),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 抽屉关闭时导航不可见；打开抽屉后出现文字导航。
    expect(find.text('对话'), findsNothing);
    final scaffoldState = tester.firstState<ScaffoldState>(
      find.byType(Scaffold),
    );
    scaffoldState.openDrawer();
    await tester.pumpAndSettle();
    expect(find.text('对话'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
  });

  testWidgets('Capture mode launches successfully', (
    WidgetTester tester,
  ) async {
    // Override initialization to skip platform-specific services
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(mode: AppMode.capture),
          // CaptureScreen 已从 main.dart 移到桌面入口（其 window_service →
          // nativeapi 依赖不能进 Android 可达图），改为构造器注入。不传则
          // capture 模式会回退到 MainScreen，测不到快速记录界面。
          captureScreenBuilder: () => const CaptureScreen(),
        ),
      ),
    );

    await tester.pumpAndSettle();

    // Verify capture window: Alfred 式快速记录输入框（旧版 '快速记录' 按钮已移除）
    expect(find.byType(TextField), findsOneWidget);
    expect(find.text('记录此刻的想法...'), findsOneWidget);
    expect(find.byType(SvgPicture), findsOneWidget);
  });
}
