import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 编辑态格式工具栏：只读不显示，编辑态可对选区应用格式。
void main() {
  Widget harness({required bool editable}) {
    return MaterialApp(
      theme: AppTheme.darkTheme,
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: SizedBox(
          height: 400,
          child: WikiContentEditor(
            slug: 'toolbar-test',
            contentMd: '工具栏测试正文',
            editable: editable,
            onWikiLinkTap: (_) {},
            onSave: (_) async {},
          ),
        ),
      ),
    );
  }

  testWidgets('只读态不显示格式工具栏', (tester) async {
    await tester.pumpWidget(harness(editable: false));
    await tester.pump();

    expect(find.byTooltip('加粗'), findsNothing);
  });

  testWidgets('编辑态显示格式工具栏，加粗作用于选区', (tester) async {
    await tester.pumpWidget(harness(editable: true));
    await tester.pump();

    expect(find.byTooltip('加粗'), findsOneWidget);
    expect(find.byTooltip('无序列表'), findsOneWidget);

    final editorState = tester
        .state<WikiContentEditorState>(find.byType(WikiContentEditor))
        .editorState;
    editorState.updateSelectionWithReason(
      Selection.single(path: [0], startOffset: 0, endOffset: 3),
      reason: SelectionUpdateReason.uiEvent,
    );
    await tester.pump();

    await tester.tap(find.byTooltip('加粗'));
    await tester.pump();

    final delta = editorState.document.root.children.first.delta!;
    expect(delta.toJson().toString(), contains('bold'), reason: '工具栏加粗应写入当前选区');

    // 编辑器历史记录有 50ms 防抖 timer，等它落地避免测试结束时报 pending timer。
    await tester.pump(const Duration(milliseconds: 100));
  });
}
