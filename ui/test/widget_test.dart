import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:elsewhen_ui/main.dart';
import 'package:elsewhen_ui/models/app_config.dart';

void main() {
  testWidgets('App launches successfully', (WidgetTester tester) async {
    // Build our app and trigger a frame.
    await tester.pumpWidget(
      ProviderScope(
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
    await tester.pumpWidget(
      ProviderScope(
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
