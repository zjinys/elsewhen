import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/utils/desktop_prefs.dart';

/// 桌面壳层偏好（desktop.json）读写健壮性：默认值、往返、损坏/未知值回退。
void main() {
  group('DesktopPrefsStore', () {
    late Directory dir;
    setUp(() {
      dir = Directory.systemTemp.createTempSync('elsewhen-prefs-');
    });
    tearDown(() {
      if (dir.existsSync()) dir.deleteSync(recursive: true);
    });

    test('无文件 → 默认最小化到托盘', () {
      final store = DesktopPrefsStore(directoryOverride: dir.path);
      expect(store.loadCloseAction(), CloseAction.minimizeToTray);
    });

    test('保存后可读回（两值往返）', () {
      final store = DesktopPrefsStore(directoryOverride: dir.path);

      store.saveCloseAction(CloseAction.quit);
      expect(store.loadCloseAction(), CloseAction.quit);
      expect(
        File('${dir.path}/desktop.json').existsSync(),
        isTrue,
        reason: '应写到 desktop.json',
      );

      store.saveCloseAction(CloseAction.minimizeToTray);
      expect(store.loadCloseAction(), CloseAction.minimizeToTray);
    });

    test('文件损坏 → 回退默认', () {
      final store = DesktopPrefsStore(directoryOverride: dir.path);
      File('${dir.path}/desktop.json').writeAsStringSync('{ not json');
      expect(store.loadCloseAction(), CloseAction.minimizeToTray);
    });

    test('未知取值 → 回退默认', () {
      final store = DesktopPrefsStore(directoryOverride: dir.path);
      File('${dir.path}/desktop.json')
          .writeAsStringSync('{"closeAction":"banana"}');
      expect(store.loadCloseAction(), CloseAction.minimizeToTray);
    });
  });
}
