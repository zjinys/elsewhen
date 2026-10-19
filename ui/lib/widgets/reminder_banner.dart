import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';

/// 到期提醒横幅：挂在 _NowStatus 状态栏下方，单条独占一条。
/// 数据源 = RustBridgeRepository.dueReminders（5s tick 发现 + 记 fire + 广播）。
/// 确认/推迟走独立 API（confirmReminder / snoozeReminder），关闭只移除本机横幅。
class ReminderBanner extends ConsumerWidget {
  const ReminderBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final repo = ref.watch(storageRepositoryProvider);
    if (repo is! RustBridgeRepository) return const SizedBox.shrink();
    return ValueListenableBuilder<List<api.DueReminderDto>>(
      valueListenable: repo.dueReminders,
      builder: (context, due, _) {
        if (due.isEmpty) return const SizedBox.shrink();
        // 单条独占一条：一次只弹最旧的一条，其余排队
        final d = due.first;
        return Container(
          width: double.infinity,
          color: AppTheme.accentPrimary.withValues(alpha: 0.12),
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(
            children: [
              Icon(Icons.alarm, size: 18, color: AppTheme.accentPrimary),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  '提醒：${d.title}（${_fmtDue(d.dueAt)}）',
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              TextButton(
                onPressed: () => repo.snoozeReminder(d.todoId),
                child: const Text('+10分钟'),
              ),
              const SizedBox(width: 4),
              FilledButton.tonal(
                onPressed: () => repo.confirmReminder(d.todoId),
                child: const Text('完成'),
              ),
              IconButton(
                tooltip: '关闭（不标记完成）',
                icon: const Icon(Icons.close, size: 18),
                onPressed: () => repo.dismissReminder(d.todoId),
              ),
            ],
          ),
        );
      },
    );
  }
}

String _fmtDue(String dueAt) {
  if (dueAt.isEmpty) return '';
  final dt = DateTime.tryParse(dueAt)?.toLocal();
  if (dt == null) return dueAt;
  final now = DateTime.now();
  final sameDay =
      dt.year == now.year && dt.month == now.month && dt.day == now.day;
  final hm =
      '${dt.hour.toString().padLeft(2, '0')}:${dt.minute.toString().padLeft(2, '0')}';
  return sameDay ? hm : '${dt.month}/${dt.day} $hm';
}
