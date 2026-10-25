import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../theme/app_theme.dart';
import '../utils/error_report.dart';

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
              Text(
                '最近同步错误：${stripErrorShell(error)}',
                style: theme.textTheme.bodySmall,
              ),
            ] else if (tick != null) ...[
              Text(
                tick.ran
                    ? '最近同步：上行 ${tick.uploaded} · 到达 ${tick.pulledAccepted}'
                          '${tick.pulledWaiting > 0 ? ' · 等待依赖 ${tick.pulledWaiting}' : ''}'
                          '${tick.rosterConverged ? ' · 名册已补发' : ''}'
                          '${tick.pulledVersionMismatch > 0 ? ' · 版本不匹配 ${tick.pulledVersionMismatch}' : ''}'
                          '${tick.acknowledgementFailed ? ' · 回执失败' : ''}'
                    : '最近同步：本轮跳过（锁定或后台忙碌）',
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
  static String reasonLabel(String code) => switch (code) {
    'relay_quota' => '中继信箱已满，等对方取件',
    'protocol' => '与中继的消息格式不一致，需要更新版本',
    'integrity' => '内容校验反复失败，已暂停投递',
    'transport' => '网络不稳定，正在重试',
    'replay' => '正在重新同步',
    _ => code,
  };

  static String stateLabel(api.SyncDeliveryStatusDto status) {
    switch (status.state) {
      case 'uploading':
        return '发送中';
      case 'awaiting_receipt':
        return '已送达中继，等对方确认';
      case 'backoff':
        return '稍后重试：${reasonLabel(status.reason)}';
      case 'blocked':
        // 阻塞不是永久停摆：Rust 在 `until_ms` 里给的是「到点会再试」的时刻。
        // 只说原因（「中继信箱已满」）会让人以为要自己动手处理。
        return status.untilMs > 0
            ? '${reasonLabel(status.reason)}（${_retryHint(status.untilMs)}）'
            : reasonLabel(status.reason);
      case 'idle':
        return '空闲';
      default:
        // 没见过的状态码**原样显示**（与 `reasonLabel` 同一个精神）：旧 UI 遇到
        // 新版本 Rust 新增的状态时静默回落成「空闲」会让人以为一切正常 —— 明明
        // 有信封在等、明明状态是新的，界面却说没事，线索就此消失。
        return status.state.isEmpty ? '空闲' : '未知状态：${status.state}';
    }
  }

  /// 把「到点再试」翻译成一句人话。剩不到两分钟按秒说，再长按分钟说 ——
  /// 让人盯着「还剩 847 秒」没有意义，但「马上就好」和「一刻钟后」差别很大。
  static String _retryHint(int untilMs) {
    final remainingMs = untilMs - DateTime.now().millisecondsSinceEpoch;
    if (remainingMs <= 0) return '即将重试';
    final seconds = (remainingMs / 1000).ceil();
    if (seconds < 120) return '$seconds 秒后重试';
    return '${(seconds / 60).ceil()} 分钟后重试';
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
                  '投递 ${_shortPeer(status.peerDeviceId)}'
                  '（待发 ${status.pending - status.awaitingReceipt}'
                  ' · 待确认 ${status.awaitingReceipt}）：'
                  '${stateLabel(status)}',
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
