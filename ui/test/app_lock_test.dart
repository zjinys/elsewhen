// 打开密码哈希核心的单元测试：直接驱动 AppLockService 的
// @visibleForTesting 入口，验证真实实现的算法契约，防止将来逻辑漂移。
import "dart:typed_data";

import "package:elsewhen_ui/utils/app_lock.dart";
import "package:flutter_test/flutter_test.dart";

void main() {
  final saltA = Uint8List.fromList(List.generate(16, (i) => i));
  final saltB = Uint8List.fromList(List.generate(16, (i) => i + 100));

  group("派生", () {
    test("同密码同盐结果确定", () {
      final d1 = AppLockService.debugDeriveKey("correct horse", saltA);
      final d2 = AppLockService.debugDeriveKey("correct horse", saltA);
      expect(d1.length, 32);
      expect(d1, d2);
    });

    test("同密码不同盐结果不同", () {
      final d1 = AppLockService.debugDeriveKey("correct horse", saltA);
      final d3 = AppLockService.debugDeriveKey("correct horse", saltB);
      expect(d1, isNot(d3));
    });

    test("迭代次数参与结果", () {
      final d1 = AppLockService.debugDeriveKey("pw", saltA);
      final d2 = AppLockService.debugDeriveKey("pw", saltA, iterations: 2);
      expect(d1, isNot(d2));
    });
  });

  group("记录编解码", () {
    test("编码后解码原样还原", () {
      final hash = AppLockService.debugDeriveKey("pw", saltA);
      final raw = AppLockService.debugEncodeRecord(
        AppLockRecord(salt: saltA, iterations: 120000, dkLen: 32, hash: hash),
      );
      final decoded = AppLockService.debugDecodeRecord(raw);
      expect(decoded, isNotNull);
      expect(decoded!.salt, saltA);
      expect(decoded.iterations, 120000);
      expect(decoded.dkLen, 32);
      expect(decoded.hash, hash);
    });

    test("坏数据解码返回 null 而非异常", () {
      expect(AppLockService.debugDecodeRecord("not json"), isNull);
      expect(AppLockService.debugDecodeRecord("{}"), isNull);
      expect(
        AppLockService.debugDecodeRecord("v2-kdf-other-json"),
        isNull,
      );
    });
  });

  group("校验", () {
    test("正确密码通过", () {
      final hash = AppLockService.debugDeriveKey("pw", saltA);
      final record =
          AppLockRecord(salt: saltA, iterations: 120000, dkLen: 32, hash: hash);
      expect(AppLockService.debugVerifyRecord(record, "pw"), isTrue);
    });

    test("错误密码拒绝", () {
      final hash = AppLockService.debugDeriveKey("pw", saltA);
      final record =
          AppLockRecord(salt: saltA, iterations: 120000, dkLen: 32, hash: hash);
      expect(AppLockService.debugVerifyRecord(record, "wrong"), isFalse);
      expect(AppLockService.debugVerifyRecord(record, ""), isFalse);
    });
  });
}
