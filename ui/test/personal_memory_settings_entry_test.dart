import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/isolated_bridge.dart';
import 'support/localizations.dart';

void main() {
  testWidgets('settings opens memory directly without a provider', (
    tester,
  ) async {
    final repo = await tester.runAsync(() => createIsolatedBridge());
    await tester.binding.setSurfaceSize(const Size(1100, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo!)],
        child: MaterialApp(
          locale: const Locale('zh'),
          localizationsDelegates: appTestLocalizationDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: const SettingsScreen(),
        ),
      ),
    );
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 400)),
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('记忆').first);
    await tester.tap(find.text('记忆').first);
    await tester.pump(const Duration(milliseconds: 300));
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump(const Duration(seconds: 2));
    expect(find.text('记忆'), findsWidgets);
    expect(tester.takeException(), isNull);
  });
}
