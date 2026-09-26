import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/settings.dart';
import '../providers/settings_provider.dart';
import '../theme/app_theme.dart';
import 'font_picker_dialog.dart';

/// 知识页「AA」浮层：编辑器（内容区）阅读参数覆盖层。
///
/// 两层覆盖模型（见 docs/notes/proposed/product/2026-09-23-editor-reading-settings-layer.md）：
/// 这里编辑的是覆盖层三个参数（字体/字号/行距），每项可空——
/// **「跟随全局」= 空**，回到继承（全局设置或 vendor 默认）。改动即生效并持久化。
void showWikiReadingSettings(BuildContext context) {
  showDialog<void>(
    context: context,
    builder: (_) => const WikiReadingSettingsDialog(),
  );
}

class WikiReadingSettingsDialog extends ConsumerWidget {
  const WikiReadingSettingsDialog({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(settingsProvider);
    final notifier = ref.read(settingsProvider.notifier);
    return AlertDialog(
      title: Row(
        children: [
          Icon(Icons.format_size, size: 20, color: AppTheme.accentPrimary),
          const SizedBox(width: 8),
          const Text('正文阅读设置'),
          const Spacer(),
          IconButton(
            tooltip: '关闭',
            visualDensity: VisualDensity.compact,
            onPressed: () => Navigator.of(context).pop(),
            icon: const Icon(Icons.close, size: 18),
          ),
        ],
      ),
      contentPadding: const EdgeInsets.fromLTRB(24, 8, 24, 8),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildFontField(context, settings, notifier),
            const Divider(height: 24),
            _buildFontSizeSlider(settings, notifier),
            const Divider(height: 24),
            _buildLineHeightSlider(settings, notifier),
            const SizedBox(height: 8),
            Text(
              '三项独立生效；「跟随全局」恢复继承，两个滑块和字体互不影响。',
              style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }

  /// 字体覆盖：当前值 + 点按打开 FontPicker + 「跟随全局」重置
  Widget _buildFontField(
    BuildContext context,
    AppSettings settings,
    SettingsNotifier notifier,
  ) {
    final override = settings.editorFontName;
    final following = override == null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              '字体',
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: AppTheme.textSecondary,
              ),
            ),
            const Spacer(),
            Text(
              following ? '继承全局' : '覆盖全局',
              style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: OutlinedButton(
                onPressed: () => _showFontPicker(context, notifier, override),
                style: OutlinedButton.styleFrom(
                  alignment: Alignment.centerLeft,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 12,
                  ),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: following
                          ? const Text('跟随全局', overflow: TextOverflow.ellipsis)
                          : FontNameLabel(
                              stored: override,
                              fallbackStyle: null,
                            ),
                    ),
                    Icon(
                      Icons.font_download_outlined,
                      size: 18,
                      color: AppTheme.textSecondary,
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(width: 8),
            _FollowGlobalChip(
              key: const Key('reading-font-reset'),
              label: '跟随全局',
              selected: following,
              onPressed: following
                  ? null
                  : () {
                      notifier
                        ..updateEditorFont(null)
                        ..saveTheme();
                    },
            ),
          ],
        ),
      ],
    );
  }

  /// 字号覆盖：滑块 12–24（拖拽实时预览、松手持久化，与设置页观感一致）
  Widget _buildFontSizeSlider(AppSettings settings, SettingsNotifier notifier) {
    final following = settings.editorFontSize == null;
    final size = settings.contentFontSize;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              '字号',
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: AppTheme.textSecondary,
              ),
            ),
            const Spacer(),
            Text(
              '${size.toStringAsFixed(size == size.roundToDouble() ? 0 : 1)} pt',
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textTertiary,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
            const SizedBox(width: 8),
            _FollowGlobalChip(
              key: const Key('reading-size-reset'),
              label: '跟随全局',
              selected: following,
              onPressed: following
                  ? null
                  : () {
                      notifier
                        ..updateEditorFontSize(null)
                        ..saveTheme();
                    },
            ),
          ],
        ),
        Slider(
          value: size,
          min: AppFonts.minFontSize,
          max: AppFonts.maxFontSize,
          divisions: (AppFonts.maxFontSize - AppFonts.minFontSize).round(),
          label: size.toStringAsFixed(0),
          // 拖动即覆盖全局（进入覆盖态）；松手持久化
          onChanged: (v) => notifier.updateEditorFontSize(v),
          onChangeEnd: (v) => notifier
            ..updateEditorFontSize(v)
            ..saveTheme(),
        ),
      ],
    );
  }

  /// 行距覆盖：滑块 1.0–2.5（步进 0.1）
  Widget _buildLineHeightSlider(
    AppSettings settings,
    SettingsNotifier notifier,
  ) {
    final following = settings.editorLineHeight == null;
    final height = settings.contentLineHeight;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              '行距',
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: AppTheme.textSecondary,
              ),
            ),
            const Spacer(),
            Text(
              '${height.toStringAsFixed(1)}×',
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textTertiary,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
            const SizedBox(width: 8),
            _FollowGlobalChip(
              key: const Key('reading-height-reset'),
              label: '跟随全局',
              selected: following,
              onPressed: following
                  ? null
                  : () {
                      notifier
                        ..updateEditorLineHeight(null)
                        ..saveTheme();
                    },
            ),
          ],
        ),
        Slider(
          value: height,
          min: AppFonts.minLineHeight,
          max: AppFonts.maxLineHeight,
          divisions: ((AppFonts.maxLineHeight - AppFonts.minLineHeight) * 10)
              .round(),
          label: height.toStringAsFixed(1),
          onChanged: (v) => notifier.updateEditorLineHeight(v),
          onChangeEnd: (v) => notifier
            ..updateEditorLineHeight(v)
            ..saveTheme(),
        ),
      ],
    );
  }

  /// 打开字体选择对话框（同设置页：picker 内部「Select」自关闭，onFontChanged
  /// 只更新状态、不重复 pop）。
  Future<void> _showFontPicker(
    BuildContext context,
    SettingsNotifier notifier,
    String? current,
  ) async {
    final picked = await showFontPickerDialog(
      context,
      current: current,
      includeFollowGlobal: true,
      title: '选择正文阅读字体',
    );
    if (picked == null || !context.mounted) return;
    // 「跟随全局」映射回 null（清除覆盖），其余先预热再落库
    await notifier.setEditorFont(picked == kFontFollowGlobal ? null : picked);
    await notifier.saveTheme();
  }
}

/// 「跟随全局」chip：高亮 = 当前参数为空（继承全局）；点按清除覆盖。
class _FollowGlobalChip extends StatelessWidget {
  const _FollowGlobalChip({
    super.key,
    required this.label,
    required this.selected,
    required this.onPressed,
  });

  final String label;
  final bool selected;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return ActionChip(
      label: Text(label),
      labelStyle: TextStyle(
        fontSize: 12,
        color: selected ? AppTheme.accentPrimary : AppTheme.textSecondary,
      ),
      visualDensity: VisualDensity.compact,
      backgroundColor: selected ? AppTheme.surface3 : AppTheme.surface2,
      side: BorderSide(
        color: selected ? AppTheme.accentPrimary : AppTheme.surface3,
      ),
      onPressed: onPressed,
    );
  }
}
