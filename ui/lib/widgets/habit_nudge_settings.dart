import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../theme/app_theme.dart';
import '../utils/error_report.dart';
import 'habit_nudge_dialog.dart';

/// 设置页「习惯提醒」列表的一行：习惯页 + 它当前的问候计划。
class HabitReminderEntry {
  final WikiPageMeta page;
  final api.HabitNudgeDto? plan;

  const HabitReminderEntry({required this.page, required this.plan});
}

/// 汇总所有未归档习惯页及其问候计划。
///
/// 计划本身仍按页存储（一页一条），这里只做聚合读；任何一页保存/删除后
/// `habitNudgeRevisionProvider` 变化，列表自动重查。页面标题等元数据来自
/// `listWikiPageMetas`，不额外读取正文。
final habitReminderEntriesProvider = FutureProvider<List<HabitReminderEntry>>((
  ref,
) async {
  ref.watch(knowledgeRevisionProvider);
  ref.watch(habitNudgeRevisionProvider);
  final repo = ref.watch(storageRepositoryProvider);
  if (repo is! RustBridgeRepository) return const [];

  final pages = await repo.listWikiPageMetas();
  final entries = <HabitReminderEntry>[];
  for (final page in pages) {
    if (page.kind != 'habit' || page.status != 'active') continue;
    entries.add(
      HabitReminderEntry(
        page: page,
        plan: await repo.getHabitNudgeForPage(page.slug),
      ),
    );
  }
  entries.sort(_compareEntries);
  return entries;
});

/// 运行中的排前面，其次按当天触发时间，再按标题；未设置为最后。
int _compareEntries(HabitReminderEntry a, HabitReminderEntry b) {
  final aEnabled = a.plan?.enabled ?? false;
  final bEnabled = b.plan?.enabled ?? false;
  if (aEnabled != bEnabled) return aEnabled ? -1 : 1;
  final aTime = a.plan?.localTime;
  final bTime = b.plan?.localTime;
  if (aTime != bTime) {
    if (aTime == null) return 1;
    if (bTime == null) return -1;
    return aTime.compareTo(bTime);
  }
  return a.page.title.compareTo(b.page.title);
}

/// 设置页里的集中维护区。可嵌入 `_buildSection` 的 child。
class HabitNudgeSettingsContent extends ConsumerStatefulWidget {
  const HabitNudgeSettingsContent({super.key});

  @override
  ConsumerState<HabitNudgeSettingsContent> createState() =>
      _HabitNudgeSettingsContentState();
}

