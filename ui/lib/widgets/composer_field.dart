import 'package:material_ui/material_ui.dart';

import '../theme/app_theme.dart';
import '../theme/content_font.dart';
import 'app_pressable.dart';

/// 统一输入条（胶囊形 composer）：左侧语境图标、中间输入框、右侧发送键。
///
/// 三处对话入口（主对话、知识页 AI 对话、目标讨论）共用同一个实现，
/// 不再各自拼 `Row(TextField + 外侧 IconButton.filled)`——那种拼法有下列
/// 毛病，也正是这个组件要解决的：
///
/// * 发送键在胶囊**外面**，被 12px 深的容器背景隔开，看起来像另一块组件；
/// * 输入框与按钮各自一套边框/圆角，聚焦时两圈描边互相打架；
/// * 左右留白不对称（左边 contentPadding 16、右边 space2 + 按钮 40）；
/// * 多行时 `InputDecoration.suffixIcon` 是垂直居中的，发送键会飘到胶囊顶部，
///   离最后一行文字三行远。
///
/// 这里用一行 `Row(crossAxisAlignment: end)`：输入框多行增高时，图标与发送键
/// 始终贴底，与最后一行文字同一视觉带。
class ComposerField extends StatefulWidget {
  /// 输入框圆角：单行时等价胶囊（半径 > 高/2 被夹成跑道），多行时退成圆角矩形。
  static const double radius = 24;

