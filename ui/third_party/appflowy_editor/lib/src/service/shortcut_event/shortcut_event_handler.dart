import 'package:appflowy_editor/src/editor_state.dart';
import 'package:material_ui/material_ui.dart';

typedef ShortcutEventHandler = KeyEventResult Function(
  EditorState editorState,
  KeyEvent? event,
);