class _HabitNudgeSettingsContentState
    extends ConsumerState<HabitNudgeSettingsContent> {
  /// 正在切换开关的计划 id，避免连点写出两次相反状态。
  final Set<String> _busy = {};

  @override
  Widget build(BuildContext context) {
    final entriesAsync = ref.watch(habitReminderEntriesProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '这里汇总所有未归档习惯页的定期问候。时间、问法和按钮改动会直接保存到对应习惯页；'
          '主设备到点后把问候写进主对话，其他设备只同步显示，不会重复发问。',
          style: TextStyle(
            color: AppTheme.textSecondary,
            fontSize: 13,
            height: 1.6,
          ),
        ),
        const SizedBox(height: AppTheme.space3),
        entriesAsync.when(
          loading: () => const Padding(
            padding: EdgeInsets.symmetric(vertical: AppTheme.space4),
            child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
          ),
          error: (error, stack) => Text(
            '习惯提醒读取失败：${_errorLine(error)}',
            style: TextStyle(color: AppTheme.error, fontSize: 12),
          ),
          data: (entries) => _buildList(entries),
        ),
      ],
    );
  }

  Widget _buildList(List<HabitReminderEntry> entries) {
    if (entries.isEmpty) {
      return Container(
        width: double.infinity,
        padding: const EdgeInsets.all(AppTheme.space4),
        decoration: BoxDecoration(
          color: AppTheme.surface2.withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          border: Border.all(color: AppTheme.surface3),
        ),
        child: Text(
          '还没有未归档的习惯页。先在知识库创建「习惯」页，再回到这里设置定期问候。',
          style: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 12.5,
            height: 1.6,
          ),
        ),
      );
    }

    final enabled = entries.where((e) => e.plan?.enabled == true).length;
    final paused = entries
        .where((e) => e.plan != null && e.plan!.enabled == false)
        .length;
    final unset = entries.where((e) => e.plan == null).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '运行中 $enabled · 已暂停 $paused · 未设置 $unset',
          style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
        ),
        const SizedBox(height: AppTheme.space2),
        for (final entry in entries) ...[
          _buildEntry(entry),
          const SizedBox(height: AppTheme.space2),
        ],
      ],
    );
  }

  Widget _buildEntry(HabitReminderEntry entry) {
    final plan = entry.plan;
    final isBusy = plan != null && _busy.contains(plan.id);
    final status = plan == null
        ? ('未设置', AppTheme.textTertiary)
        : plan.enabled
        ? ('运行中', AppTheme.success)
        : ('已暂停', AppTheme.warning);

    return Container(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space3,
        AppTheme.space3,
        AppTheme.space2,
        AppTheme.space3,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface2.withValues(alpha: 0.45),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Icon(
            plan?.enabled == true
                ? Icons.notifications_active_outlined
                : Icons.notifications_none,
            size: 18,
            color: plan?.enabled == true
                ? AppTheme.accentPrimary
                : AppTheme.textTertiary,
          ),
          const SizedBox(width: AppTheme.space3),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        entry.page.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 13.5,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.textPrimary,
                        ),
                      ),
                    ),
                    const SizedBox(width: AppTheme.space2),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 1,
                      ),
                      decoration: BoxDecoration(
                        color: status.$2.withValues(alpha: 0.12),
                        borderRadius: BorderRadius.circular(
                          AppTheme.radiusFull,
                        ),
                      ),
                      child: Text(
                        status.$1,
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w600,
                          color: status.$2,
                        ),
                      ),
                    ),
                  ],
                ),
                if (plan != null) ...[
                  const SizedBox(height: 3),
                  Text(
                    plan.question,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    _scheduleSummary(plan),
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ] else ...[
                  const SizedBox(height: 3),
                  Text(
                    '尚未设置定期问候',
                    style: TextStyle(
                      fontSize: 11.5,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ],
              ],
            ),
          ),
          if (isBusy)
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 10),
              child: SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            )
          else if (plan != null)
            Switch(
              value: plan.enabled,
              activeThumbColor: AppTheme.accentPrimary,
              onChanged: (value) => _toggle(plan, value),
            ),
          TextButton(
            onPressed: isBusy ? null : () => _edit(entry),
            child: Text(plan == null ? '设置' : '调整'),
          ),
        ],
      ),
    );
  }

  Future<void> _edit(HabitReminderEntry entry) async {
    final changed = await showHabitNudgeDialog(context, page: entry.page);
    if (changed == true && mounted) {
      ref.invalidate(habitReminderEntriesProvider);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('定期问候已更新'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }

  Future<void> _toggle(api.HabitNudgeDto plan, bool enabled) async {
    if (_busy.contains(plan.id)) return;
    setState(() => _busy.add(plan.id));
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.saveHabitNudge(
        api.HabitNudgeInputDto(
          pageSlug: plan.pageSlug,
          question: plan.question,
          doneLabel: plan.doneLabel,
          doneText: plan.doneText,
          missedLabel: plan.missedLabel,
          missedText: plan.missedText,
          weekdays: plan.weekdays,
          localTime: plan.localTime,
          enabled: enabled,
        ),
      );
      if (!mounted) return;
      ref.invalidate(habitReminderEntriesProvider);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(enabled ? '定期问候已恢复' : '定期问候已暂停'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(reportUiError('切换定期问候', error)),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } finally {
      if (mounted) setState(() => _busy.remove(plan.id));
    }
  }

  static String _scheduleSummary(api.HabitNudgeDto plan) {
    final days = _weekdaySummary(plan.weekdays);
    return '$days ${plan.localTime} · ${plan.doneLabel}/${plan.missedLabel}';
  }

  static String _weekdaySummary(int mask) {
    if (mask == 127) return '每天';
    const names = ['一', '二', '三', '四', '五', '六', '日'];
    final days = [
      for (var i = 0; i < 7; i++)
        if ((mask & (1 << i)) != 0) names[i],
    ];
    return '每周 ${days.join('、')}';
  }

  static String _errorLine(Object error) {
    final text = error.toString().replaceFirst('Exception: ', '');
    return text.split('\n').first.trim();
  }
}
