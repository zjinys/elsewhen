import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/prompt_settings.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';

import 'support/isolated_bridge.dart';

void main() {
  testWidgets('提示词入口按需加载且未初始化桥接也可展示设置', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: PromptSettings())),
    );
    await tester.pumpAndSettle();
    expect(find.text('加载提示词'), findsNothing);
    expect(find.text('查看运行版本'), findsNothing);
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: PromptSettings(showDiagnostics: true)),
      ),
    );
    expect(find.text('加载提示词'), findsOneWidget);
    expect(find.text('查看运行版本'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'English template draft activation and reset stay independent from Chinese',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 1100));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.runAsync(() => createIsolatedBridge());
      Future<api.PromptDefinitionDto> policy(String locale) async =>
          (await api.listPromptDefinitions(locale: locale))
              .singleWhere((p) => p.key == 'chat.personal.policy');
      final chinese = (await tester.runAsync(() => policy('zh-Hans')))!;
      final english = (await tester.runAsync(() => policy('en')))!;
      expect(english.resolvedLocale, 'en');
      expect(english.builtinBody, startsWith('You are Elsewhen:'));
      expect(chinese.builtinBody, startsWith('你是「Elsewhen」'));
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          supportedLocales: AppLocalizations.supportedLocales,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(
            body: SingleChildScrollView(
              child: PromptSettings(showDiagnostics: true),
            ),
          ),
        ),
      );
      Future<void> settle() async {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 100)),
        );
        await tester.pumpAndSettle();
      }

      final language = find.byKey(const ValueKey('prompt-template-language'));
      await tester.tap(language);
      await tester.pumpAndSettle();
      await tester.tap(find.text('English').last);
      await tester.pumpAndSettle();
      // Selecting a variant still leaves the read-only preview unloaded.
      expect(find.byType(ListTile), findsNothing);
      await tester.tap(find.text('Load prompts'));
      await settle();
      final title = lookupAppLocalizations(const Locale('en'))
          .promptPersonalPolicy;
      await tester.tap(find.widgetWithText(ListTile, title));
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        english.currentBody,
      );
      const draft = 'Use concrete facts and ask one focused question. 保留用户原文。';
      await tester.enterText(find.byType(TextField), draft);
      await tester.tap(find.text('Save draft'));
      await settle();
      expect(
        (await tester.runAsync(() => policy('en')))!.currentRevisionId,
        english.currentRevisionId,
      );
      await tester.tap(find.text('Activate draft'));
      await settle();
      expect(find.byType(AlertDialog), findsNothing);
      final active = (await tester.runAsync(() => policy('en')))!;
      expect(active.currentBody, draft);
      expect(active.generation, english.generation + 1);
      expect(
        (await tester.runAsync(() => policy('zh-Hans')))!.currentRevisionId,
        chinese.currentRevisionId,
      );
      // Reset affects the displayed variant, even while the interface is English.
      await tester.tap(
        find.descendant(
          of: find.widgetWithText(ListTile, title),
          matching: find.byType(PopupMenuButton<String>),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Restore default'));
      await settle();
      expect(
        (await tester.runAsync(() => policy('en')))!.currentRevisionId,
        english.currentRevisionId,
      );
      final history = (await tester.runAsync(
        () =>
            api.listPromptRevisions(key: english.key, locale: 'en', offset: 0),
      ))!;
      expect(history.any((r) => r.body == draft), isTrue);
      expect(
        (await tester.runAsync(() => policy('zh-Hans')))!.currentRevisionId,
        chinese.currentRevisionId,
      );
      final strings = lookupAppLocalizations(const Locale('en'));
      Finder policyField(String label) => find.byWidgetPredicate(
        (w) =>
            w is DropdownButtonFormField<String> &&
            w.decoration.labelText == label,
      );
      Future<void> choose(String label, String option) async {
        await tester.ensureVisible(policyField(label));
        await tester.tap(policyField(label));
        await tester.pumpAndSettle();
        await tester.tap(find.text(option).last);
        await settle();
      }

      await choose(strings.aiResponseLanguage, strings.english);
      final responsePreference = (await tester.runAsync(
        () => api.getPromptLanguagePreferences(),
      ))!;
      expect(responsePreference.responseMode, 'fixed');
      expect(responsePreference.responseLocale, 'en');
      expect(responsePreference.instructionLocale, isNull);
      await choose(strings.aiInstructionLanguage, strings.chinese);
      final instructionPreference = (await tester.runAsync(
        () => api.getPromptLanguagePreferences(),
      ))!;
      expect(instructionPreference.instructionLocale, 'zh-Hans');
      expect(instructionPreference.responseLocale, 'en');
      expect(
        instructionPreference.generation,
        responsePreference.generation + 1,
      );
      expect(
        (await tester.runAsync(() => api.getKnowledgeLanguage()))!.outputLocale,
        'zh-Hans',
      );
      await choose(strings.knowledgeLanguage, strings.english);
      expect(
        (await tester.runAsync(() => api.getKnowledgeLanguage()))!.outputLocale,
        'en',
      );
      expect(
        (await tester.runAsync(() => api.getPromptLanguagePreferences()))!
            .instructionLocale,
        'zh-Hans',
      );
      expect(find.text(strings.knowledgeLanguageHelp), findsOneWidget);
      expect(
        (await tester.runAsync(() => api.listPromptBundles(offset: 0)))!,
        isEmpty,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
