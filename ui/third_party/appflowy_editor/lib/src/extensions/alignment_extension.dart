import 'package:material_ui/material_ui.dart';

extension AppFlowyTextAlign on Alignment {
  TextAlign get toTextAlign {
    switch (this) {
      case Alignment.center:
        return TextAlign.center;

      case Alignment.centerRight:
        return TextAlign.right;
      case Alignment.centerLeft:
      default:
        return TextAlign.left;
    }
  }
}
