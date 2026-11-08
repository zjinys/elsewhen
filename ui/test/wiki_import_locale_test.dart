import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _ImportRepo extends RustBridgeRepository {
  int fetches = 0, saves = 0;
  String? savedText, savedTitle;
  List<String>? savedTags;

  @override
  Future<String> guessImportKind(String url) async {
    fetches++;
    throw Exception('诊断 SQL /private/path');
  }

  @override
  Future<WikiPage> saveTopicPage({
    required String text,
    String? title,
    List<String> tags = const [],
  }) async {
    saves++;
    savedText = text;
    savedTitle = title;
    savedTags = tags;
    throw Exception('诊断 SQL /private/path');
  }
}

Future<ValueNotifier<Locale>> _pump(
  WidgetTester tester,
  _ImportRepo repo,
) async {
  await tester.binding.setSurfaceSize(const Size(380, 1000));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  final locale = ValueNotifier(const Locale('en'));
  addTearDown(locale.dispose);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
      child: ValueListenableBuilder<Locale>(
        valueListenable: locale,
        builder: (_, value, _) => MaterialApp(
          locale: value,
          supportedLocales: AppLocalizations.supportedLocales,
          localizationsDelegates: const [
            ...AppLocalizations.localizationsDelegates,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: Scaffold(
            body: Builder(
              builder: (context) => Column(
                children: [
                  TextButton(
                    onPressed: () =>
                        openWikiImportDialog(context, urlMode: true),
                    child: const Text('url'),
                  ),
                  TextButton(
                    onPressed: () => openCreateTopicDialog(context),
                    child: const Text('text'),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return locale;
}

void main() {
  testWidgets(
    'URL import validation and diagnostics follow locale without clearing input',
    (tester) async {
      final repo = _ImportRepo();
      final locale = await _pump(tester, repo);
      await tester.tap(find.text('url'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Fetch'));
      await tester.pumpAndSettle();
      expect(find.text('Paste a link first.'), findsOneWidget);
      expect(repo.fetches, 0);
      await tester.enterText(
        find.byType(TextField),
        'https://example.com/用户原文',
      );
      await tester.tap(find.text('Fetch'));
      await tester.pumpAndSettle();
      expect(repo.fetches, 1);
      expect(find.textContaining('SQL'), findsNothing);
      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('网址导入'), findsOneWidget);
      expect(find.text('https://example.com/用户原文'), findsOneWidget);
      expect(find.text('抓取失败，请重试一次'), findsOneWidget);
      await tester.tap(find.text('查看详情'));
      await tester.pumpAndSettle();
      expect(find.textContaining('SQL /private/path'), findsOneWidget);
    },
  );

  testWidgets(
    'create-topic locale switch keeps title, tags and editor text until explicit save',
    (tester) async {
      final repo = _ImportRepo();
      final locale = await _pump(tester, repo);
      await tester.tap(find.text('text'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).first, '用户自己的标题');
      await tester.enterText(find.byType(TextField).last, '#工作, Rust');
      final state = tester.state<WikiContentEditorState>(
        find.byType(WikiContentEditor),
      );
      final paragraph = state.editorState.document.root.children.first;
      final transaction = state.editorState.transaction;
      transaction.insertText(paragraph, 0, '用户自己的正文，保留 original text。');
      await state.editorState.apply(transaction);
      await tester.pump();
      locale.value = const Locale('zh');
      await tester.pumpAndSettle();
      expect(find.text('创建主题'), findsOneWidget);
      expect(find.text('用户自己的标题'), findsOneWidget);
      expect(find.text('#工作, Rust'), findsOneWidget);
      expect(repo.saves, 0);
      await tester.tap(find.text('保存到知识库'));
      await tester.pumpAndSettle();
      expect(repo.saves, 1);
      expect(repo.savedTitle, '用户自己的标题');
      expect(repo.savedTags, ['工作', 'Rust']);
      expect(repo.savedText, contains('用户自己的正文，保留 original text。'));
      expect(find.byType(WikiContentEditor), findsOneWidget);
      expect(find.textContaining('SQL'), findsNothing);
    },
  );
}
