import 'package:flutter/material.dart';

/// 三类同步冲突审阅区（Provider / 知识库 / 待办目标）共用的外壳。
///
/// 收敛三份重复的标题/解释/空态/读取中/错误排版：
/// - 首屏安静：标题行只带图标 + 名称 + 状态徽标；审阅约束说明收进
///   标题旁的 ⓘ tooltip，仅在有待办内容时以横幅展开（操作相关时才读）；
/// - 状态徽标：读取中 / 读取失败 / 「无需处理」/「N 条待审阅」；
/// - 空态保留文案（供屏幕阅读与测试断言），弱化为次要色；
/// - 错误保留 liveRegion 语义，绝不静默清空列表。
/// 冲突卡本体仍由各区自绘（DTO 不同）。
class SyncConflictSection extends StatelessWidget {
  const SyncConflictSection({
    super.key,
    required this.icon,
    required this.title,
    required this.explainer,
    required this.loaded,
    required this.busy,
    required this.error,
    required this.cards,
    required this.emptyText,
  });

  final IconData icon;
  final String title;
  final String explainer;
  final bool loaded;
  final bool busy;
  final String? error;
  final List<Widget> cards;
  final String emptyText;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final reviewing = cards.isNotEmpty;
    final strong = error != null || reviewing;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 24),
        Row(
          children: [
            Icon(
              icon,
              size: 18,
              color: strong && error != null
                  ? theme.colorScheme.error
                  : strong
                  ? theme.colorScheme.primary
                  : theme.colorScheme.outline,
            ),
            const SizedBox(width: 8),
            Text(title, style: theme.textTheme.titleSmall),
            const SizedBox(width: 4),
            Tooltip(
              triggerMode: TooltipTriggerMode.tap,
              showDuration: const Duration(seconds: 6),
              message: explainer,
              child: Padding(
                padding: const EdgeInsets.all(4),
                child: Icon(
                  Icons.info_outline,
                  size: 14,
                  color: theme.colorScheme.outline,
                ),
              ),
            ),
            const Spacer(),
            _StatusBadge(
              busy: busy && !loaded,
              reviewing: reviewing,
              count: cards.length,
              failed: error != null,
            ),
          ],
        ),
        if (error != null) ...[
          const SizedBox(height: 8),
          Semantics(
            liveRegion: true,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(Icons.error_outline, size: 14, color: theme.colorScheme.error),
                const SizedBox(width: 4),
                Expanded(
                  child: Text(
                    error!,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
        const SizedBox(height: 8),
        if (busy && !loaded)
          const Row(
            children: [
              SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
              SizedBox(width: 8),
              Text('正在读取冲突…'),
            ],
          )
        else ...[
          if (reviewing) ...[
            Text(
              explainer,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 8),
          ],
          ...cards,
          if (loaded && !reviewing)
            Row(
              children: [
                Icon(
                  Icons.check_circle_outline,
                  size: 14,
                  color: theme.colorScheme.outline,
                ),
                const SizedBox(width: 4),
                Expanded(
                  child: Text(
                    emptyText,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
            ),
        ],
      ],
    );
  }
}

/// 标题行右端的状态徽标：待审 = 主题色实心小丸；清晰态 = 弱化 ✓。
class _StatusBadge extends StatelessWidget {
  const _StatusBadge({
    required this.busy,
    required this.reviewing,
    required this.count,
    required this.failed,
  });

  final bool busy;
  final bool reviewing;
  final int count;
  final bool failed;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    if (busy) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(strokeWidth: 1.6),
          ),
          const SizedBox(width: 6),
          Text(
            '正在读取…',
            style: theme.textTheme.labelSmall?.copyWith(
              color: colorScheme.outline,
            ),
          ),
        ],
      );
    }
    final pillStyle = theme.textTheme.labelSmall?.copyWith(
      fontWeight: FontWeight.w600,
    );
    if (failed) {
      return _pill(
        text: '读取失败',
        fg: colorScheme.error,
        bg: colorScheme.error.withValues(alpha: 0.12),
        style: pillStyle!,
      );
    }
    if (!reviewing) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check_circle_outline, size: 14, color: colorScheme.outline),
          const SizedBox(width: 4),
          Text(
            '无需处理',
            style: theme.textTheme.labelSmall?.copyWith(
              color: colorScheme.outline,
            ),
          ),
        ],
      );
    }
    return _pill(
      text: '$count 条待审阅',
      fg: colorScheme.onPrimaryContainer,
      bg: colorScheme.primaryContainer.withValues(alpha: 0.8),
      style: pillStyle!,
    );
  }

  Widget _pill({
    required String text,
    required Color fg,
    required Color bg,
    required TextStyle style,
  }) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
    decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(999)),
    child: Text(text, style: style.copyWith(color: fg)),
  );
}

/// 冲突卡内的两侧对比排版：宽空间左右并列（竖分隔线），窄空间上下堆叠。
class SyncConflictSides extends StatelessWidget {
  const SyncConflictSides({super.key, required this.children})
    : assert(children.length == 2);

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth < 520) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              children[0],
              const Divider(height: 24),
              children[1],
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(child: children[0]),
            const VerticalDivider(width: 24),
            Expanded(child: children[1]),
          ],
        );
      },
    );
  }
}
