import 'dart:convert';
import 'dart:io';

import 'window_geometry_store.dart' show WindowGeometryStore;

/// 记录用户已主动取消/关闭的生成任务（ChatRun）ID。
/// 确保用户点击取消后，状态条在切会话或重启 App 后不再重复弹出。
class DismissedRunsStore {
  DismissedRunsStore._() {
    _load();
  }

  static final DismissedRunsStore instance = DismissedRunsStore._();

  static const String fileName = 'dismissed_chat_runs.json';
  static String? _directoryOverride;

  /// 用于测试或移动端注入目录
  static void setDirectory(String dir) {
    _directoryOverride = dir;
    instance._load();
  }

  final Set<String> _dismissedIds = {};

  String? get _directory =>
      _directoryOverride ?? WindowGeometryStore.resolveDataDir();

  File _file() {
    final dir = _directory;
    return dir == null
        ? File(fileName)
        : File('$dir${Platform.pathSeparator}$fileName');
  }

  void _load() {
    try {
      final f = _file();
      if (!f.existsSync()) return;
      final raw = f.readAsStringSync();
      final decoded = jsonDecode(raw);
      if (decoded is List) {
        _dismissedIds.clear();
        for (final item in decoded) {
          if (item is String && item.isNotEmpty) {
            _dismissedIds.add(item);
          }
        }
      }
    } catch (_) {
      // 容错读取
    }
  }

  bool isDismissed(String? runId) {
    if (runId == null || runId.isEmpty) return false;
    return _dismissedIds.contains(runId);
  }

  Future<void> dismissRun(String runId) async {
    if (runId.isEmpty || _dismissedIds.contains(runId)) return;
    _dismissedIds.add(runId);
    try {
      final f = _file();
      if (f.parent.path.isNotEmpty && !f.parent.existsSync()) {
        f.parent.createSync(recursive: true);
      }
      // 最多保留最近 500 个已取消 run，防止文件无限增大
      final list = _dismissedIds.toList();
      final bounded = list.length > 500 ? list.sublist(list.length - 500) : list;
      await f.writeAsString(jsonEncode(bounded));
    } catch (_) {
      // 写入失败静默，内存态仍然生效
    }
  }
}
