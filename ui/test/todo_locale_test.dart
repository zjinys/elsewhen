import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/models/todo.dart';
import 'package:elsewhen_ui/providers/todo_provider.dart';
import 'package:elsewhen_ui/widgets/todo_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final locale in ['zh', 'en']) {
    testWidgets(
      '$locale to-do labels, menu, editor and narrow layout preserve original content',
      (tester) async {
        tester.view.physicalSize = const Size(380, 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final todo = Todo(
          id: 'task',
          title: '原始待办：read the project notes',
          status: TodoStatus.open,
          priority: 'high',
          dueAt: '2027-01-02T12:30:00Z',
          reminder: true,
          relatedWikiSlug: 'source/original',
          note: '用户原始补充😀',
          createdAt: DateTime(2026),
          updatedAt: DateTime(2026),
        );
        final strings = lookupAppLocalizations(Locale(locale));
        await tester.pumpWidget(
          ProviderScope(
            overrides: [
              todosProvider.overrideWith((ref) async => [todo]),
            ],
            child: MaterialApp(
              locale: Locale(locale),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              supportedLocales: AppLocalizations.supportedLocales,
              home: const Scaffold(body: TodoListView()),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(find.text(todo.title), findsOneWidget);
        expect(find.text(todo.note!), findsOneWidget);
        expect(find.text(strings.todoOpenGroup(1)), findsOneWidget);
        expect(find.text(strings.knowledgePage), findsOneWidget);
        expect(tester.takeException(), isNull);
        await tester.tap(find.byType(PopupMenuButton<String>));
        await tester.pumpAndSettle();
        await tester.tap(find.text(strings.edit));
        await tester.pumpAndSettle();
        expect(find.text(strings.todoEdit), findsOneWidget);
        expect(find.text(strings.todoPriority), findsOneWidget);
        expect(find.text(strings.priorityHigh), findsOneWidget);
        expect(find.text(strings.todoReminderOption), findsOneWidget);
        expect(find.widgetWithText(TextField, todo.title), findsOneWidget);
        expect(tester.takeException(), isNull);
        await tester.tap(find.text(strings.cancel));
        await tester.pumpAndSettle();
        expect(find.text(todo.title), findsOneWidget);
      },
    );
  }

  testWidgets(
    'English empty state and unfinished input survive a display language switch',
    (tester) async {
      late StateSetter update;
      var locale = const Locale('en');
      await tester.pumpWidget(
        ProviderScope(
          overrides: [todosProvider.overrideWith((ref) async => [])],
          child: StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              return MaterialApp(
                locale: locale,
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                supportedLocales: AppLocalizations.supportedLocales,
                home: const Scaffold(body: TodoListView()),
              );
            },
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.text(lookupAppLocalizations(locale).todoEmptyHelp),
        findsOneWidget,
      );
      await tester.enterText(find.byType(TextField).first, 'Unsent 原文');
      update(() => locale = const Locale('zh'));
      await tester.pumpAndSettle();
      expect(find.text('Unsent 原文'), findsOneWidget);
      expect(
        find.text(lookupAppLocalizations(locale).todoEmptyHelp),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
