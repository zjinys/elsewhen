/// 系统字体枚举（经 Rust 桥 fontdb 跨平台实现：Linux 解析 fontconfig 配置、
/// macOS/Windows 扫描系统字体目录；枚举失败返回空列表，调用方隐藏
/// “本地字体”一节即可）。
///
/// 存值格式（app_meta `theme_font` 不透明字符串，Rust 侧不动）：
/// - `system`：跟随系统默认字体；
/// - `google:<family>`：Google Fonts（联网下载，沿用旧行为）；
/// - `local:<family>`：fontconfig 本地字体（如 `LXGW WenKai Mono`）；
/// - 无前缀历史值：Google 表里有 → google，否则按本地解析（旧逻辑回退 Inter）。
library;

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:google_fonts/google_fonts.dart';

import '../bridge/generated.dart/api.dart' as api;

/// 解析后的存值。
class StoredFont {
  const StoredFont(this.kind, this.family);

  /// 'system' | 'google' | 'local'
  final String kind;
  final String family;
}

/// 解析字体存值（纯函数，可单测）。
StoredFont parseStoredFont(String stored) {
  if (stored == 'system') return const StoredFont('system', '');
  if (stored.startsWith('google:')) {
    return StoredFont('google', stored.substring('google:'.length));
  }
  if (stored.startsWith('local:')) {
    return StoredFont('local', stored.substring('local:'.length));
  }
  if (GoogleFonts.asMap().containsKey(stored)) {
    return StoredFont('google', stored);
  }
  return StoredFont('local', stored);
}

/// 求值：存值 → 可直接用于 `TextStyle.fontFamily` 的家族名。
/// - system → null（不注入，跟随系统）；
/// - google 未知家族 / 本地缺文件 → [fallback]（沿用旧“未知回退默认”契约）；
/// - 本地字体：只要文件索引里有就返回家族名（加载是异步预热的，
///   就绪后 notifier 会触发重建，见 [SystemFontService.ensureLoadedForStored]）。
String? resolveFontFamily(String? stored, {required String fallback}) {
  if (stored == null || stored.isEmpty) return null;
  final parsed = parseStoredFont(stored);
  switch (parsed.kind) {
    case 'system':
      return null;
    case 'google':
      return GoogleFonts.asMap().containsKey(parsed.family)
          ? parsed.family
          : fallback;
    default:
      return SystemFontService.instance.fileForFamilySync(parsed.family) != null
          ? parsed.family
          : fallback;
  }
}

/// 一条 fontconfig 记录（同一家族只保留一个首选文件，字重由引擎合成）。
class SystemFontEntry {
  const SystemFontEntry({
    required this.family,
    required this.file,
    required this.style,
  });

  final String family;
  final String file;
  final String style;
}

class _FileAndStyle {
  _FileAndStyle(this.file, this.style);
  final String file;
  final String style;
}

class SystemFontService {
  SystemFontService._();
  static final SystemFontService instance = SystemFontService._();

  List<SystemFontEntry>? _entries;
  final Map<String, String> _fileIndex = {};
  final Set<String> _loaded = {};
  final Map<String, Future<bool>> _inflight = {};

  /// 文件索引是否就绪（就绪前 [fileForFamilySync] 恒返回 null）。
  bool get isCacheReady => _entries != null;

  bool isLoaded(String family) => _loaded.contains(family);

  /// 测试环境（flutter test 的 FakeAsync 区）下不跑真实 FFI：桥的 Future
  /// 在 fake async 里永远不会完成，字体行会一直转圈 → 拖垮渲染字体行的
  /// widget 测试。直接空列表，行为与「枚举失败」一致。
  static bool get _isTestEnvironment =>
      Platform.environment.containsKey('FLUTTER_TEST');

  /// 枚举系统字体（Rust 桥 fontdb；结果缓存，失败/未初始化返回空列表）。
  Future<List<SystemFontEntry>> listFonts() async {
    if (_entries != null) return _entries!;
    final out = <SystemFontEntry>[];
    if (_isTestEnvironment) {
      // 测试里不枚举，任何本地字体都视为缺文件，行为与「枚举失败」一致。
    } else {
      try {
        final faces = await api.listSystemFonts();
        out.addAll(_buildEntries(faces));
      } catch (_) {
        // RustLib 未就绪 / FFI 失败：本地一节隐藏，不影响 Google Fonts。
      }
    }
    _entries = out;
    _fileIndex
      ..clear()
      ..addEntries(out.map((e) => MapEntry(e.family, e.file)));
    return out;
  }

  /// 按家族聚合字面列表：同族内保留首选样式（[_styleRank] 最小者），
  /// 整体按家族名排序。沿用旧 fc-list 时期的语义。
  static List<SystemFontEntry> _buildEntries(List<api.SystemFontFace> faces) {
    final byFamily = <String, List<_FileAndStyle>>{};
    for (final f in faces) {
      final family = f.family.trim();
      final file = f.file.trim();
      final style = f.style.trim();
      if (family.isEmpty || file.isEmpty) continue;
      byFamily.putIfAbsent(family, () => []).add(_FileAndStyle(file, style));
    }
    final out = <SystemFontEntry>[];
    for (final e in byFamily.entries) {
      e.value.sort(
        (a, b) => _styleRank(a.style).compareTo(_styleRank(b.style)),
      );
      final best = e.value.first;
      out.add(
        SystemFontEntry(family: e.key, file: best.file, style: best.style),
      );
    }
    out.sort(
      (a, b) => a.family.toLowerCase().compareTo(b.family.toLowerCase()),
    );
    return out;
  }

  /// 同风格优先级：Regular 系优先，其次 Normal/Book/Medium，其它按原序。
  static int _styleRank(String style) {
    final s = style.toLowerCase();
    if (s.contains('regular')) return 0;
    if (s.contains('normal') ||
        s.contains('book') ||
        s.contains('medium') ||
        s.contains('roman')) {
      return 1;
    }
    return 2;
  }

  /// 同步查文件（索引未就绪返回 null）。精确匹配优先，其次忽略大小写。
  String? fileForFamilySync(String family) {
    final hit = _fileIndex[family];
    if (hit != null) return hit;
    final lower = family.toLowerCase();
    for (final e in _fileIndex.entries) {
      if (e.key.toLowerCase() == lower) return e.value;
    }
    return null;
  }

  /// 把本地字体注册进 Flutter（文件字节 → FontLoader），幂等、并发安全。
  Future<bool> ensureLoaded(String family) {
    if (_loaded.contains(family)) return Future.value(true);
    return _inflight.putIfAbsent(family, () => _load(family));
  }

  Future<bool> _load(String family) async {
    try {
      var file = fileForFamilySync(family);
      if (file == null) {
        await listFonts();
        file = fileForFamilySync(family);
      }
      if (file == null) return false;
      final bytes = await File(file).readAsBytes();
      final loader = FontLoader(family)
        ..addFont(Future.value(ByteData.view(bytes.buffer)));
      await loader.load();
      _loaded.add(family);
      return true;
    } catch (_) {
      return false;
    } finally {
      _inflight.remove(family);
    }
  }

  /// 按存值预热（local 才加载；system/google 直接过）。
  Future<void> ensureLoadedForStored(String? stored) async {
    if (stored == null || stored.isEmpty) return;
    final parsed = parseStoredFont(stored);
    if (parsed.kind == 'local' && parsed.family.isNotEmpty) {
      await ensureLoaded(parsed.family);
    }
  }
}
