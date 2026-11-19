import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:material_ui/material_ui.dart';

mixin BlockComponentAlignMixin {
  Node get node;

  Alignment? get alignment {
    final alignString = node.attributes[blockComponentAlign] as String?;
    switch (alignString) {
      case 'center':
        return Alignment.center;

      case 'right':
        return Alignment.centerRight;

      case 'left':
        return Alignment.centerLeft;

      default:
        return null;
    }
  }
}
