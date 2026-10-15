import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import 'state_holder.dart';

/// 回顾页（R1）：日 / 周 / 月三粒度。事件浏览域已并入回顾——日视图里
/// 呈现当天事件（原文展开 + 当日 AI 回顾勾连），周/月是实时 SQL 拼装
/// 统计，无 AI、不落表（`deviation_reports` 推迟到偏差检测切片）。
enum ReviewGranularity { day, week, month }

final reviewGranularityProvider =
    NotifierProvider<StateHolder<ReviewGranularity>, ReviewGranularity>(
      () => StateHolder(ReviewGranularity.day),
    );

/// 三粒度共用一个锚点日期：周/月视图锚定所在周/月，日视图锚定当日。
final reviewAnchorDateProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder(_today()),
);

String _today() {
  final now = DateTime.now();
  return '${now.year}-${now.month.toString().padLeft(2, '0')}-${now.day.toString().padLeft(2, '0')}';
}

/// 日视图读面：AI 回顾 + 当日事件 + 待办 + 规则提醒 + 记忆回引。
final dailyOverviewProvider = FutureProvider.autoDispose
    .family<api.DailyOverviewDto, String>((ref, date) {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.getDailyOverview(date);
    });

/// 周/月统计读面。family key 带粒度：日内容变化时不会串视图。
final periodStatsProvider = FutureProvider.autoDispose
    .family<api.PeriodStatsDto, (String, String)>((ref, key) {
      final (periodType, date) = key;
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.getPeriodStats(periodType, date);
    });

/// 周期盘点报告读面（R4）：family key = 粒度（week/month），最新在前。
/// 未读 = 任一报告 readAt 为空；忽略单条偏差后刷新本 provider。
final deviationReportsProvider = FutureProvider.autoDispose
    .family<List<api.DeviationReportDto>, String>((ref, periodType) {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.listDeviationReports(periodType: periodType);
    });

/// 偏差类型中文呈现（枚举封闭，未知的原样透出，便于发现协议漂移）。
String deviationTypeLabel(String type) => switch (type) {
      'no_progress' => '无进展',
      'stalled' => '停滞',
      'conflict' => '相悖',
      'scope_drift' => '目标偏移',
      _ => type,
    };
