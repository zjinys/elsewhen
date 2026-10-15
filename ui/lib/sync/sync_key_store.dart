import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

// Keys belong to this installation, never to a synced Provider bundle. Android
// backup exclusions use this exact namespace; changing it requires a migration.
const syncSecureStorage = FlutterSecureStorage(
  aOptions: AndroidOptions(
    storageNamespace: 'elsewhen_sync_keys',
    resetOnError: false,
    migrateWithBackup: false,
  ),
  iOptions: IOSOptions(
    accountName: 'elsewhen.sync.keys',
    accessibility: KeychainAccessibility.first_unlock_this_device,
    synchronizable: false,
  ),
  mOptions: MacOsOptions(
    accountName: 'elsewhen.sync.keys',
    accessibility: KeychainAccessibility.first_unlock_this_device,
    synchronizable: false,
  ),
);

enum SyncKeyStoreError {
  unavailable,
  missing,
  corrupt,
  alreadyExists,
  invalidIdentity,
  unsupportedPlatform,
  disposed,
  revisionRollback,
}

/// 密钥存储健康状态（诊断探针结果，只读展示用）。
enum SyncKeyStoreHealth {
  /// 主密钥记录存在且格式合法。
  ok,

  /// 记录缺失（新装或换了系统账号/设备）。
  missing,

  /// 记录存在但内容损坏；绝不自动替换。
  corrupt,

  /// 系统安全存储不可用/锁定/平台不支持。
  unavailable,
}

class SyncKeyStoreException implements Exception {
  const SyncKeyStoreException(this.code);
  final SyncKeyStoreError code;

  @override
  String toString() => 'SyncKeyStoreException(${code.name})';
}

class SyncMasterKey {
  SyncMasterKey._(this._bytes);
  final Uint8List _bytes;
  bool _disposed = false;

  Uint8List copyBytesForRust() {
    if (_disposed) {
      throw const SyncKeyStoreException(SyncKeyStoreError.disposed);
    }
    return Uint8List.fromList(_bytes);
  }

  void dispose() {
    _bytes.fillRange(0, _bytes.length, 0);
    _disposed = true;
  }

  @override
  String toString() => 'SyncMasterKey([REDACTED])';
}

/// Native keystore adapter. There is deliberately no file/preferences fallback,
/// automatic replacement of a missing key, or delete-on-error path. The pairing
/// coordinator must distinguish a new installation from a restored database.
/// Calls are serialized within this Flutter isolate, not across native processes.
class SyncKeyStore {
  SyncKeyStore({FlutterSecureStorage? storage})
    : _storage = storage ?? syncSecureStorage;

  final FlutterSecureStorage _storage;
  static Future<void>? _pending;
  static final _identity = RegExp(r'^[A-Za-z0-9_-]{1,128}$');

  Future<T> _serialized<T>(Future<T> Function() operation) async {
    final previous = _pending;
    final done = Completer<void>();
    _pending = done.future;
    try {
      if (previous != null) await previous;
      return await operation();
    } finally {
      done.complete();
      if (identical(_pending, done.future)) _pending = null;
    }
  }

  String _key(String installationId) {
    if (kIsWeb || defaultTargetPlatform == TargetPlatform.fuchsia) {
      throw const SyncKeyStoreException(SyncKeyStoreError.unsupportedPlatform);
    }
    if (!_identity.hasMatch(installationId)) {
      throw const SyncKeyStoreException(SyncKeyStoreError.invalidIdentity);
    }
    return 'elsewhen.sync.master.v1.$installationId';
  }

  Future<String?> _read(String key) async {
    try {
      return await _storage.read(key: key);
    } catch (_) {
      // Platform diagnostics may include key material. Never propagate them.
      throw const SyncKeyStoreException(SyncKeyStoreError.unavailable);
    }
  }

  SyncMasterKey _decode(String encoded) {
    try {
      if (encoded.length != 47 || !encoded.startsWith('v1:')) {
        throw const FormatException();
      }
      final bytes = base64Decode(encoded.substring(3));
      if (bytes.length != 32 || 'v1:${base64Encode(bytes)}' != encoded) {
        throw const FormatException();
      }
      return SyncMasterKey._(bytes);
    } catch (_) {
      throw const SyncKeyStoreException(SyncKeyStoreError.corrupt);
    }
  }

  String _revisionKey(String installationId) {
    _key(installationId);
    return 'elsewhen.sync.revision.v1.$installationId';
  }

