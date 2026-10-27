import 'package:elsewhen_ui/sync/background_failure_policy.dart';
import 'package:flutter_test/flutter_test.dart';

/// roadmap `2026-10-27-sync-lifecycle-boundaries` C2 的判定：
/// 终态失败不重试（退避 + 同因只记一次），瞬时失败保持原样重试。
void main() {
  late DateTime now;
  BackgroundFailurePolicy policy() => BackgroundFailurePolicy(clock: () => now);

  setUp(() => now = DateTime.utc(2026, 10, 27, 12));

  test('领域错误码算终态，抢锁/网络噪声不算', () {
    for (final token in BackgroundFailurePolicy.terminalTokens) {
      expect(
        BackgroundFailurePolicy.isTerminal('AnyhowException($token)'),
        isTrue,
        reason: '$token 必须是终态',
      );
    }
    expect(
      BackgroundFailurePolicy.isTerminal(
        'AnyhowException(database is locked: SqliteFailure(...))',
      ),
      isFalse,
      reason: '抢锁 BUSY 是瞬时失败，重试是对的',
    );
    expect(
      BackgroundFailurePolicy.isTerminal('SocketException: Connection reset'),
      isFalse,
    );
  });

  test('终态失败：同一原因只记一次日志，并进入退避窗口', () {
    final p = policy();
    expect(p.blocked, isFalse);
    expect(p.recordFailure('AnyhowException(sync_not_configured)'), isTrue,
        reason: '第一次必须留下日志');
    expect(p.blocked, isTrue, reason: '终态之后必须退避，不能每拍重试');
    expect(
      p.recordFailure('AnyhowException(sync_not_configured)'),
      isFalse,
      reason: '同一原因第二次必须静默，否则真故障被噪声淹掉',
    );
  });

  test('退避窗口到期后允许再试一拍', () {
    final p = policy();
    p.recordFailure('AnyhowException(sync_not_configured)');
    expect(p.blocked, isTrue);
    now = now.add(const Duration(minutes: 2));
    expect(p.blocked, isFalse, reason: '窗口到期必须能重试，不能永久卡死');
  });

  test('退避指数增长并封顶', () {
    final p = policy();
    p.recordFailure('AnyhowException(sync_not_configured)');
    // 第一次 2min；连续失败 8 次应封顶在 15min，不无限增长
    for (var i = 0; i < 8; i++) {
      now = now.add(const Duration(minutes: 30));
      p.recordFailure('AnyhowException(sync_not_configured)');
      expect(p.blocked, isTrue);
    }
    now = now.add(const Duration(minutes: 14));
    expect(p.blocked, isTrue, reason: '封顶 15min：14 分钟时仍在窗口内');
    now = now.add(const Duration(minutes: 2));
    expect(p.blocked, isFalse);
  });

  test('瞬时失败不推进退避、每次都记日志', () {
    final p = policy();
    const busy = 'AnyhowException(database is locked)';
    expect(p.recordFailure(busy), isTrue);
    expect(p.recordFailure(busy), isTrue,
        reason: '瞬时失败每次都要留下来，否则永远查不出来');
    expect(p.blocked, isFalse, reason: '瞬时失败绝不能退避');
  });

  test('成功后解除退避并重置「已报过」', () {
    final p = policy();
    p.recordFailure('AnyhowException(sync_not_configured)');
    expect(p.blocked, isTrue);
    p.recordSuccess();
    expect(p.blocked, isFalse);
    expect(p.recordFailure('AnyhowException(sync_not_configured)'), isTrue,
        reason: '恢复后再犯必须重新记一条');
  });

  test('用户显式动作（解锁/准备本机）立刻解除退避', () {
    final p = policy();
    p.recordFailure('AnyhowException(sync_not_configured)');
    expect(p.blocked, isTrue);
    p.reset();
    expect(p.blocked, isFalse, reason: '不能让用户「操作成功但要等 15 分钟」');
  });
}
