import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// 桌面单实例守卫。
///
/// 目标：系统级快捷键（见 SHORTCUTS_SETUP.md，`Super+Space` 调
/// `scripts/elsewhen-capture.sh` 再跑一次 `flutter run`）在已有实例运行时，
/// 应把命令转发给该实例并唤出窗口，而不是起第二个进程抢同一个 SQLite 库。
///
/// 实现：以「数据目录」为身份，在运行目录建一个 Unix 域 socket。
/// - 第一个进程 bind 成功 → primary，持续监听后续连接；
/// - 后来的进程 connect 成功 → 写入命令后返回 secondary，调用方应退出。
///
/// 仅覆盖 Linux / macOS（dart:io 的 Unix 域 socket）。其它平台直接返回
/// primary（不拦截），避免在未验证的 IPC 路径上引入风险。
class SingleInstance {
  SingleInstance._(this._server, this._path);

  final ServerSocket _server;
  final String _path;

  static SingleInstance? _active;

  /// 保证当前进程是唯一实例。
  ///
  /// - 已是 primary：返回 true，并在后台用 [onCommand] 处理后续实例发来的命令；
  /// - 已有 primary：把 [command] 转发过去，返回 false（调用方应 `exit(0)`）；
  /// - 非 Linux/macOS：直接返回 true（不做拦截）。
  ///
  /// [identity] 通常是数据目录路径，保证不同数据目录各自独立（互不干扰）。
  static Future<bool> ensurePrimary({
    required String identity,
    required String command,
    required void Function(String command) onCommand,
  }) async {
    if (!(Platform.isLinux || Platform.isMacOS)) return true;
    if (_active != null) return true;

    final path = _socketPath(identity);

    // 1) 已有实例在跑：转发命令后让位。
    if (await _trySend(path, command)) return false;

    // 2) 连接失败可能是上次崩溃残留的 socket 文件，清掉再抢。
    try {
      final file = File(path);
      if (file.existsSync()) file.deleteSync();
    } catch (_) {
      // 删不掉就继续尝试 bind，失败还有下面的兜底。
    }

    ServerSocket server;
    try {
      server = await ServerSocket.bind(
        InternetAddress(path, type: InternetAddressType.unix),
        0,
      );
    } catch (_) {
      // 抢不到又连不上：可能是并发第二实例刚删了文件又 bind 成功。
      // 再转发一次；仍失败则放行（宁可多开一个，也不把用户挡在门外）。
      if (await _trySend(path, command)) return false;
      return true;
    }

    final instance = SingleInstance._(server, path);
    _active = instance;
    server.listen((socket) {
      final buf = StringBuffer();
      socket.listen(
        (chunk) {
          buf.write(utf8.decode(chunk, allowMalformed: true));
          final text = buf.toString();
          final nl = text.indexOf('\n');
          if (nl >= 0) {
            final cmd = text.substring(0, nl).trim();
            if (cmd.isNotEmpty) onCommand(cmd);
            unawaited(socket.close());
          }
        },
        onError: (_) {},
        cancelOnError: true,
      );
    }, onError: (_) {});
    return true;
  }

  static Future<bool> _trySend(String path, String command) async {
    try {
      final socket = await Socket.connect(
        InternetAddress(path, type: InternetAddressType.unix),
        0,
        timeout: const Duration(milliseconds: 500),
      );
      socket.write('$command\n');
      await socket.flush();
      await socket.close();
      return true;
    } catch (_) {
      return false;
    }
  }

  static String _socketPath(String identity) {
    final runtime = Platform.environment['XDG_RUNTIME_DIR'];
    final base = (runtime != null && runtime.isNotEmpty)
        ? runtime
        : Directory.systemTemp.path;
    final id = identity.hashCode.toUnsigned(32).toRadixString(16);
    return '$base${Platform.pathSeparator}elsewhen-$id.sock';
  }

  /// 释放监听并清理 socket 文件（退出前调用；幂等）。
  static Future<void> dispose() async {
    final instance = _active;
    if (instance == null) return;
    _active = null;
    try {
      await instance._server.close();
    } catch (_) {}
    try {
      final file = File(instance._path);
      if (file.existsSync()) file.deleteSync();
    } catch (_) {}
  }
}
