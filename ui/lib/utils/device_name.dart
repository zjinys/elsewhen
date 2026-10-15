import 'dart:io' show Platform;

import 'package:device_info_plus/device_info_plus.dart';

/// 桌面端格式：hostname@系统名（如「pp@Ubuntu 24.04」「DESKTOP-8X@Windows」）。
/// 名字会出现在审批列表和成员列表里，保持短而可辨认。
String _desktopName(String host, String system) {
  final h = host.trim().isEmpty ? null : host.trim();
  final s = system.trim().isEmpty ? null : system.trim();
  if (h == null && s == null) return '';
  final combined = s == null ? h! : (h == null ? s : '$h@$s');
  return combined.length > 48 ? combined.substring(0, 48) : combined;
}

/// 配对表单里「设备名称」输入框的缺省值：尽量用真机器名（用户不必再
/// 手改「这台设备」，多设备同时配对时辨识度立即不同）。取不到时回退
/// 通用文案；失败绝不阻塞表单。
Future<String?> defaultDeviceName() async {
  try {
    final plugin = DeviceInfoPlugin();
    if (Platform.isAndroid) {
      final info = await plugin.androidInfo;
      // model 如「Pixel 8」；manufacturer 前缀在杂牌机上更易辨识。
      final model = info.model.trim();
      if (model.isEmpty) return null;
      return model;
    }
    if (Platform.isLinux) {
      final info = await plugin.linuxInfo;
      return _desktopName(info.name, info.prettyName);
    }
    if (Platform.isMacOS) {
      final info = await plugin.macOsInfo;
      return _desktopName(info.computerName, 'macOS');
    }
    if (Platform.isWindows) {
      final info = await plugin.windowsInfo;
      // device_info_plus 只给 build 号；22000 起 Windows 11。
      final build = info.buildNumber;
      final system = build >= 22000
          ? 'Windows 11'
          : build >= 10240
          ? 'Windows 10'
          : 'Windows';
      return _desktopName(info.computerName, system);
    }
    // 兜底：hostname 足够（不拼系统名）。
    return Platform.localHostname.trim().isEmpty
        ? null
        : Platform.localHostname.trim();
  } catch (_) {
    return null;
  }
}
