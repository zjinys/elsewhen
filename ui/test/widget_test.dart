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

    // Verify that the app starts with the event list view
    expect(find.text('Elsewhen'), findsOneWidget);
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

    // Verify capture screen
    expect(find.text('快速记录'), findsOneWidget);
  });
}
