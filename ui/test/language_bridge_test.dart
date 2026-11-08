import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:elsewhen_ui/utils/app_error.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';

import 'support/isolated_bridge.dart';

void main() {
  final binding = TestWidgetsFlutterBinding.ensureInitialized();

  test('language survives reload, supports system changes and keeps AI preferences independent', () async {
    final repo = await createIsolatedBridge();
    ProviderContainer container() => ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    );
    final first = container();
    addTearDown(first.dispose);
    final settings = first.read(settingsProvider.notifier);
    await settings.loadLanguageFromBridge();
    expect(first.read(settingsProvider).language, 'zh-Hans');
    await settings.updateLanguage('en');
    expect(first.read(settingsProvider).language, 'en');
    expect((await api.getUiLanguagePreference()).locale, 'en');

    final reopened = container();
    addTearDown(reopened.dispose);
    await reopened.read(settingsProvider.notifier).loadLanguageFromBridge();
    expect(reopened.read(settingsProvider).language, 'en');

    await expectLater(settings.updateLanguage('fr'), throwsA(anything));
    expect(first.read(settingsProvider).language, 'en');
    expect((await api.getUiLanguagePreference()).locale, 'en');

    // Rapid changes are persisted in order; failed writes don't poison the queue.
    await Future.wait([
      settings.updateLanguage('zh-Hans'),
      settings.updateLanguage(null),
    ]);
    expect(first.read(settingsProvider).language, isNull);
    expect((await api.getUiLanguagePreference()).locale, isNull);
    binding.platformDispatcher.localesTestValue = [const Locale('en', 'GB')];
    addTearDown(binding.platformDispatcher.clearLocalesTestValue);
    await settings.synchronizeLanguage();
    expect(Intl.defaultLocale, 'en');
    binding.platformDispatcher.localesTestValue = [const Locale('zh', 'CN')];
    await settings.synchronizeLanguage();
    expect(Intl.defaultLocale, 'zh_CN');
    expect(first.read(settingsProvider).language, isNull);

    final initial = await api.getPromptLanguagePreferences();
    expect(initial.responseMode, 'auto');
    expect(initial.instructionLocale, isNull);
    final saved = await api.updatePromptLanguagePreferences(
      instructionLocale: 'en_GB',
      responseMode: 'fixed',
      responseLocale: 'zh_CN',
      expectedGeneration: initial.generation,
    );
    expect(saved.instructionLocale, 'en-GB');
    expect(saved.responseLocale, 'zh-Hans');
    await expectLater(
      api.updatePromptLanguagePreferences(
        responseMode: 'auto',
        expectedGeneration: initial.generation,
      ),
      throwsA(anything),
    );
    expect(await api.getPromptLanguagePreferences(), saved);
    await api.updatePromptLanguagePreferences(
      responseMode: 'auto',
      expectedGeneration: saved.generation,
    );
    expect(
      (await api.getPromptLanguagePreferences()).instructionLocale,
      isNull,
    );
    final knowledge = await api.getKnowledgeLanguage();
    expect(knowledge.outputLocale, 'zh-Hans');
    expect(knowledge.policyVersion, 1);
    expect(knowledge.canEdit, isTrue);
    expect(knowledge.supported, isTrue);
    final englishKnowledge = await api.updateKnowledgeLanguage(
      outputLocale: 'en',
      expectedGeneration: knowledge.generation,
    );
    expect(englishKnowledge.outputLocale, 'en');
    expect((await api.getKnowledgeLanguage()), englishKnowledge);
    await expectLater(
      api.updateKnowledgeLanguage(
        outputLocale: 'zh-Hans',
        expectedGeneration: knowledge.generation,
      ),
      throwsA(
        predicate<Object>(
          (error) => AppError.decode(error)?.code == 'settings_changed',
        ),
      ),
    );
    expect((await api.getKnowledgeLanguage()).outputLocale, 'en');
    expect((await api.getPromptLanguagePreferences()).responseMode, 'auto');
    await expectLater(
      api.createTodo(title: '', reminder: false),
      throwsA(
        predicate<Object>(
          (error) => AppError.decode(error)?.code == 'field_required',
        ),
      ),
    );
    final todo = await api.createTodo(title: '原始待办', reminder: false);
    await expectLater(
      api.setTodoReminder(id: todo.id, reminder: true),
      throwsA(
        predicate<Object>(
          (error) => AppError.decode(error)?.code == 'reminder_time_required',
        ),
      ),
    );
    expect((await api.getUiLanguagePreference()).locale, isNull);

    final topic = await api.saveTopicPage(
      text: '不可被失败保存覆盖的原文',
      title: '编辑错误契约',
      tags: const [],
    );
    await expectLater(
      api.formatImportContent(content: ''),
      throwsA(
        predicate<Object>(
          (error) => AppError.decode(error)?.parameters['field'] == 'content',
        ),
      ),
    );
    await expectLater(
      api.saveWikiPageContent(
        slug: topic.slug,
        contentMd: '过期编辑',
        reason: 'test',
        expectedUpdatedAt: '2000-01-01T00:00:00Z',
      ),
      throwsA(
        predicate<Object>(
          (error) => AppError.decode(error)?.code == 'wiki_page_changed',
        ),
      ),
    );
    await expectLater(
      api.saveWikiPageContent(slug: topic.slug, contentMd: '', reason: 'test'),
      throwsA(
        predicate<Object>((error) {
          final parsed = AppError.decode(error);
          return parsed?.code == 'field_required' &&
              parsed?.parameters['field'] == 'content';
        }),
      ),
    );
    expect(
      (await api.getWikiPage(slug: topic.slug))?.contentMd,
      topic.contentMd,
    );
  });
}
