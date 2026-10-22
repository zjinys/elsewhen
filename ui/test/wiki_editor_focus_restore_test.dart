import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 窗口焦点恢复：桌面端切走再切回，编辑器要自动拿回光标与键盘，
/// 不需要重新点击（Linux 窗口失活会丢 primary focus，vendor 失焦清空选区）。
void main() {
  Widget harness({required List<bool> editable}) {
    return MaterialApp(
      theme: AppTheme.darkTheme,
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: Row(
          children: [
            for (final flag in editable)
              Expanded(
                child: SizedBox(
                  height: 400,
                  child: WikiContentEditor(
                    slug: 'focus-test-${flag ? 1 : 0}',
                    contentMd: '焦点恢复测试正文',
                    editable: flag,
                    onWikiLinkTap: (_) {},
                    onSave: (_) async {},
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  /// 释放一帧后落地编辑器历史防抖 timer，避免 pending timer 报警。
  Future<void> flushEditorTimers(WidgetTester tester) async {
    await tester.pump(const Duration(milliseconds: 100));
  }

  testWidgets('切换窗口回来后恢复光标与键盘', (tester) async {
    await tester.pumpWidget(harness(editable: [true]));
    await tester.pump();

    final editor = tester.state<WikiContentEditorState>(
      find.byType(WikiContentEditor),
    );
    final selection = Selection.single(path: [0], startOffset: 3);
    editor.editorState.updateSelectionWithReason(
      selection,
      reason: SelectionUpdateReason.uiEvent,
    );
    await tester.pump();

    // 模拟窗口失活（切到别的程序）
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pump();

    // 模拟窗口重新激活
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();

    expect(
      editor.editorState.selection,
      selection,
      reason: '回前台后编辑器应自动恢复失活前的光标',
    );
    await flushEditorTimers(tester);
  });

  testWidgets('只读编辑器不恢复光标', (tester) async {
    await tester.pumpWidget(harness(editable: [false]));
    await tester.pump();

    final editor = tester.state<WikiContentEditorState>(
      find.byType(WikiContentEditor),
    );
    expect(editor.editorState.selection, isNull);
    expect(editor.debugHasTrackedSelection, isFalse, reason: '只读态不应跟踪选区');

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pump();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();

    expect(editor.editorState.selection, isNull);
    await flushEditorTimers(tester);
  });

  testWidgets('多个可编辑编辑器都有残存选区时不认领（避免抢焦点）', (tester) async {
    await tester.pumpWidget(harness(editable: [true, true]));
    await tester.pump();

    final editors = tester
        .stateList<WikiContentEditorState>(find.byType(WikiContentEditor))
        .toList();
    final selection = Selection.single(path: [0], startOffset: 0);
    for (final editor in editors) {
      // 设置选区后清空（模拟 vendor 失焦行为），_lastSelection 保留残值。
      editor.editorState.updateSelectionWithReason(
        selection,
        reason: SelectionUpdateReason.uiEvent,
      );
    }
    await tester.pump();
    for (final editor in editors) {
      editor.editorState.selection = null;
    }
    await tester.pump();

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pump();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();

    expect(
      editors.any((e) => e.debugIsFocusRecoveryOwner),
      isFalse,
      reason: '两个编辑器都候选时不能盲认领，防止抢错焦点',
    );
    await flushEditorTimers(tester);
  });
}
