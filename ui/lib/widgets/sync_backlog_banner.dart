import 'package:flutter/material.dart';

import '../sync/sync_backlog_notice.dart';
import '../sync/sync_setup_controller.dart';
import '../theme/app_theme.dart';

/// 主界面的全局同步状态位（roadmap C8 / 缺陷 D4）。
///
/// 只在「真的有积压」时出现 —— 常驻状态条会变成背景噪音，用户很快就学会无视它。
/// 点一下直达同步页（解锁按钮就在那里），避免用户为了解开同步去翻设置。
class SyncBacklogBanner extends StatefulWidget {
  const SyncBacklogBanner({
    super.key,
    required this.controller,
    required this.onOpenSync,
  });

  final SyncSetupController? controller;
  final VoidCallback onOpenSync;

  @override
  State<SyncBacklogBanner> createState() => _SyncBacklogBannerState();
}

class _SyncBacklogBannerState extends State<SyncBacklogBanner> {
  @override
  void initState() {
    super.initState();
    // 状态位是「现在」的投影：挂上时读一次，避免用启动时的旧快照骗人。
    widget.controller?.refresh();
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    if (controller == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final notice = syncBacklogNotice(controller.state);
        if (notice == null) return const SizedBox.shrink();
        final color = notice.actionable
            ? AppTheme.warning
            : AppTheme.textTertiary;
        return Padding(
          padding: const EdgeInsets.fromLTRB(
            AppTheme.space4,
            AppTheme.space2,
            AppTheme.space4,
            0,
          ),
          child: Material(
            color: color.withValues(alpha: 0.12),
            borderRadius: BorderRadius.circular(10),
            child: InkWell(
              borderRadius: BorderRadius.circular(10),
              onTap: widget.onOpenSync,
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                child: Row(
                  children: [
                    Icon(
                      notice.actionable
                          ? Icons.lock_outline
                          : Icons.sync_problem,
                      size: 16,
                      color: color,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        notice.message,
                        style: TextStyle(fontSize: 12, color: color),
                      ),
                    ),
                    if (notice.actionable)
                      Text(
                        '${notice.actionLabel} ›',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: color,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
