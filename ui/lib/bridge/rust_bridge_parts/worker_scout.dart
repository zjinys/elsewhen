part of '../rust_bridge_repository.dart';

//! RustBridgeRepository 便捷薄委托（scout 情报域）。
//! 轮询纪律（提案 2026-11-15 §4.2）：分钟级降频 + 离线容忍。挂在与航向
//! 校准相同的 12 拍（≈60s）节奏上，到期与否由本地水位 `last_polled_at`
//! 与 `poll_interval_minutes`（≥5 分钟）判定。研究分析独立于采集间隔，
//! 每拍最多处理一个用户明确启用的证据版本，可能请求已配置的 AI 渠道。
//! 各失败分开记录，不影响其他后台任务。

extension RepoWorkerScout on RustBridgeRepository {
  /// 有界研究分析后检查采集间隔；未到期不请求 Scout 服务。
  Future<void> _tickScoutPoll() async {
    if (_disposed || _scoutPollRunning) return;
    _scoutPollRunning = true;
    try {
      try {
        if (await api.tickScoutResearch()) scoutRevision.value++;
      } catch (e) {
        debugPrint('scout research tick failed: $e');
      }
      if (_disposed) return;
      final settings = await api.getScoutSettings();
      if (_disposed) return;
      if (settings == null || !settings.enabled) return;
      final last = settings.lastPolledAt == null
          ? null
          : DateTime.tryParse(settings.lastPolledAt!);
      final due =
          last == null ||
          DateTime.now().isAfter(
            last.add(Duration(minutes: settings.pollIntervalMinutes)),
          );
      if (!due) return;
      await pollScoutNow();
    } catch (e) {
      // 故障隔离：scout 不可达/未配置只记日志，绝不带崩其他后台流程。
      debugPrint('scout poll tick failed: $e');
    } finally {
      _scoutPollRunning = false;
    }
  }

  /// 手动「立即同步」：与 tick 共用同一条拉取路径；成功后刷新徽标与列表。
  /// 失败上抛（设置页要展示错误文案）。
  Future<ScoutPollResult> pollScoutNow() async {
    final result = await api.pollScoutNow();
    scoutUnreadCount.value = result.unreadTotal;
    scoutRevision.value++;
    return ScoutPollResult(
      fetched: result.fetched,
      newCount: result.newCount,
      unreadTotal: result.unreadTotal,
    );
  }

  /// 标记已读后统一刷新徽标 + revision（列表页订阅）。
  Future<void> markScoutFindingsRead(List<int> ids) async {
    await api.markScoutFindingsRead(ids: Int64List.fromList(ids));
    scoutUnreadCount.value = await api.scoutUnreadCount();
    scoutRevision.value++;
  }

  /// 设置保存 / 镜像清空后刷新徽标。
  Future<void> refreshScoutCounters() async {
    scoutUnreadCount.value = await api.scoutUnreadCount();
    scoutRevision.value++;
  }
}

/// pollScoutNow 的轻量结果（避免 UI 直接依赖 api DTO）。
class ScoutPollResult {
  final int fetched;
  final int newCount;
  final int unreadTotal;
  const ScoutPollResult({
    required this.fetched,
    required this.newCount,
    required this.unreadTotal,
  });
}
