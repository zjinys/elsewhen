import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
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
