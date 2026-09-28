import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/providers/app_provider.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:elsewhen_ui/widgets/ai_provider_setup_hint.dart';

Widget _app({required bool configured}) {
  return ProviderScope(
    overrides: [
      aiProviderConfiguredProvider.overrideWith((ref) async => configured),
    ],
    child: const MaterialApp(home: Scaffold(body: AiProviderSetupHint())),
  );
}

void main() {
  testWidgets('未配置 AI Provider 时显示首次运行提示', (tester) async {
    await tester.pumpWidget(_app(configured: false));
    await tester.pumpAndSettle();

    expect(find.textContaining('首次使用提示'), findsOneWidget);
    expect(find.text('去配置'), findsOneWidget);
  });

  testWidgets('已配置 AI Provider 时不显示提示', (tester) async {
    await tester.pumpWidget(_app(configured: true));
    await tester.pumpAndSettle();

    expect(find.textContaining('首次使用提示'), findsNothing);
    expect(find.text('去配置'), findsNothing);
  });

  testWidgets('点击关闭后提示隐藏', (tester) async {
    await tester.pumpWidget(_app(configured: false));
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();

    expect(find.textContaining('首次使用提示'), findsNothing);
  });

  testWidgets('点击「去配置」进入设置界面', (tester) async {
    await tester.pumpWidget(_app(configured: false));
    await tester.pumpAndSettle();

    await tester.tap(find.text('去配置'));
    await tester.pumpAndSettle();

    expect(find.byType(SettingsScreen), findsOneWidget);
  });
}
