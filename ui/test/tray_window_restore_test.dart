import 'dart:async';

import 'package:elsewhen_ui/utils/window_service.dart';
import 'package:elsewhen_ui/utils/window_service_desktop.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:window_manager/window_manager.dart';

void main() {
  final binding = TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('window_manager');
  final calls = <String>[];
  var minimized = false;
  var failShow = false;
  Completer<bool>? minimizedResult;

  setUp(() {
    calls.clear();
    minimized = false;
    failShow = false;
    minimizedResult = null;
    binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('tray_manager'),
      (_) async => null,
    );
    binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      calls.add(call.method);
      if (call.method == 'isMinimized') {
        return minimizedResult?.future ?? minimized;
      }
      if (call.method == 'show' && failShow) {
        throw PlatformException(code: 'show_failed');
      }
      return null;
    });
  });

  tearDown(() {
    binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, null);
    binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('tray_manager'),
      null,
    );
  });

  testWidgets('tray restore keeps geometry and resumes the main window', (
    tester,
  ) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(binding.hasScheduledFrame, isFalse);
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    await service.hideWindow();
    expect(service.currentMode, WindowMode.hidden);
    calls.clear();
    await service.switchToMainMode();
    expect(calls, ['isMinimized', 'show', 'focus']);
    expect(service.currentMode, WindowMode.main);
    expect(binding.hasScheduledFrame, isFalse);
    await tester.pump();
  });

  testWidgets('minimized window is restored before showing and focusing', (
    tester,
  ) async {
    minimized = true;
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    await service.showWindow();
    expect(calls, ['isMinimized', 'restore', 'show', 'focus']);
    await tester.pump();
  });

  testWidgets('concurrent tray requests share one restore sequence', (
    tester,
  ) async {
    minimizedResult = Completer<bool>();
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    final first = service.showWindow();
    final second = service.switchToMainMode();
    expect(identical(first, second), isTrue);
    minimizedResult!.complete(false);
    await Future.wait([first, second]);
    expect(calls, ['isMinimized', 'show', 'focus']);
    await tester.pump();
  });

  testWidgets('failed restore can be retried without hiding the window', (
    tester,
  ) async {
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    failShow = true;
    await expectLater(service.showWindow(), throwsA(isA<PlatformException>()));
    failShow = false;
    await service.showWindow();
    expect(calls, ['isMinimized', 'show', 'isMinimized', 'show', 'focus']);
    await tester.pump();
  });

  testWidgets('window events do not force redundant Dart frames', (
    tester,
  ) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    await service.initialize();
    for (final event in ['show', 'restore', 'focus']) {
      expect(binding.hasScheduledFrame, isFalse);
      for (final listener in windowManager.listeners) {
        listener.onWindowEvent(event);
      }
      expect(binding.hasScheduledFrame, isFalse);
      await tester.pump();
    }
    expect(calls, isEmpty);
  });

  testWidgets('restore leaves lifecycle frame scheduling to the engine', (
    tester,
  ) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    addTearDown(
      () => binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed),
    );
    expect(binding.framesEnabled, isFalse);
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    await service.showWindow();
    expect(binding.hasScheduledFrame, isFalse);
    expect(binding.lifecycleState, AppLifecycleState.paused);
    await tester.pump();
  });
}
