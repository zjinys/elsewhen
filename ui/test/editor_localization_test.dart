import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/utils/editor_localization.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('编辑器静态文案加载为中文，占位符不再显示英文', () async {
    await configureEditorLocalization();

    expect(AppFlowyEditorL10n.current.slashPlaceHolder, contains('插入'));
    expect(
      AppFlowyEditorL10n.current.slashPlaceHolder,
      isNot(contains('Enter')),
    );
  });

  test('切到 zh_CN 后 DateFormat 已有日期符号表，不会抛 LocaleDataException', () async {
    await configureEditorLocalization();

    expect(
      () => DateFormat('yyyy-MM-dd HH:mm').format(DateTime(2026, 10, 21, 13)),
      returnsNormally,
    );
  });

  testWidgets('MaterialApp 的默认 en locale 不会把编辑器文案改回英文', (tester) async {
    await configureEditorLocalization();

    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: const [
          DefaultMaterialLocalizations.delegate,
          DefaultWidgetsLocalizations.delegate,
          ChineseAppFlowyEditorLocalizationsDelegate(),
        ],
        home: const SizedBox.shrink(),
      ),
    );
    await tester.pumpAndSettle();

    expect(AppFlowyEditorL10n.current.slashPlaceHolder, contains('插入'));
  });

  testWidgets('空正文编辑器实际渲染中文段落占位符', (tester) async {
    await configureEditorLocalization();

    await tester.pumpWidget(
      MaterialApp(
        theme: AppTheme.darkTheme,
        localizationsDelegates: const [
          DefaultMaterialLocalizations.delegate,
          DefaultWidgetsLocalizations.delegate,
          ChineseAppFlowyEditorLocalizationsDelegate(),
        ],
        home: Scaffold(
          body: SizedBox(
            height: 400,
            child: WikiContentEditor(
              slug: 'editor-localization-test',
              contentMd: '',
              editable: true,
              onWikiLinkTap: (_) {},
              onSave: (_) async {},
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    final editorState = tester
        .state<WikiContentEditorState>(find.byType(WikiContentEditor))
        .editorState;
    editorState.updateSelectionWithReason(
      Selection.single(path: [0], startOffset: 0),
      reason: SelectionUpdateReason.uiEvent,
    );
    await tester.pump();

    final placeholder = find.byWidgetPredicate(
      (widget) =>
          widget is RichText && widget.text.toPlainText().contains('插入'),
    );
    expect(placeholder, findsOneWidget);

    // 编辑器历史记录有 50ms 防抖 timer，等它落地避免测试结束时报 pending timer。
    await tester.pump(const Duration(milliseconds: 100));
  });
}