  int _decodeRevision(String encoded) {
    final value = encoded.length <= 24 && encoded.startsWith('v1:')
        ? int.tryParse(encoded.substring(3))
        : null;
    if (value == null || value <= 0 || 'v1:$value' != encoded) {
      throw const SyncKeyStoreException(SyncKeyStoreError.corrupt);
    }
    return value;
  }

  Future<int> readRevision(String installationId) => _serialized(() async {
    final value = await _read(_revisionKey(installationId));
    if (value == null) {
      throw const SyncKeyStoreException(SyncKeyStoreError.missing);
    }
    return _decodeRevision(value);
  });

  /// Call only while holding the Rust workspace lease. Initial revision 1 is
  /// reserved before creating its DB vault; later revisions follow verified DB
  /// commits, and no network use is allowed until Rust confirms this write.
  Future<void> saveRevision(String installationId, int revision) => _serialized(
    () async {
      if (revision <= 0) {
        throw const SyncKeyStoreException(SyncKeyStoreError.corrupt);
      }
      final master = await _read(_key(installationId));
      if (master == null) {
        throw const SyncKeyStoreException(SyncKeyStoreError.missing);
      }
      _decode(master).dispose();
      final key = _revisionKey(installationId);
      final prior = await _read(key);
      if (prior == null && revision != 1) {
        throw const SyncKeyStoreException(SyncKeyStoreError.missing);
      }
      if (prior != null) {
        final previous = _decodeRevision(prior);
        if (revision < previous) {
          throw const SyncKeyStoreException(SyncKeyStoreError.revisionRollback);
        }
        if (revision == previous) return;
      }
      final encoded = 'v1:$revision';
      try {
        await _storage.write(key: key, value: encoded);
      } catch (_) {
        throw const SyncKeyStoreException(SyncKeyStoreError.unavailable);
      }
      if (await _read(key) != encoded) {
        throw const SyncKeyStoreException(SyncKeyStoreError.unavailable);
      }
    },
  );

  Future<SyncMasterKey> load(String installationId) => _serialized(() async {
    final encoded = await _read(_key(installationId));
    if (encoded == null) {
      throw const SyncKeyStoreException(SyncKeyStoreError.missing);
    }
    return _decode(encoded);
  });

  /// 诊断探针：不修改任何记录，把主密钥记录状态归类为健康枚举。
  ///
  /// 与解锁路径共用同一套 `_read`/`_decode`（同一 fail-closed 语义），
  /// 只是把异常翻译为可展示的状态；平台诊断信息仍不会外泄。
  /// 注意：探针内**不得**调用 `load()`/`readRevision()`——它们各自要拿
  /// `_serialized` 锁，从已持锁的操作里再进入会自死锁（宽限：内联实现读取）。
  Future<SyncKeyStoreHealth> probe(String installationId) =>
      _serialized(() async {
        try {
          final encoded = await _read(_key(installationId));
          if (encoded == null) {
            return SyncKeyStoreHealth.missing;
          }
          _decode(encoded).dispose();
          return SyncKeyStoreHealth.ok;
        } on SyncKeyStoreException catch (error) {
          return switch (error.code) {
            SyncKeyStoreError.missing => SyncKeyStoreHealth.missing,
            SyncKeyStoreError.corrupt => SyncKeyStoreHealth.corrupt,
            _ => SyncKeyStoreHealth.unavailable,
          };
        }
      });

  Future<SyncMasterKey> createForNewInstallation(String installationId) =>
      _serialized(() async {
        final key = _key(installationId);
        if (await _read(key) != null) {
          throw const SyncKeyStoreException(SyncKeyStoreError.alreadyExists);
        }
        String encoded;
        try {
          final random = Random.secure();
          final bytes = Uint8List.fromList(
            List<int>.generate(32, (_) => random.nextInt(256)),
          );
          encoded = 'v1:${base64Encode(bytes)}';
          bytes.fillRange(0, bytes.length, 0);
          await _storage.write(key: key, value: encoded);
        } catch (_) {
          throw const SyncKeyStoreException(SyncKeyStoreError.unavailable);
        }
        if (await _read(key) != encoded) {
          throw const SyncKeyStoreException(SyncKeyStoreError.unavailable);
        }
        return _decode(encoded);
      });

  /// 抹掉一个安装的本机密钥与修订记录。仅用于「删除加入暂存项」这类
  /// 明确放弃该数据集的动作；日常同步绝不调用（fail-closed 前提）。
  Future<void> deleteInstallation(String installationId) =>
      _serialized(() async {
        await _storage.delete(key: _key(installationId));
        await _storage.delete(key: _revisionKey(installationId));
      });
}
