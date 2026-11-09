import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/prompt_settings.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';

import 'support/isolated_bridge.dart';

void main() {
  testWidgets(
    'format preview manifest displays source preservation instead of a fixed chat language',
    (tester) async {
      await tester.runAsync(createIsolatedBridge);
      await tester.runAsync(() async {
        await expectLater(
          api.formatImportContent(content: '来源原文，只排版'),
          throwsA(
            predicate<Object>(
              (error) => error.toString().contains(
                'No active AI provider configuration',
              ),
            ),
          ),
        );
        final bundles = await api.listPromptBundles(offset: 0);
        final policy = jsonDecode(
          bundles
              .singleWhere((b) => b.ownerId.startsWith('import_format:'))
              .responsePolicyJson,
        ) as Map<String, dynamic>;
        expect(policy['mode'], 'preserve_source');
        expect(policy['locale'], 'und');
      });
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          supportedLocales: AppLocalizations.supportedLocales,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(body: PromptSettings(showDiagnostics: true)),
        ),
      );
      await tester.tap(find.text('View run versions'));
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.textContaining('preview ·'));
      await tester.pumpAndSettle();
      expect(
        find.text(
          'Output policy: preserve the source language (formatting only)',
        ),
        findsOneWidget,
      );
      expect(find.textContaining('und ('), findsNothing);
    },
  );
}
