import 'app_error_notice.dart';
import '../l10n/localization.dart';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../sync/sync_backlog_notice.dart';
import '../theme/app_theme.dart';

/// 最近一次网络同步 tick 的只读状态行（诊断：错误/计数/收敛/版本错位）。
/// 数据由后台 worker 更新；本组件只展示，绝不触发同步。
class SyncTickStatusLine extends StatelessWidget {
  const SyncTickStatusLine({
    super.key,
    required this.ticks,
    required this.errors,
  });

  final ValueListenable<api.SyncTickResultDto?> ticks;
  final ValueListenable<Object?> errors;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([ticks, errors]),
      builder: (context, _) {
        final error = errors.value;
        final tick = ticks.value;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (error != null) ...[
              AppErrorMessage(
                action: context.l10n.sync,
                error: error,
                style: theme.textTheme.bodySmall,
              ),
            ] else if (tick != null) ...[
              Text(
                tick.ran
                    ? context.l10n.l6LastSyncUploadedArg0ReceivedArg120f78(
                        (tick.uploaded).toString(),
                        (tick.pulledAccepted).toString(),
                        (tick.pulledWaiting > 0
                                ? context.l10n
                                      .l6WaitingForDependenciesArg08ff9b(
                                        (tick.pulledWaiting).toString(),
                                      )
                                : '')
                            .toString(),
                        (tick.rosterConverged
                                ? context.l10n.l6RosterResentdba01
                                : '')
                            .toString(),
                        (tick.pulledVersionMismatch > 0
                                ? context.l10n.l6VersionMismatchesArg02d8cc(
                                    (tick.pulledVersionMismatch).toString(),
                                  )
                                : '')
                            .toString(),
                        (tick.acknowledgementFailed
                                ? context.l10n.l6ReceiptFailedf7d93
                                : '')
                            .toString(),
                      )
                    : context.l10n.l6LastSyncSkippedLockedOrBackground113af,
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        );
      },
    );
  }
}

/// 每个对端的投递状态行（R6-1/R6-2）。
///
/// 状态由 Rust 从 outbox 现有列推导（`idle → uploading → awaiting_receipt →
/// backoff → blocked`），这里只把分类码翻成人话。**不给任何按钮**：重传靠时间
/// 与回执自愈，加一个「强制重传」只会让用户在真正卡住时反复制造重复信封。
class SyncDeliverStatusLine extends StatelessWidget {
  const SyncDeliverStatusLine({super.key, required this.statuses});

  final ValueListenable<List<api.SyncDeliveryStatusDto>> statuses;

  /// 分类码 → 人话。没见过的码原样显示：静默吞掉才会让人查不到线索。
  static String reasonLabel(String code, {BuildContext? context}) =>
      _reasonLabel(
        context?.l10n ?? lookupAppLocalizations(const Locale('zh')),
        code,
      );
  static String _reasonLabel(AppLocalizations strings, String code) =>
      switch (code) {
        'relay_quota' => strings.l6RelayMailboxFullWaitingForThec480e,
        'protocol' => strings.l6RelayMessageFormatMismatchUpdateThe1d3c3,
        'integrity' =>
          strings.l6RepeatedContentValidationFailuresDeliveryPausedb04cc,
        'transport' => strings.l6NetworkIsUnstableRetrying2aaa3,
        'replay' => strings.l6Resyncing0f8da,
        _ => code,
      };

