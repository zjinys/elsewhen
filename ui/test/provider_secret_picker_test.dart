import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:elsewhen_ui/widgets/provider_key_warning_badge.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  testWidgets(
    'Provider selects a named key, restores selection, and accepts a direct key',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 1100));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final repo = (await tester.runAsync(() => createIsolatedBridge()))!;
      await tester.runAsync(() async {
        await api.saveNamedSecret(
          name: 'shared',
          title: '共用服务密钥',
          value: 'synthetic-key',
        );
        await repo.saveAiProviderConfig(
          const api.AiProviderConfigDto(
            id: '',
            name: 'Secret picker test',
            providerType: 'openai-compatible',
            baseUrl: 'https://example.invalid/v1',
            model: 'test',
            apiKeySource: '',
            apiKey: 'initial-key',
            hasApiKey: true,
            isActive: true,
            temperature: 0.7,
          ),
        );
      });
      for (final locale in [const Locale('zh'), const Locale('en')]) {
        final strings = lookupAppLocalizations(locale);
        await tester.pumpWidget(
          ProviderScope(
            overrides: [storageRepositoryProvider.overrideWithValue(repo)],
            child: MaterialApp(
              locale: locale,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: const SettingsScreen(),
            ),
          ),
        );
        Future<void> settle() async {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 200)),
          );
          await tester.pumpAndSettle();
        }

        await settle();
        expect(find.text(strings.settings), findsOneWidget);
        Future<void> open() async {
          final edit = find.descendant(
            of: find.byType(ProviderCardActions),
            matching: find.byTooltip(strings.edit),
          );
          await tester.ensureVisible(edit);
          await tester.tap(edit);
          await settle();
        }

        Future<void> save() async {
          await tester.tap(
            find.descendant(
              of: find.byType(AlertDialog),
              matching: find.widgetWithText(FilledButton, strings.save),
            ),
          );
          await settle();
        }

        await open();
        expect(find.byKey(const ValueKey('provider-api-key')), findsOneWidget);
        expect(find.text(strings.manualInput), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('provider-secret-picker')));
        await settle();
        await tester.tap(find.text('共用服务密钥'));
        await tester.pumpAndSettle();
        expect(find.text(strings.secretVault), findsOneWidget);
        await save();
        expect(
          (await tester.runAsync(() => repo.listAiProviderConfigs()))!
              .single
              .apiKeySource,
          'named_secret:shared',
        );
        await settle(); // Saving closes the dialog before the settings metadata reload finishes.
        await open();
        await settle();
        final keyField = find.byKey(const ValueKey('provider-api-key'));
        expect(
          tester.widget<TextFormField>(keyField).controller!.text,
          '共用服务密钥',
        );
        expect(find.text(strings.secretVault), findsOneWidget);
        await tester.enterText(keyField, 'replacement-direct-key');
        await tester.pump();
        expect(find.text(strings.manualInput), findsOneWidget);
        await save();
        expect(
          (await tester.runAsync(() => repo.listAiProviderConfigs()))!
              .single
              .apiKeySource,
          'database',
        );
        expect(tester.takeException(), isNull);
      }
    },
  );
}
