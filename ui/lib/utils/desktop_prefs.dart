import 'dart:convert';
import 'dart:io';

import 'window_geometry_store.dart' show WindowGeometryStore;

/// 「关闭窗口时」的行为（桌面壳层偏好，与业务设置正交）。
enum CloseAction {
  /// 隐藏窗口、驻留系统托盘（默认）。
  minimizeToTray('tray', '最小化到托盘'),

  /// 直接退出应用。
  quit('quit', '退出应用');

  const CloseAction(this.stored, this.displayName);

  /// 存盘用的小写机器名
  final String stored;

  /// 设置页展示名
  final String displayName;

  static CloseAction fromStored(String? value) =>
      value == quit.stored ? quit : minimizeToTray;
}

/// 桌面行为偏好的本地存储：写到应用数据目录的 `desktop.json`，
/// 与 `window.json` / elsewhen.db 同目录但不进数据库（与窗口几何同理，
/// 这是纯桌面壳层偏好，多端同步无意义）。
class DesktopPrefsStore {
  const DesktopPrefsStore({this.directoryOverride});

  /// 测试注入用：显式目录覆盖平台推导；null 时走真实平台规则。
  final String? directoryOverride;

  static const String fileName = 'desktop.json';

  String? get _directory =>
      directoryOverride ?? WindowGeometryStore.resolveDataDir();

  File _file() {
    final dir = _directory;
    return dir == null
        ? File(fileName)
        : File('$dir${Platform.pathSeparator}$fileName');
  }

  /// 读取「关闭时」偏好；无文件/损坏/非法一律回退默认值（最小化到托盘）。
  CloseAction loadCloseAction() {
    try {
      final f = _file();
      if (!f.existsSync()) return CloseAction.minimizeToTray;
      final decoded = jsonDecode(f.readAsStringSync());
      if (decoded is! Map) return CloseAction.minimizeToTray;
      return CloseAction.fromStored(decoded['closeAction'] as String?);
    } catch (_) {
      return CloseAction.minimizeToTray; // 损坏/不可读：当默认处理
    }
  }

  void saveCloseAction(CloseAction action) {
    try {
      final f = _file();
      f.parent.createSync(recursive: true);
      f.writeAsStringSync(
        const JsonEncoder.withIndent('  ').convert({
          'closeAction': action.stored,
        }),
      );
    } catch (_) {
      // 写失败静默：偏好丢了只是下次回到默认，不值得打断用户。
    }
  }
}
