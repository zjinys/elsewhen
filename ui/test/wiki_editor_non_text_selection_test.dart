import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';
import 'package:flutter_test/flutter_test.dart';

/// 回归：选区首节点是不带 delta 的块（分割线 / 图片 / 表格）时，
/// vendor 的 deleteSelection 不能直接 deleteText，否则断言崩溃。
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('选区跨越两个无 delta 的块时，退格删除不会触发 vendor 断言', () async {
    final document = wikiMarkdownToDocument('a\n\n---\n\n---\n\nb');
    final state = EditorState(document: document);

    final first = document.root.children[1];
    final last = document.root.children[2];
    expect(first.delta, isNull);
    expect(last.delta, isNull);

    final selection = Selection(
      start: Position(path: first.path, offset: 0),
      end: Position(path: last.path, offset: 0),
    );
    state.updateSelectionWithReason(
      selection,
      reason: SelectionUpdateReason.uiEvent,
    );

    await state.deleteSelection(selection);

    expect(
      document.root.children.where((node) => node.type == 'divider'),
      isEmpty,
      reason: '选中的分割线应被删除，而不是对无 delta 节点调用 deleteText',
    );
  });
}
