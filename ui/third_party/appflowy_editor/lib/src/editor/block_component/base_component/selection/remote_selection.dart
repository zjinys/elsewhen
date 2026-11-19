import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:material_ui/material_ui.dart';

class RemoteSelection {
  const RemoteSelection({
    required this.id,
    required this.selection,
    required this.selectionColor,
    required this.cursorColor,
    this.builder,
  });

  final String id;
  final Selection selection;
  final Color selectionColor;
  final Color cursorColor;
  final Widget Function(
    BuildContext context,
    RemoteSelection selection,
    Rect rect,
  )? builder;
}
