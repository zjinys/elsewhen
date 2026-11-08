import 'package:elsewhen_ui/utils/window_service_desktop.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('tray language updates retain action keys and never invoke window lifecycle', () async {
    final calls = <MethodCall>[];
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    for (final channel in ['tray_manager', 'window_manager']) {
      messenger.setMockMethodCallHandler(MethodChannel(channel), (call) async {
        calls.add(call);
        return null;
      });
      addTearDown(
        () => messenger.setMockMethodCallHandler(MethodChannel(channel), null),
      );
    }
    final service = DesktopWindowService();
    addTearDown(service.dispose);
    await service.updateLocale(const Locale('en'));
    expect(calls, isEmpty);
    await service.initialize();
    expect(service.trayAvailable, isTrue);
    Map<dynamic, dynamic> latestMenu() =>
        calls.lastWhere((call) => call.method == 'setContextMenu').arguments
            as Map;
    expect(latestMenu().toString(), contains('Show main window'));
    expect(latestMenu().toString(), contains('Quit Elsewhen'));
    calls.clear();
    await Future.wait([
      service.updateLocale(const Locale('zh')),
      service.updateLocale(const Locale('en')),
    ]);
    expect(calls.map((call) => call.method), [
      'setContextMenu',
      'setContextMenu',
    ]);
    final first = calls.first.arguments.toString();
    expect(first, contains('显示主窗口'));
    expect(first, contains('退出 Elsewhen'));
    expect(latestMenu().toString(), contains('Show main window'));
    expect(latestMenu().toString(), contains('show'));
    expect(latestMenu().toString(), contains('quit'));
  });
}
