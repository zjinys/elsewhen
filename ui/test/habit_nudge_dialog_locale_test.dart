import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/widgets/habit_nudge_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _HabitRepo extends RustBridgeRepository {
  api.HabitNudgeInputDto? saved;
  int reads = 0;

  @override
  Future<api.HabitNudgeSuggestionDto> getHabitNudgeSuggestion(
    String slug,
  ) async {
    reads++;
    return api.HabitNudgeSuggestionDto(
      pageSlug: slug,
      pageTitle: '午饭后散步',
      question: '今天午饭后散步了吗？',
      weekdays: 31,
      localTime: '12:30',
      doneLabel: '走了',
      doneText: '今天午饭后散步了',
      missedLabel: '没走',
      missedText: '今天还没走',
      basisExcerpt: '我工作日吃完午饭会散步。',
      confidence: 0.8,
    );
  }

  @override
  Future<api.HabitNudgeDto> saveHabitNudge(api.HabitNudgeInputDto input) async {
    saved = input;
    return api.HabitNudgeDto(
      id: 'nudge-1',
      pageSlug: input.pageSlug,
      question: input.question,
      doneLabel: input.doneLabel,
      doneText: input.doneText,
      missedLabel: input.missedLabel,
      missedText: input.missedText,
      weekdays: input.weekdays,
      localTime: input.localTime,
      enabled: input.enabled,
      createdAt: '2026-11-06T00:00:00Z',
      updatedAt: '2026-11-06T00:00:00Z',
    );
  }
}

void main() {
  testWidgets(
    'language change preserves habit draft and validation; save retains exact user fields',
    (tester) async {
      tester.view.physicalSize = const Size(420, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final locale = ValueNotifier(const Locale('en'));
      addTearDown(locale.dispose);
      final repo = _HabitRepo();
      final now = DateTime(2026, 11, 6);
      final page = WikiPageMeta(
        id: 'page-1',
        slug: 'habit/walk',
        kind: 'habit',
        title: '午饭后散步',
        summary: '',
        tags: const [],
        evidenceCount: 1,
        firstSeenAt: now,
        lastSeenAt: now,
        createdAt: now,
        updatedAt: now,
        status: 'active',
        area: 'insight',
      );
      await tester.pumpWidget(
        ProviderScope(
          overrides: [storageRepositoryProvider.overrideWithValue(repo)],
          child: ValueListenableBuilder<Locale>(
            valueListenable: locale,
            builder: (context, value, _) => MaterialApp(
              locale: value,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: Builder(
                  builder: (context) => TextButton(
                    onPressed: () => showHabitNudgeDialog(context, page: page),
                    child: const Text('open'),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.text('我工作日吃完午饭会散步。'), findsOneWidget);
      expect(find.text('Mon'), findsOneWidget);
      expect(find.text('走了'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.enterText(find.byType(TextField).at(0), '');
      await tester.ensureVisible(find.text('Save'));
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(find.text('Enter a check-in question'), findsOneWidget);
      expect(repo.saved, isNull);

      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('问法不能为空'), findsOneWidget);
      expect(find.text('Enter a check-in question'), findsNothing);
      final question = find.byType(TextField).at(0);
      expect(tester.widget<TextField>(question).controller!.text, '');
      expect(repo.reads, 1);
      await tester.ensureVisible(question);
      await tester.enterText(question, '今天散步了几分钟？');
      await tester.ensureVisible(find.text('保存'));
      await tester.tap(find.text('保存'));
      await tester.pumpAndSettle();
      expect(find.byType(HabitNudgeDialog), findsNothing);
      expect(repo.saved!.pageSlug, 'habit/walk');
      expect(repo.saved!.question, '今天散步了几分钟？');
      expect(repo.saved!.weekdays, 31);
      expect(repo.saved!.localTime, '12:30');
      expect(repo.saved!.doneLabel, '走了');
      expect(repo.saved!.doneText, '今天午饭后散步了');
      expect(repo.saved!.missedLabel, '没走');
      expect(repo.saved!.missedText, '今天还没走');
      expect(tester.takeException(), isNull);
    },
  );
}
