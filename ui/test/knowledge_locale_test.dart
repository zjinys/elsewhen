import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_workflows.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'knowledge_workflows_test.dart' show WorkflowFake;

class _LibraryFake extends WorkflowFake {
  final requests =
      <
        ({
          String query,
          String? area,
          String? kind,
          String? tag,
          String? state,
          int offset,
        })
      >[];
  bool fail = false;
  int readingWrites = 0;

  @override
  Future<api.LibraryPage> browse(
    String query,
    String? area,
    String? kind,
    String? tag,
    String? state,
    int offset,
  ) async {
    requests.add((
      query: query,
      area: area,
      kind: kind,
      tag: tag,
      state: state,
      offset: offset,
    ));
    if (fail) throw Exception('数据库损坏 SELECT private_path');
    return super.browse(query, area, kind, tag, state, offset);
  }

  @override
  Future<void> setReading(String slug, String state) async {
    readingWrites++;
    await super.setReading(slug, state);
  }
}

Future<ValueNotifier<Locale>> _pump(
  WidgetTester tester,
  _LibraryFake repo,
  Widget child, {
  Locale initialLocale = const Locale('zh'),
  double width = 380,
}) async {
  await tester.binding.setSurfaceSize(Size(width, 1100));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  final locale = ValueNotifier(initialLocale);
  addTearDown(locale.dispose);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [knowledgeRepositoryProvider.overrideWithValue(repo)],
      child: ValueListenableBuilder<Locale>(
        valueListenable: locale,
        builder: (_, value, _) => MaterialApp(
          locale: value,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(body: child),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return locale;
}

Future<void> _select(WidgetTester tester, int index, String label) async {
  await tester.tap(find.byType(DropdownButton<String>).at(index));
  await tester.pumpAndSettle();
  await tester.tap(find.text(label).last);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'library locale switch preserves input, filter wire values and pagination',
    (tester) async {
      final repo = _LibraryFake();
      final locale = await _pump(tester, repo, const KnowledgeLibrary());
      await tester.enterText(find.byType(TextField).first, '用户原文');
      await tester.enterText(find.byType(TextField).last, '我的标签');
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pumpAndSettle();
      final requestsBeforeSwitch = repo.requests.length;
      locale.value = const Locale('en');
      await tester.pumpAndSettle();
      expect(repo.requests, hasLength(requestsBeforeSwitch));
      expect(find.text('用户原文'), findsOneWidget);
      expect(find.text('我的标签'), findsOneWidget);
      expect(find.text('页面 0'), findsOneWidget);
      expect(find.text('摘要'), findsOneWidget);
      await _select(tester, 0, 'Contacts and projects');
      await _select(tester, 1, 'Method');
      await _select(tester, 2, 'Worth using');
      await tester.tap(find.text('Load more'));
      await tester.pumpAndSettle();
      final request = repo.requests.last;
      expect(request.query, '用户原文');
      expect(request.tag, '我的标签');
      expect(request.area, 'network');
      expect(request.kind, 'method');
      expect(request.state, 'valuable');
      expect(request.offset, 1);
      expect(find.text('页面 1'), findsOneWidget);
    },
  );

  testWidgets(
    'narrow English reading control writes only after an explicit selection',
    (tester) async {
      final repo = _LibraryFake();
      final locale = await _pump(
        tester,
        repo,
        const KnowledgeReadingControl(slug: '原文页面'),
        width: 320,
      );
      locale.value = const Locale('en');
      await tester.pumpAndSettle();
      expect(repo.readingWrites, 0);
      await _select(tester, 0, 'Worth using');
      expect(repo.readingValue, 'valuable');
      expect(repo.readingWrites, 1);
      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('值得使用'), findsOneWidget);
      expect(repo.readingWrites, 1);
    },
  );

  testWidgets(
    'inline load failure changes language and keeps diagnostics behind details',
    (tester) async {
      final repo = _LibraryFake()..fail = true;
      final locale = await _pump(tester, repo, const KnowledgeLibrary());
      locale.value = const Locale('en');
      await tester.pumpAndSettle();
      final en = lookupAppLocalizations(const Locale('en'));
      expect(find.text(en.operationFailed(en.load)), findsOneWidget);
      expect(find.textContaining('SELECT'), findsNothing);
      await tester.tap(find.text(en.errorDetails));
      await tester.pumpAndSettle();
      expect(find.textContaining('SELECT private_path'), findsOneWidget);
    },
  );
}
