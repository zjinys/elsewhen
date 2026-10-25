import 'package:flutter_test/flutter_test.dart';

/// 配对进度条当前步是无限转动的图标（SyncPairingSteps._StepSpinner），
/// 只要它在屏幕上，`pumpAndSettle` 永远不会 idle。渲染 owner/joiner 配对卡
/// 的测试改用这里的定长推进：等动画跑几帧，同时处理控制器返回的 Future。
Future<void> pumpPairingSteps(
  WidgetTester tester, {
  Duration step = const Duration(milliseconds: 50),
  int frames = 8,
}) async {
  for (var i = 0; i < frames; i++) {
    await tester.pump(step);
  }
}
