import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/l10n/app_locale.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('resource keys and placeholders stay aligned; protocol examples remain literal', () {
    final chinese = jsonDecode(
      File('lib/l10n/app_zh.arb').readAsStringSync(),
    ) as Map<String, dynamic>;
    final english = jsonDecode(
      File('lib/l10n/app_en.arb').readAsStringSync(),
    ) as Map<String, dynamic>;
    final keys = chinese.keys.where((k) => !k.startsWith('@')).toSet();
    expect(english.keys.where((k) => !k.startsWith('@')).toSet(), keys);
    final en = lookupAppLocalizations(const Locale('en'));
    expect(en.approximateTokens(1), 'About 1 token');
    expect(en.approximateTokens(2), 'About 2 tokens');
    expect(en.secretVaultDescription, contains('{{secret:code}}'));
    expect(en.deleteSecretConfirmation('用户填写的名称'), contains('用户填写的名称'));
    for (final key in keys) {
      expect(chinese[key], isNotEmpty, reason: key);
      expect(english[key], isNotEmpty, reason: key);
    }
  });

  test(
    'missing language stays Chinese, explicit null restores system mode',
    () {
      final initial = AppSettings.defaults();
      expect(initial.language, 'zh-Hans');
      final english = initial.copyWith(language: 'en');
      expect(english.copyWith(fontSize: 18).language, 'en');
      expect(english.copyWith(language: null).language, isNull);
      expect(resolveAppLocale('en', [const Locale('zh')]), const Locale('en'));
      expect(
        resolveAppLocale('zh-Hans', [const Locale('en')]),
        const Locale('zh'),
      );
      expect(
        resolveAppLocale(null, [const Locale('en', 'GB')]),
        const Locale('en'),
      );
      expect(
        resolveAppLocale(null, [const Locale('fr'), const Locale('zh', 'CN')]),
        const Locale('zh'),
      );
      expect(resolveAppLocale(null, [const Locale('fr')]), const Locale('zh'));
    },
  );

  testWidgets(
    'application and Material delegates resolve both display languages',
    (tester) async {
      for (final locale in [const Locale('en'), const Locale('zh')]) {
        await tester.pumpWidget(
          MaterialApp(
            locale: locale,
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Builder(
              builder: (context) => Column(
                children: [
                  Text(AppLocalizations.of(context).initializationFailed),
                  Text(MaterialLocalizations.of(context).cancelButtonLabel),
                ],
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(
          find.text(
            locale.languageCode == 'en' ? 'Initialization failed' : '初始化失败',
          ),
          findsOneWidget,
        );
        expect(
          find.text(locale.languageCode == 'en' ? 'Cancel' : '取消'),
          findsOneWidget,
        );
      }
    },
  );
}
