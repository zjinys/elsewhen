import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:elsewhen_ui/main.dart';
import 'package:elsewhen_ui/models/app_config.dart';
import 'package:elsewhen_ui/providers/app_provider.dart';

void main() {
  testWidgets('App launches successfully', (WidgetTester tester) async {
    // Override initialization to skip platform-specific services
    final container = ProviderContainer(
      overrides: [
        appInitializationProvider.overrideWith((ref) async => true),
      ],
    );

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(
            mode: AppMode.main,
            databasePath: ':memory:',
          ),
        ),
      ),
    );

    // Wait for async operations to complete
    await tester.pumpAndSettle();

    // Verify that the app starts with the conversation view
    // （旧的 header「Elsewhen」已移除，改为验证对话 tab 工具栏的新建入口）
    expect(find.text('新建对话'), findsOneWidget);
    expect(find.text('知识库'), findsOneWidget);
  });

  testWidgets('Capture mode launches successfully', (WidgetTester tester) async {
    // Override initialization to skip platform-specific services
    final container = ProviderContainer(
      overrides: [
        appInitializationProvider.overrideWith((ref) async => true),
      ],
    );

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: ElsewhenApp(
          config: AppConfig(
            mode: AppMode.capture,
          ),
        ),
      ),
    );

    await tester.pumpAndSettle();

    // Verify capture window: Alfred 式快速记录输入框（旧版 '快速记录' 按钮已移除）
    expect(find.byType(TextField), findsOneWidget);
    expect(find.text('记录此刻的想法...'), findsOneWidget);
  });
}
