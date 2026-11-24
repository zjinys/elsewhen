import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';
import 'package:url_launcher/url_launcher.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';
import '../providers/scout_provider.dart';
import '../providers/scout_research_provider.dart';
import '../providers/state_holder.dart';
import '../providers/settings_provider.dart';
import '../theme/app_theme.dart';
import '../theme/chat_type_scale.dart';
import '../theme/content_font.dart';
import '../utils/pretty_time.dart';
import 'feedback_toast.dart';
import 'scout_source_identity.dart';
import '../theme/design_tokens.dart';
export 'scout_source_identity.dart';

import 'dart:convert';
import 'dart:async';

import 'package:crypto/crypto.dart';

import '../models/import_fetch.dart';
import '../providers/event_provider.dart';
import '../providers/wiki_provider.dart';
import '../screens/settings_screen.dart';
import 'knowledge_import.dart';

part 'scout_view_parts/scout_navigation.dart';
part 'scout_view_parts/scout_products.dart';
part 'scout_view_parts/scout_research_form.dart';
part 'scout_view_parts/scout_preferences.dart';
part 'scout_view_parts/scout_detail.dart';
part 'scout_view_parts/scout_findings.dart';

/// 情报阅读工作区：工作区切换（总览/研究/动态）、动态列表和站内详情。
/// 原侧栏来源导航的工作区切换与来源筛选已搬进主区：顶部 SegmentedButton
/// 切工作区，来源筛选在动态页右上角的筛选面板里。
class ScoutView extends ConsumerWidget {
  const ScoutView({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workspace = ref.watch(scoutWorkspaceProvider);
    if (workspace == ScoutWorkspace.studies) {
      return _withWorkspaceNav(const ScoutResearchList());
    }
    final settings = ref.watch(scoutSettingsProvider);
    final scale = chatTypeScaleOf(ref.watch(settingsProvider));
    return settings.when(
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (e, _) => _ScoutMessage(
        text: context.errorMessage(context.l10n.scoutConnection, e),
      ),
      data: (s) => s == null
          ? _ScoutNotConfigured(scale: scale)
          : _withWorkspaceNav(
              workspace == ScoutWorkspace.activity
                  ? const ScoutFindingsPane()
                  : const ScoutProductWorkspace(),
            ),
    );
  }

  /// 主区顶部统一的工作区切换行；未配置/加载/错误态不包导航。
  Widget _withWorkspaceNav(Widget child) {
    return Column(
      children: [
        const _ScoutWorkspaceNav(),
        Expanded(child: child),
      ],
    );
  }
}

/// 主区顶部的工作区切换（总览/研究/原始动态）：抽屉不再承载，入口直接
/// 在主区可见。
class _ScoutWorkspaceNav extends ConsumerWidget {
  const _ScoutWorkspaceNav();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workspace = ref.watch(scoutWorkspaceProvider);
    final settings = ref.watch(scoutSettingsProvider).value;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final navigation = SegmentedButton<ScoutWorkspace>(
            style: constraints.maxWidth < 480
                ? SegmentedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 10,
                      vertical: 8,
                    ),
                    textStyle: Theme.of(context).textTheme.labelLarge
                        ?.copyWith(fontSize: 12),
                  )
                : null,
            segments: [
              for (final w in ScoutWorkspace.values)
                ButtonSegment(
                  value: w,
                  icon: Icon(switch (w) {
                    ScoutWorkspace.discover => Icons.explore_outlined,
                    ScoutWorkspace.studies => Icons.science_outlined,
                    ScoutWorkspace.activity => Icons.history,
                  }, size: constraints.maxWidth < 480 ? 16 : 18),
                  label: Text(switch (w) {
                    ScoutWorkspace.discover => context.l10n.scoutDiscover,
                    ScoutWorkspace.studies => context.l10n.scoutStudies,
                    ScoutWorkspace.activity => context.l10n.scoutActivityLog,
                  }, maxLines: 1),
                ),
            ],
            selected: {workspace},
            onSelectionChanged: (selection) =>
                ref.read(scoutWorkspaceProvider.notifier).set(selection.first),
          );
          final sync = Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (workspace == ScoutWorkspace.activity)
                IconButton(
                  tooltip: context.l10n.scoutSources,
                  icon: const Icon(Icons.filter_list),
                  onPressed: () => showModalBottomSheet<void>(
                    context: context,
                    isScrollControlled: true,
                    builder: (_) => SizedBox(
                      height: MediaQuery.sizeOf(context).height * .75,
                      child: ScoutSourceNavigation(
                        onSelected: () => Navigator.of(context).pop(),
                      ),
                    ),
                  ),
                ),
              if (settings?.lastPolledAt != null)
                Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: _ScoutLastPolled(timestamp: settings!.lastPolledAt!),
                ),
              _SyncNowButton(
                scale: chatTypeScaleOf(ref.watch(settingsProvider)),
              ),
            ],
          );
          if (constraints.maxWidth >= 760) {
            return Row(
              children: [
                Expanded(
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: navigation,
                  ),
                ),
                Container(
                  margin: const EdgeInsets.only(left: 32),
                  padding: const EdgeInsets.only(left: 24),
                  decoration: BoxDecoration(
                    border: Border(
                      left: BorderSide(color: AppTheme.boundaryColor),
                    ),
                  ),
                  child: sync,
                ),
              ],
            );
          }
          return Wrap(
            spacing: 32,
            runSpacing: 12,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [navigation, sync],
          );
        },
      ),
    );
  }
}

