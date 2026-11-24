import 'package:flutter/widgets.dart';

void main() {
  runApp(
    const Directionality(
      textDirection: TextDirection.ltr,
      child: ColoredBox(
        color: Color(0xFF22AA66),
        child: Center(
          child: Text(
            'Tray restore probe',
            style: TextStyle(fontSize: 32, color: Color(0xFFFFFFFF)),
          ),
        ),
      ),
    ),
  );
}
