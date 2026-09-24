import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// 主窗口上次关闭时的几何信息（逻辑像素）。
class WindowGeometry {
  const WindowGeometry({
    required this.x,
    required this.y,
    required this.width,
    required this.height,
  });

  final double x;
  final double y;
  final double width;
  final double height;

  Size get size => Size(width, height);
  Offset get position => Offset(x, y);

  Map<String, dynamic> toJson() => {
    'x': x,
    'y': y,
    'width': width,
    'height': height,
  };

  /// 从 JSON 恢复；字段缺失/类型不对/尺寸非法返回 null（视为无效配置，
  /// 调用方退回默认值，绝不带着坏几何去设置窗口）。
  static WindowGeometry? tryParse(Object? json) {
    if (json is! Map) return null;
    final x = json['x'];
    final y = json['y'];
    final w = json['width'];
    final h = json['height'];
    if (x is! num || y is! num || w is! num || h is! num) return null;
    if (w <= 0 || h <= 0) return null;
    return WindowGeometry(
      x: x.toDouble(),
      y: y.toDouble(),
      width: w.toDouble(),
      height: h.toDouble(),
    );
  }
}

/// 主窗口几何的本地存储：写到应用数据目录的 `window.json`，
/// 与 elsewhen.db 同目录但不进数据库（用户明确要求不落库）。
class WindowGeometryStore {
  const WindowGeometryStore({this.directoryOverride});

  /// 测试注入用：显式目录覆盖平台推导；null 时走真实平台规则。
  final String? directoryOverride;

  static const String fileName = 'window.json';

  /// 应用数据目录：规则与 Rust 侧 `src/config.rs` 的 `AppConfig::load`
  /// 保持一致（`ELSEWHEN_DATA_DIR` 环境变量优先，否则平台默认数据目录），
  /// 保证 window.json 与 elsewhen.db 落在同一目录、多端行为单一。
  static String? resolveDataDir() {
    final env = Platform.environment['ELSEWHEN_DATA_DIR'];
    if (env != null && env.isNotEmpty) return env;
    if (Platform.isLinux) {
      final home = Platform.environment['HOME'] ?? '';
      final xdg = Platform.environment['XDG_DATA_HOME'];
      final base = (xdg != null && xdg.isNotEmpty) ? xdg : '$home/.local/share';
      return '$base/elsewhen';
    }
    if (Platform.isMacOS) {
      final home = Platform.environment['HOME'] ?? '';
      return '$home/Library/Application Support/dev.elsewhen.elsewhen';
    }
    if (Platform.isWindows) {
      final local =
          Platform.environment['LOCALAPPDATA'] ??
          Platform.environment['APPDATA'];
      return local == null ? null : '$local\\dev\\elsewhen\\elsewhen';
    }
    return null;
  }

  String? get _directory => directoryOverride ?? resolveDataDir();

  File _file() {
    final dir = _directory;
    return dir == null
        ? File(fileName)
        : File('$dir${Platform.pathSeparator}$fileName');
  }

  /// 读取上次保存的窗口几何；无文件/损坏/非法返回 null。
  WindowGeometry? load() {
    try {
      final f = _file();
      if (!f.existsSync()) return null;
      final decoded = jsonDecode(f.readAsStringSync());
      if (decoded is! Map) return null;
      return WindowGeometry.tryParse(decoded);
    } catch (_) {
      return null; // 文件损坏或不可读：当没有历史处理
    }
  }

  void save(WindowGeometry geometry) {
    try {
      final f = _file();
      f.parent.createSync(recursive: true);
      f.writeAsStringSync(
        const JsonEncoder.withIndent('  ').convert(geometry.toJson()),
      );
    } catch (_) {
      // 写失败静默：窗口偏好丢了也只是下次回到默认落位，不值得打断用户。
    }
  }

  /// 把保存的几何夹进可见区（纯函数，便于单测）。
  ///
  /// 规则：
  /// - 尺寸不小于 [minSize]，且不超过所在可见区；
  /// - 以保存的窗口「中心」判断目标屏（多屏下可能不是主屏）；
  /// - 位置夹进目标可见区，保证窗口完全可见；
  /// - 保存值中心不在任何可见区（显示器布局变了，比如拔了外接屏）
  ///   返回 null —— 调用方应退回默认落位而非把窗口摆到屏幕外。
  static Rect? computeRestoreBounds(
    WindowGeometry saved,
    List<Rect> visibleAreas, {
    required Size minSize,
  }) {
    var size = Size(
      math.max(minSize.width, saved.width),
      math.max(minSize.height, saved.height),
    );

    Rect? target;
    if (visibleAreas.isNotEmpty) {
      final center = saved.position + Offset(size.width / 2, size.height / 2);
      for (final r in visibleAreas) {
        if (r.contains(center)) {
          target = r;
          break;
        }
      }
      if (target == null) return null;
      size = Size(
        math.max(minSize.width, math.min(size.width, target.width)),
        math.max(minSize.height, math.min(size.height, target.height)),
      );
    }

    final position = target == null
        ? saved.position
        : Offset(
            math.min(math.max(saved.x, target.left), target.right - size.width),
            math.min(
              math.max(saved.y, target.top),
              target.bottom - size.height,
            ),
          );

    return Rect.fromLTWH(position.dx, position.dy, size.width, size.height);
  }
}