/// Relative text ages while the workspace remains open; exact time stays available.
class _ScoutLastPolled extends StatefulWidget {
  const _ScoutLastPolled({required this.timestamp});
  final String timestamp;

  @override
  State<_ScoutLastPolled> createState() => _ScoutLastPolledState();
}

class _ScoutLastPolledState extends State<_ScoutLastPolled> {
  late final Timer _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(minutes: 1), (_) => setState(() {}));
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Tooltip(
    message: _exactTime(widget.timestamp),
    child: Text(
      context.l10n.scoutLastPolled(
        prettyTimestamp(widget.timestamp, strings: context.l10n) ?? '',
      ),
      style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
    ),
  );
}

/// 未配置空态：引导去设置页（设置→情报连接）。
class _ScoutNotConfigured extends StatelessWidget {
  final ChatTypeScale scale;
  const _ScoutNotConfigured({required this.scale});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.travel_explore_outlined,
            size: 40,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: 12),
          Text(
            context.l10n.scoutNotConfigured,
            style: TextStyle(
              fontSize: scale.bubble,
              color: AppTheme.textSecondary,
            ),
          ),
          const SizedBox(height: 8),
          TextButton(
            onPressed: () => Navigator.of(
              context,
            ).push(MaterialPageRoute(builder: (_) => const SettingsScreen())),
            child: Text(
              context.l10n.scoutGoConfigure,
              style: TextStyle(
                fontSize: scale.bubbleSecondary,
                color: AppTheme.textTertiary,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// 通用信息文案。
class _ScoutMessage extends StatelessWidget {
  final String text;
  const _ScoutMessage({required this.text});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        text,
        style: TextStyle(fontSize: 14, color: AppTheme.textSecondary),
      ),
    );
  }
}

/// 「立即同步」：手动触发一轮拉取，成功/失败都有 SnackBar 反馈。
class _SyncNowButton extends ConsumerStatefulWidget {
  final ChatTypeScale scale;
  const _SyncNowButton({required this.scale});

  @override
  ConsumerState<_SyncNowButton> createState() => _SyncNowButtonState();
}

class _SyncNowButtonState extends ConsumerState<_SyncNowButton> {
  bool _running = false;

  @override
  Widget build(BuildContext context) {
    return FilledButton.tonalIcon(
      onPressed: _running ? null : _run,
      icon: _running
          ? const SizedBox(
              width: 14,
              height: 14,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : const Icon(Icons.sync, size: 16),
      label: Text(context.l10n.scoutSyncNow),
    );
  }

  Future<void> _run() async {
    setState(() => _running = true);
    try {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final result = await bridge.pollScoutNow();
      if (!mounted) return;
      ref.invalidate(scoutSettingsProvider);
      showFeedbackToast(
        context,
        message: context.l10n.scoutSyncOk(result.fetched, result.newCount),
        kind: FeedbackKind.success,
      );
    } catch (e) {
      if (!mounted) return;
      showFeedbackToast(
        context,
        message: context.errorMessage(context.l10n.scoutSyncNow, e),
        kind: FeedbackKind.error,
        duration: const Duration(seconds: 4),
      );
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }
}

/// kind 徽标样式（new/changed/gone 三色）。
Color _kindColor(String kind) => switch (kind) {
  'new' => const Color(0xFF2E7D32),
  'changed' => const Color(0xFF1565C0),
  'gone' => const Color(0xFF9E9E9E),
  _ => AppTheme.textTertiary,
};

/// kind 标签文案。
String _kindLabel(BuildContext context, String kind) => switch (kind) {
  'new' => context.l10n.scoutKindNew,
  'changed' => context.l10n.scoutKindChanged,
  'gone' => context.l10n.scoutKindGone,
  _ => kind,
};

/// 服务端 ISO 时间戳 → 本地相对展示（今天/昨天/M-d）。
String _formatTime(BuildContext context, String iso) {
  final t = DateTime.tryParse(iso)?.toLocal();
  if (t == null) return iso;
  final now = DateTime.now();
  final diff = now.difference(t);
  if (diff.isNegative) return DateFormat('M-d HH:mm').format(t);
  if (diff.inMinutes < 1) return context.l10n.justNow;
  if (diff.inHours < 24 && t.day == now.day) {
    return DateFormat('HH:mm').format(t);
  }
  if (diff.inDays < 2) return context.l10n.scoutYesterday;
  return DateFormat('M-d HH:mm').format(t);
}
