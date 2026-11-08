import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_locale.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  testWidgets('切换界面语言保留 Provider 未保存表单，语言选择落盘', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 1100));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final repo = (await tester.runAsync(() => createIsolatedBridge()))!;
    final container = ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: Consumer(
          builder: (context, ref, _) => MaterialApp(
            locale: resolveAppLocale(ref.watch(settingsProvider).language, [
              const Locale('en'),
            ]),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const SettingsScreen(),
          ),
        ),
      ),
    );
    Future<void> settle() async {
      await tester.pump();
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
      await tester.pumpAndSettle();
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pumpAndSettle();
    }

    Future<void> waitForLanguage(String? expected) async {
      // Each FFI result can schedule another editor/runtime bridge operation.
      // Flush widget microtasks between real event-loop turns until committed.
      for (var attempt = 0; attempt < 20; attempt++) {
        if (container.read(settingsProvider).language == expected) return;
        await settle();
      }
      expect(container.read(settingsProvider).language, expected);
    }

    await settle();
    final promptTitle = find.text('提示词');
    await tester.ensureVisible(promptTitle);
    expect(find.text('加载提示词'), findsNothing);
    expect(find.text('查看运行版本'), findsNothing);
    for (var i = 0; i < 9; i++) {
      await tester.tap(promptTitle);
      await tester.pump();
    }
    expect(find.text('加载提示词'), findsNothing);
    await tester.pump(const Duration(seconds: 3));
    await tester.tap(promptTitle);
    await tester.pump();
    expect(find.text('加载提示词'), findsNothing);
    for (var i = 0; i < 9; i++) {
      await tester.tap(promptTitle);
      await tester.pump();
    }
    expect(find.text('加载提示词'), findsOneWidget);
    expect(find.text('查看运行版本'), findsOneWidget);
    await tester.ensureVisible(find.text('添加'));
    await tester.tap(find.text('添加'));
    await settle();
    final keyField = find.byKey(const ValueKey('provider-api-key'));
    await tester.enterText(keyField, 'unsaved-synthetic-key');
    await tester.runAsync(
      () => container.read(settingsProvider.notifier).updateLanguage('en'),
    );
    await settle();
    expect(find.text('Add AI provider'), findsOneWidget);
    expect(find.text('Manual input'), findsOneWidget);
    expect(
      tester.widget<TextFormField>(keyField).controller!.text,
      'unsaved-synthetic-key',
    );
    await tester.tap(find.widgetWithText(TextButton, 'Cancel'));
    await settle();
    await tester.tap(find.text('Appearance').first);
    await settle();
    await tester.tap(find.text('English').first);
    await settle();
    await tester.runAsync(() async {
      await tester.tap(find.text('简体中文').last);
      await Future<void>.delayed(const Duration(milliseconds: 200));
    });
    await settle();
    await waitForLanguage('zh-Hans');
    expect(container.read(settingsProvider).language, 'zh-Hans');
    expect(
      (await tester.runAsync(() => api.getUiLanguagePreference()))!.locale,
      'zh-Hans',
    );
    expect(find.text('语言'), findsOneWidget);
    await tester.tap(find.text('简体中文').first);
    await settle();
    // The theme selector also has a system option; scope this to the open menu.
    await tester.runAsync(() async {
      await tester.tap(find.text('跟随系统').last);
      await Future<void>.delayed(const Duration(milliseconds: 200));
    });
    await settle();
    await waitForLanguage(null);
    expect(container.read(settingsProvider).language, isNull);
    expect(
      (await tester.runAsync(() => api.getUiLanguagePreference()))!.locale,
      isNull,
    );
    expect(find.text('Language'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