  static String stateLabel(
    api.SyncDeliveryStatusDto status, {
    BuildContext? context,
  }) {
    final strings = context?.l10n ?? lookupAppLocalizations(const Locale('zh'));
    switch (status.state) {
      case 'uploading':
        return strings.l6Sendingd948e;
      case 'awaiting_receipt':
        return strings.l6DeliveredToRelayWaitingForPeer2bf3e;
      case 'backoff':
        return strings.l6RetryingLaterArg0baf89(
          (reasonLabel(status.reason, context: context)).toString(),
        );
      case 'blocked':
        // 阻塞不是永久停摆：Rust 在 `until_ms` 里给的是「到点会再试」的时刻。
        // 只说原因（「中继信箱已满」）会让人以为要自己动手处理。
        return status.untilMs > 0
            ? '${reasonLabel(status.reason, context: context)}（${_retryHint(status.untilMs, context)}）'
            : reasonLabel(status.reason, context: context);
      case 'idle':
        return strings.l6Idle837e7;
      default:
        // 没见过的状态码**原样显示**（与 `reasonLabel` 同一个精神）：旧 UI 遇到
        // 新版本 Rust 新增的状态时静默回落成「空闲」会让人以为一切正常 —— 明明
        // 有信封在等、明明状态是新的，界面却说没事，线索就此消失。
        return status.state.isEmpty
            ? strings.l6Idle837e7
            : strings.l6UnknownStateArg0c8bbc((status.state).toString());
    }
  }

  /// 把「到点再试」翻译成一句人话。剩不到两分钟按秒说，再长按分钟说 ——
  /// 让人盯着「还剩 847 秒」没有意义，但「马上就好」和「一刻钟后」差别很大。
  static String _retryHint(int untilMs, BuildContext? context) {
    final strings = context?.l10n ?? lookupAppLocalizations(const Locale('zh'));
    final remainingMs = untilMs - DateTime.now().millisecondsSinceEpoch;
    if (remainingMs <= 0) return strings.l6RetryingSoone5ae3;
    final seconds = (remainingMs / 1000).ceil();
    if (seconds < 120) {
      return strings.l6RetryInArg0Secondsccd79((seconds).toString());
    }
    return strings.l6RetryInArg0Minutesef5e1(
      ((seconds / 60).ceil()).toString(),
    );
  }

  static bool _isAttention(api.SyncDeliveryStatusDto status) =>
      status.state == 'blocked' || status.state == 'backoff';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: statuses,
      builder: (context, _) {
        // 没有待投递的信封 = 全部空闲，不必占位置。
        final active = statuses.value
            .where((status) => status.state != 'idle')
            .toList();
        if (active.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final status in active)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Text(
                  context.l10n
                      .l6DeliveryArg0QueuedArg1AwaitingConfirmationa5864(
                        (_shortPeer(status.peerDeviceId)).toString(),
                        (status.pending - status.awaitingReceipt).toString(),
                        (status.awaitingReceipt).toString(),
                        (stateLabel(status, context: context)).toString(),
                      ),
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: _isAttention(status)
                        ? AppTheme.warning
                        : theme.textTheme.bodySmall?.color,
                  ),
                ),
              ),
          ],
        );
      },
    );
  }

  /// 设备 id 只留前 8 位：面板一行放不下 64 位 hex，而区分对端只要前几位。
  static String _shortPeer(String peer) =>
      peer.length <= 8 ? peer : peer.substring(0, 8);
}

/// D4：`sync_wire_outbox` 里 `state='pending'` 的数据库口径总量。
///
/// 与上面的投递状态流互补而不是重复：投递流说的是「这一拍在等谁、为什么退避」，
/// 这条说的是「库里现在压着多少、多大」—— 后者才是「是不是卡住了」的判断依据。
/// 为 0 时整条不渲染（没有积压就没有话要说）。
class SyncBacklogLine extends StatelessWidget {
  const SyncBacklogLine({
    super.key,
    required this.envelopes,
    required this.bytes,
  });

  final int envelopes;
  final int bytes;

  @override
  Widget build(BuildContext context) {
    if (envelopes <= 0) return const SizedBox.shrink();
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Text(
        context.l10n.l6PeerHasNotConfirmedArg0Itemsc4ccb(
          (envelopes).toString(),
          (formatBacklogBytes(bytes)).toString(),
        ),
        style: theme.textTheme.bodySmall?.copyWith(color: AppTheme.warning),
      ),
    );
  }
}
