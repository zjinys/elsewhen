import "dart:convert";
import "dart:io";
import "dart:math";

import "package:crypto/crypto.dart";
import "package:flutter/foundation.dart";
import "package:flutter_secure_storage/flutter_secure_storage.dart";
import "package:local_auth/local_auth.dart";

// 打开密码的哈希记录属于本机安装，不随同步、不进云备份。命名空间与
// elsewhen_sync_keys 平级独立：Android 备份排除规则按命名空间匹配，
// 改动它需要有迁移方案。
const appLockSecureStorage = FlutterSecureStorage(
  aOptions: AndroidOptions(
    storageNamespace: "elsewhen_app_lock",
    resetOnError: false,
    migrateWithBackup: false,
  ),
  iOptions: IOSOptions(
    accountName: "elsewhen.app.lock",
    accessibility: KeychainAccessibility.first_unlock_this_device,
    synchronizable: false,
  ),
  mOptions: MacOsOptions(
    accountName: "elsewhen.app.lock",
    accessibility: KeychainAccessibility.first_unlock_this_device,
    synchronizable: false,
  ),
);

const _kRecordKey = "app_lock.password_record.v1";
const _kBiometricKey = "app_lock.biometric_enabled.v1";
const _kIterations = 120000;
const _kSaltBytes = 16;
const _kDkLen = 32;

/// 打开密码记录：仅含盐、迭代参数与派生哈希，永不含明文。
class AppLockRecord {
  const AppLockRecord({
    required this.salt,
    required this.iterations,
    required this.dkLen,
    required this.hash,
  });

  final Uint8List salt;
  final int iterations;
  final int dkLen;
  final Uint8List hash;
}

/// 打开密码服务。策略（SHA-256 加盐多轮派生 + 常数时间比较）与存储分离：
/// 纯函数核心可直接单测；平台封装走 flutter_secure_storage。
class AppLockService {
  AppLockService({FlutterSecureStorage? storage})
      : _storage = storage ?? appLockSecureStorage;

  final FlutterSecureStorage _storage;
  static final _random = Random.secure();

  static Uint8List _deriveKey(
    String password,
    Uint8List salt, {
    int iterations = _kIterations,
    int dkLen = _kDkLen,
  }) {
    var block = Uint8List.fromList([
      ...salt,
      ...utf8.encode(password),
    ]);
    var digest = sha256.convert(block).bytes;
    for (var i = 1; i < iterations; i++) {
      block = Uint8List.fromList([...digest, ...salt]);
      digest = sha256.convert(block).bytes;
    }
    if (digest.length == dkLen) return Uint8List.fromList(digest);
    return Uint8List.fromList(digest.sublist(0, dkLen));
  }

  static bool _constantTimeEquals(Uint8List a, Uint8List b) {
    if (a.length != b.length) return false;
    var diff = 0;
    for (var i = 0; i < a.length; i++) {
      diff |= a[i] ^ b[i];
    }
    return diff == 0;
  }

  static Uint8List _newSalt() => Uint8List.fromList(
        List.generate(_kSaltBytes, (_) => _random.nextInt(256)),
      );

  static String _encodeRecord(AppLockRecord record) => jsonEncode({
        "v": 1,
        "kdf": "sha256-iterated",
        "iterations": record.iterations,
        "dklen": record.dkLen,
        "salt": base64Encode(record.salt),
        "hash": base64Encode(record.hash),
      });

  static AppLockRecord? _decodeRecord(String raw) {
    try {
      final map = jsonDecode(raw);
      if (map is! Map<String, dynamic>) return null;
      if (map["v"] != 1 || map["kdf"] != "sha256-iterated") return null;
      final iterations = map["iterations"];
      final dkLen = map["dklen"];
      final salt = map["salt"];
      final hash = map["hash"];
      if (iterations is! int ||
          dkLen is! int ||
          salt is! String ||
          hash is! String) {
        return null;
      }
      return AppLockRecord(
        salt: base64Decode(salt),
        iterations: iterations,
        dkLen: dkLen,
        hash: base64Decode(hash),
      );
    } catch (_) {
      return null;
    }
  }

  /// 是否已设置打开密码。
  Future<bool> isEnabled() =>
      _storage.read(key: _kRecordKey).then((v) => v != null);