  const ComposerField({
    super.key,
    required this.controller,
    required this.focusNode,
    required this.hintText,
    required this.onSubmit,
    this.leading,
    this.enabled = true,
    this.canSend = true,
    this.spinning = false,
    this.sendTooltip,
    this.hintStyle,
    this.textStyle,
    this.fieldKey,
    this.maxLines = 4,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final String hintText;

  /// 回车 / 点击发送键时调用。空文本由调用方自行拦截。
  final VoidCallback onSubmit;

  /// 胶囊左端的语境图标（「＋」菜单 / 关闭提示 / 无）。
  final Widget? leading;

  final bool enabled;

  /// 发送键是否可点（生成中、未就绪时为 false）。与 [spinning] 分开：
  /// 「有输入但不可发」和「正在发送、转圈」是两种状态。
  final bool canSend;
  final bool spinning;
  final String? sendTooltip;

  final TextStyle? hintStyle;
  final TextStyle? textStyle;
  final Key? fieldKey;
  final int maxLines;

  @override
  State<ComposerField> createState() => _ComposerFieldState();
}

class _ComposerFieldState extends State<ComposerField> {
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    final contentFontStyle = ContentFont.styleOf(context);
    final defaultStyle = (contentFontStyle ?? const TextStyle()).copyWith(
      color: AppTheme.textPrimary,
      fontSize: 14,
      height: 1.35,
    );
    final effectiveTextStyle = defaultStyle.merge(widget.textStyle);
    final effectiveHintStyle = defaultStyle
        .copyWith(color: AppTheme.textTertiary)
        .merge(widget.hintStyle);

    return Focus(
      onFocusChange: (focused) => setState(() => _focused = focused),
      child: AnimatedContainer(
        // 胶囊外框：测试与视觉核对都按这个 rect 对齐两端留白。
        key: const ValueKey('composer-surface'),
        duration: const Duration(milliseconds: 120),
        curve: Curves.easeOut,
        padding: EdgeInsetsDirectional.only(
          start: widget.leading == null ? 16 : 6,
          end: 6,
        ),
        decoration: BoxDecoration(
          // 对话主场的锚点：常态比周围面板亮一档（surface2），聚焦时再抬升
          // 一档到 surface3，配合主题强调色聚焦环把视线钉在输入区。
          color: _focused ? AppTheme.surface3 : AppTheme.surface2,
          borderRadius: BorderRadius.circular(ComposerField.radius),
          border: Border.all(
            color: _focused ? AppTheme.accentPrimary : AppTheme.surface3,
            width: _focused ? 1.5 : 1,
          ),
          boxShadow: _focused
              ? [
                  // 聚焦环：与描边同源的外扩淡色光晕。
                  BoxShadow(
                    color: AppTheme.accentPrimary.withValues(alpha: 0.18),
                    blurRadius: 6,
                    spreadRadius: 1.5,
                  ),
                ]
              : [
                  BoxShadow(
                    color: Colors.black.withValues(alpha: 0.10),
                    blurRadius: 8,
                    offset: const Offset(0, 2),
                  ),
                ],
        ),
        child: ValueListenableBuilder<TextEditingValue>(
          valueListenable: widget.controller,
          builder: (context, value, _) {
            final isMultiLine = value.text.contains('\n');
            return Row(
              crossAxisAlignment: isMultiLine
                  ? CrossAxisAlignment.end
                  : CrossAxisAlignment.center,
              children: [
                // 48 = 输入框的基准高度。左右两个图标位与它同高同心，
                // 视觉上自然内嵌在输入框两端，保持像素级上下垂直对称。
                if (widget.leading != null)
                  SizedBox(
                    width: 44,
                    height: 48,
                    child: Center(child: widget.leading),
                  ),
                Expanded(
                  child: TextField(
                    key: widget.fieldKey,
                    controller: widget.controller,
                    focusNode: widget.focusNode,
                    maxLines: widget.maxLines,
                    minLines: 1,
                    enabled: widget.enabled,
                    cursorHeight: 18,
                    textAlignVertical: TextAlignVertical.center,
                    style: effectiveTextStyle,
                    decoration: InputDecoration(
                      hintText: widget.hintText,
                      hintStyle: effectiveHintStyle,
                      border: InputBorder.none,
                      contentPadding: const EdgeInsetsDirectional.only(
                        start: 2,
                        end: 6,
                        top: 14,
                        bottom: 14,
                      ),
                      isDense: true,
                    ),
                    onSubmitted: (_) => widget.onSubmit(),
                  ),
                ),
                ComposerSendButton(
                  onPressed: widget.canSend ? widget.onSubmit : null,
                  dimmed: value.text.trim().isEmpty,
                  spinning: widget.spinning,
                  tooltip: widget.sendTooltip,
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

/// 输入条内的发送键：34 的圆形强调色按钮，空文本时退成弱化的表面色。
///
/// 不用 `IconButton.filled`：M3 的 IconButton 自带 40/48 的方形窗格 + 默认
/// padding，实测即使传 fixedSize 也会画出 44 的圆，把胶囊右端顶满。这里手搭
/// 48 的触控区包 34 的圆。按下反馈交给 [AppPressable] 的缩放，不用 InkWell
/// ——否则水波直径取的是 48 的外框，圆外的四角也会出现一圈涟漪。
class ComposerSendButton extends StatelessWidget {
  const ComposerSendButton({
    super.key,
    required this.onPressed,
    required this.spinning,
    this.dimmed = false,
    this.tooltip,
  });

  final VoidCallback? onPressed;
  final bool spinning;
  final bool dimmed;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    // 空文本（dimmed）保持透明内嵌在胶囊内部，不画突兀的深色圆形底；
    // 有文字时点亮为醒目的品牌强调色实心圆，发送引导极度清晰。
    final bg = !enabled || dimmed
        ? Colors.transparent
        : AppTheme.accentPrimary;
    // 转圈时把前景提亮一档：禁用态的中性底上，textTertiary 的弧线几乎看不见。
    final fg = !enabled || dimmed
        ? (spinning ? AppTheme.textSecondary : AppTheme.textTertiary)
        : AppTheme.surface0;
    final button = AppPressable(
      scaleOnPress: 0.92,
      duration: const Duration(milliseconds: 120),
      enabled: enabled,
      onTap: onPressed,
      child: Semantics(
        button: true,
        enabled: enabled,
        label: tooltip,
        // 44 与左侧语境图标位同宽同高，视觉留白两端严格对称。
        child: SizedBox(
          width: 44,
          height: 48,
          child: Center(
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 140),
              width: 32,
              height: 32,
              decoration: BoxDecoration(color: bg, shape: BoxShape.circle),
              child: Center(
                child: spinning
                    ? SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: fg,
                          strokeCap: StrokeCap.round,
                        ),
                      )
                    : Icon(Icons.arrow_upward, size: 18, color: fg),
              ),
            ),
          ),
        ),
      ),
    );
    return tooltip == null ? button : Tooltip(message: tooltip, child: button);
  }
}