import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:material_ui/material_ui.dart';

Future<void> onPerformAction(
  TextInputAction action,
  EditorState editorState,
) async {
  AppFlowyEditorLog.input.debug('onPerformAction: $action');
}