  /// 设置新密码。当前未设置时直接写入；已设置时需先校验 [currentPassword]。
  Future<bool> setPassword(
    String newPassword, {
    String? currentPassword,
  }) async {
    final existing = await _storage.read(key: _kRecordKey);
    if (existing != null) {
      final record = _decodeRecord(existing);
      if (record == null ||
          currentPassword == null ||
          !_verifyRecord(record, currentPassword)) {
        return false;
      }
    }
    final salt = _newSalt();
    final record = AppLockRecord(
      salt: salt,
      iterations: _kIterations,
      dkLen: _kDkLen,
      hash: _deriveKey(newPassword, salt),
    );
    await _storage.write(key: _kRecordKey, value: _encodeRecord(record));
    return true;
  }

  /// 校验输入密码。未设置密码时视为不通过（锁屏只在已设置时才会出现）。
  Future<bool> verifyPassword(String password) async {
    final raw = await _storage.read(key: _kRecordKey);
    if (raw == null) return false;
    final record = _decodeRecord(raw);
    if (record == null) return false;
    return _verifyRecord(record, password);
  }

  /// 关闭打开密码。需先校验当前密码，防止他人拿到已解锁设备直接关锁。
  Future<bool> disable(String currentPassword) async {
    if (!await verifyPassword(currentPassword)) return false;
    await _storage.delete(key: _kRecordKey);
    await _storage.delete(key: _kBiometricKey);
    return true;
  }

  // ── 仅供测试触达的纯函数入口（无存储副作用）──
  // 私有静态无法跨库断言；这些包装只暴露算法契约，不暴露任何存储路径。
  @visibleForTesting
  static Uint8List debugDeriveKey(
    String password,
    Uint8List salt, {
    int iterations = _kIterations,
    int dkLen = _kDkLen,
  }) =>
      _deriveKey(password, salt, iterations: iterations, dkLen: dkLen);

  @visibleForTesting
  static String debugEncodeRecord(AppLockRecord record) =>
      _encodeRecord(record);

  @visibleForTesting
  static AppLockRecord? debugDecodeRecord(String raw) => _decodeRecord(raw);

  @visibleForTesting
  static bool debugVerifyRecord(AppLockRecord record, String password) =>
      _verifyRecord(record, password);

  // ── 生物识别（指纹/面容）：仅移动端生效，桌面端一律返回不支持 ──

  static final LocalAuthentication _localAuth = LocalAuthentication();

  /// 设备是否具备可用生物验证：硬件支持且已录入至少一项生物特征。
  /// 桌面端直接返回 false，不触碰平台通道。
  Future<bool> biometricAvailable() async {
    if (kIsWeb || !(Platform.isAndroid || Platform.isIOS)) return false;
    try {
      if (!await _localAuth.isDeviceSupported()) return false;
      final enrolled = await _localAuth.getAvailableBiometrics();
      return enrolled.isNotEmpty;
    } catch (_) {
      return false;
    }
  }

  /// 用户是否已开启生物识别解锁（开关与密码记录同库，随关锁一并清除）。
  Future<bool> biometricUnlockEnabled() async {
    if (kIsWeb || !(Platform.isAndroid || Platform.isIOS)) return false;
    final v = await _storage.read(key: _kBiometricKey);
    return v == "1";
  }

  /// 开启/关闭生物识别解锁。开启前先校验当前密码（与设置页流程衔接：
  /// 调用方保证已验证），再确认设备当前仍可用生物验证。
  Future<bool> setBiometricUnlock(bool enable) async {
    if (kIsWeb || !(Platform.isAndroid || Platform.isIOS)) return false;
    if (enable && !await biometricAvailable()) return false;
    if (enable) {
      await _storage.write(key: _kBiometricKey, value: "1");
    } else {
      await _storage.delete(key: _kBiometricKey);
    }
    return true;
  }

  /// 调起系统生物验证。成功返回 true；用户取消/失败/不可用返回 false，
  /// 由调用方回落到密码输入。生物识别永不替代密码本体。
  Future<bool> authenticateWithBiometrics() async {
    if (kIsWeb || !(Platform.isAndroid || Platform.isIOS)) return false;
    try {
      return await _localAuth.authenticate(
        localizedReason: "验证指纹或面容以解锁 Elsewhen",
        options: const AuthenticationOptions(
          biometricOnly: true,
          stickyAuth: true,
        ),
      );
    } catch (_) {
      return false;
    }
  }

  static bool _verifyRecord(AppLockRecord record, String password) {
    final candidate = _deriveKey(
      password,
      record.salt,
      iterations: record.iterations,
      dkLen: record.dkLen,
    );
    return _constantTimeEquals(candidate, record.hash);
  }
}
