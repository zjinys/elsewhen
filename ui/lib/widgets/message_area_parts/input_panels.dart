part of '../message_area.dart';

//! 输入引导面板与补全面板。

class _InputGuidePanel extends StatelessWidget {
  final ValueChanged<String> onInsert;

  const _InputGuidePanel({required this.onInsert});

  @override
  Widget build(BuildContext context) {
    const items = [
      (Icons.person_outline, '@人名', '标注联系人', '@张伟 '),
      (Icons.tag_outlined, '#主题名', '进入主题', '#付款流程'),
      (Icons.link_outlined, '链接', '导入内容', 'https://'),
      (
        Icons.folder_open_outlined,
        '目录',
        '导入项目或文件',
        '看看 `/path/to/project` 下的项目，导入到知识库',
      ),
      (Icons.check_circle_outline, '确认 / 取消', '处理 AI 草案', '确认'),
    ];
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        border: Border.all(color: AppTheme.surface3),
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final item in items)
            _InputGuideItem(
              icon: item.$1,
              title: item.$2,
              description: item.$3,
              onTap: () => onInsert(item.$4),
            ),
        ],
      ),
    );
  }
}

class _InputCompletionPanel extends StatelessWidget {
  final String marker;
  final List<WikiPageRef> pages;
  final ValueChanged<String> onSelected;

  const _InputCompletionPanel({
    required this.marker,
    required this.pages,
    required this.onSelected,
  });

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerLeft,
      child: GlassPanel(
        elevation: 2,
        borderRadius: AppTheme.radiusMedium,
        padding: EdgeInsets.zero,
        child: Material(
          type: MaterialType.transparency,
          child: Container(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final page in pages)
                Material(
                  color: Colors.transparent,
                  child: InkWell(
                    hoverColor: AppTheme.surface2,
                    borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                    onTap: () => onSelected(page.title),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 8,
                      ),
                      child: Row(
                        children: [
                          Icon(
                            marker == '@'
                                ? Icons.person_outline
                                : Icons.tag_outlined,
                            size: 15,
                            color: AppTheme.accentPrimary,
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              '$marker${page.title}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                color: AppTheme.textPrimary,
                                fontSize: 13,
                                fontWeight: FontWeight.w500,
                              ),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 2,
                            ),
                            decoration: BoxDecoration(
                              color: AppTheme.surface3.withValues(alpha: 0.5),
                              borderRadius: BorderRadius.circular(4),
                            ),
                            child: Text(
                              page.kindLabel,
                              style: TextStyle(
                                fontSize: 10.5,
                                color: AppTheme.textTertiary,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
            ],
          ),
          ),
        ),
      ),
    );
  }
}

class _InputGuideItem extends StatelessWidget {
  final IconData icon;
  final String title;
  final String description;
  final VoidCallback onTap;

  const _InputGuideItem({
    required this.icon,
    required this.title,
    required this.description,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return AppPressable(
      scaleOnPress: 0.96,
      duration: const Duration(milliseconds: 130),
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        decoration: BoxDecoration(
          color: AppTheme.surface1,
          borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          border: Border.all(
            color: AppTheme.surface3.withValues(alpha: 0.8),
            width: 1,
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 16, color: AppTheme.accentPrimary),
            const SizedBox(width: 6),
            Text(
              title,
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 12,
                fontWeight: FontWeight.w500,
              ),
            ),
            const SizedBox(width: 6),
            Text(
              description,
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
          ],
        ),
      ),
    );
  }
}

/// 写入系统剪贴板并给出轻量反馈
Future<void> _copyToClipboard(
  BuildContext context,
  String text,
  String label,
) async {
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text('已复制$label'),
      duration: const Duration(seconds: 1),
      behavior: SnackBarBehavior.floating,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(999)),
      ),
    ),
  );
}

/// 把整段对话格式化为可粘贴文本（角色 + 时间 + 内容）
String _formatConversationForCopy(List<Message> messages) {
  return messages
      .map((m) {
        final role = m.isUser ? '我' : 'AI';
        final time = DateFormat('MM-dd HH:mm').format(m.createdAt);
        return '$role ($time)\n${m.content}';
      })
      .join('\n\n');
}
