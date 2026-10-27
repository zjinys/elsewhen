/// 后台 worker 失败的**分类与退避**（roadmap `2026-10-27-sync-lifecycle-boundaries` C2）。
///
/// 为什么需要它：worker 的 catch 原本把两种完全不同的失败当同一种处理 ——
/// * **终态**：`sync_not_configured` / `sync_not_initialized` / `sync_vault_missing`。
///   同一条原因重试一万次也不会变好（用户没准备同步身份、密钥没解锁、库被换掉）。
///   每拍重试的唯一产物是日志噪声 + 白烧 CPU，而且真正的故障被噪声淹掉。
/// * **瞬时**：抢锁 BUSY、网络抖动。这些重试是对的，必须保持原样。
///
/// 本类只做判定与状态推进，**不碰 IO、不读系统时间**（时钟可注入），因此可以纯单测。
/// 判定依据是错误文本里的领域错误码 —— 这些码是 Rust 侧 `anyhow` 的稳定契约。
class BackgroundFailurePolicy {
  BackgroundFailurePolicy({
    this.initialBackoff = const Duration(minutes: 2),
    this.maxBackoff = const Duration(minutes: 15),
    DateTime Function()? clock,
  }) : _clock = clock ?? DateTime.now;

  /// 终态错误的领域错误码。改了 Rust 侧的错误码就必须同步改这里。
  static const List<String> terminalTokens = <String>[
    'sync_not_configured',
    'sync_not_initialized',
    'sync_vault_missing',
  ];

  final Duration initialBackoff;
  final Duration maxBackoff;
  final DateTime Function() _clock;

  DateTime? _blockedUntil;
  Duration _backoff = Duration.zero;

  /// 同一终态原因只记一次日志：`seen` 记录已经报过的错误码。
  final Set<String> _reported = <String>{};

  /// 当前是否处于退避窗口内（窗口内的 wake 直接跳过，不跑 tick）。
  bool get blocked {
    final until = _blockedUntil;
    return until != null && _clock().isBefore(until);
  }

  /// 终态判定。瞬时错误返回 false（调用方保持现有重试）。
  static bool isTerminal(Object error) {
    final text = '$error';
    return terminalTokens.any(text.contains);
  }

  /// 记录一次失败，返回**是否应该打印日志**。
  ///
  /// * 终态：推进退避（指数增长到 [maxBackoff]），同一错误码只在第一次返回 true；
  /// * 瞬时：不动退避，每次都返回 true（这些才是真正需要看到的日志）。
  bool recordFailure(Object error) {
    if (!isTerminal(error)) return true;
    final text = '$error';
    _backoff = _backoff == Duration.zero
        ? initialBackoff
        : Duration(
            milliseconds: (_backoff.inMilliseconds * 2).clamp(
              0,
              maxBackoff.inMilliseconds,
            ).toInt(),
          );
    _blockedUntil = _clock().add(_backoff);
    return _reported.add(_terminalKey(text));
  }

  /// 成功一次即解除退避并清空「已报过」，下次故障重新记一条。
  void recordSuccess() {
    _blockedUntil = null;
    _backoff = Duration.zero;
    _reported.clear();
  }

  /// 用户显式动作（例如刚在设置里点了「准备本机」/解锁）应立刻解除退避，
  /// 否则用户会看到「操作成功但后台要等 15 分钟才恢复」。
  void reset() => recordSuccess();

  /// 只取命中的那个错误码作 key，避免把整条错误文本（含 id/时间戳）当 key 导致
  /// 每次都被当成「新原因」重复打印。
  String _terminalKey(String text) =>
      terminalTokens.firstWhere(text.contains, orElse: () => text);
}
