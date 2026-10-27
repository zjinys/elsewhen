import '../bridge/api.dart' as api;

/// 主界面同步状态位要显示的「积压」判定（roadmap C8 / 缺陷 D4）。
///
/// 缺陷本身：同步暂停（本机密钥锁定）或积压时，主界面**什么都没有** ——
/// 用户只能看到「消息没同步过去」，看不出是「要解锁」还是「在等对端」。
/// 两者的动作完全不同（前者点一下就通，后者只能等），所以状态位必须先分清。
enum SyncBacklogKind {
  /// 本机密钥锁定：积压是本机解不开、也就发不出去。动作 = 解锁。
  lockedBacklog,

  /// 本机已就绪、封包已发出但没收到对端回执。动作 = 看进度，没有可点的修复。
  awaitingPeer,
}

class SyncBacklogNotice {
  const SyncBacklogNotice({
    required this.kind,
    required this.envelopes,
    required this.bytes,
  });

  final SyncBacklogKind kind;
  final int envelopes;
  final int bytes;

  /// 只有「要解锁」这一类才有可点的动作；等对端时不给假按钮。
  bool get actionable => kind == SyncBacklogKind.lockedBacklog;

  String get message => switch (kind) {
    SyncBacklogKind.lockedBacklog =>
      '待解锁积压 $envelopes 条 · $_size，解开本机密钥后自动同步',
    SyncBacklogKind.awaitingPeer =>
      '同步延迟：对端未确认 $envelopes 条 · $_size，正在重试',
  };

  String get actionLabel => '去解锁';

  /// 只渲染字节量级：条数已经在两条文案里各自说过了，再附一次会读成
  /// 「对端未确认 1 条 · 2.7 KB（1 条）」——真机首屏当场撞到的重复。
  String get _size => formatBacklogBytes(bytes);
}

/// 人类可读的字节量级：积压是「还要不要等」的判断依据，裸字节数帮不上这个忙。
String formatBacklogBytes(int bytes) {
  if (bytes < 1024) return '$bytes B';
  final kb = bytes / 1024;
  if (kb < 1024) {
    return '${kb < 10 ? kb.toStringAsFixed(1) : kb.round()} KB';
  }
  final mb = kb / 1024;
  return '${mb < 10 ? mb.toStringAsFixed(1) : mb.round()} MB';
}

/// 判定入口（纯函数）。返回 `null` 表示主界面不该出现状态位。
///
/// 刻意不覆盖 `requires_repair` / `not_configured`：前者有自己的出口（同步页的
/// 「清除本机同步身份」），后者本来就没有同步，摆一条常驻状态位只会变成噪音。
SyncBacklogNotice? syncBacklogNotice(api.LocalSyncStateDto? state) {
  if (state == null) return null;
  final envelopes = state.pendingWireEnvelopes;
  if (envelopes <= 0) return null;
  return switch (state.state) {
    'locked' => SyncBacklogNotice(
      kind: SyncBacklogKind.lockedBacklog,
      envelopes: envelopes,
      bytes: state.pendingBytes,
    ),
    'local_ready' => SyncBacklogNotice(
      kind: SyncBacklogKind.awaitingPeer,
      envelopes: envelopes,
      bytes: state.pendingBytes,
    ),
    _ => null,
  };
}
