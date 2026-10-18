import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:elsewhen_ui/main.dart';
import 'package:elsewhen_ui/models/app_config.dart';
import 'package:elsewhen_ui/providers/app_provider.dart';

void main() {
  testWidgets('continuous 1px-per-frame resize across breakpoints', (tester) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );
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

    // 逐帧扫：1440 → 600 每步 8px（模拟真实拖动速度），再扫回来。
    for (var w = 1440.0; w >= 600.0; w -= 8) {
      tester.view.physicalSize = Size(w, 900);
      await tester.pump();
      final ex = tester.takeException();
      if (ex != null) fail('resize to $w threw: $ex');
    }
    for (var w = 600.0; w <= 1440.0; w += 8) {
      tester.view.physicalSize = Size(w, 900);
      await tester.pump();
      final ex = tester.takeException();
      if (ex != null) fail('resize to $w threw: $ex');
    }
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });
}
