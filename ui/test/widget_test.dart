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
    expect(find.text('知识库'), findsOneWidget);
    expect(find.text('知识库'), findsOneWidget);
    expect(find.text('编辑器测试'), findsNothing);
    expect(find.byType(SvgPicture), findsOneWidget);
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
