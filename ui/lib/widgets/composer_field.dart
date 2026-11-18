import 'package:material_ui/material_ui.dart';

import '../theme/app_theme.dart';
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
    return Focus(
      onFocusChange: (focused) => setState(() => _focused = focused),
      child: AnimatedContainer(
        // 胶囊外框：测试与视觉核对都按这个 rect 对齐两端留白。
        key: const ValueKey('composer-surface'),
        duration: const Duration(milliseconds: 120),
        curve: Curves.easeOut,
        padding: EdgeInsetsDirectional.only(
          start: widget.leading == null ? AppTheme.space3 : 6,
          end: 6,
        ),
        decoration: BoxDecoration(
          color: AppTheme.surface2,
          borderRadius: BorderRadius.circular(ComposerField.radius),
          border: Border.all(
            color: _focused ? AppTheme.accentPrimary : AppTheme.surface3,
            width: _focused ? 1.5 : 1,
          ),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            // 48 = 输入框的最低高度（InputDecorator 的 kMinInteractiveDimension，
            // 即使 isDense 也压不下去）。左右两个图标位与它同高，胶囊内三块
            // 内容才真正同高同底，不会出现「图标比输入框矮 2px」的错位。
            if (widget.leading != null)
              SizedBox(
                width: 48,
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
                textAlignVertical: TextAlignVertical.center,
                style: widget.textStyle,
                decoration: InputDecoration(
                  hintText: widget.hintText,
                  hintStyle: widget.hintStyle,
                  // 胶囊自身的填充与描边已经画好了，输入框退成透明的纯文本层：
                  // 两套圆角叠在一起时，聚焦那 1.5px 描边会与胶囊外圈错开。
                  border: InputBorder.none,
                  // 垂直各 10：单行输入框即 48 高（=InputDecorator 的
                  // kMinInteractiveDimension 下限，也是两侧图标位的高度），
                  // 胶囊内三块内容因此同高同底。水平只留 8，文字与图标之间
                  // 才不各隔一片。
                  contentPadding: const EdgeInsetsDirectional.only(
                    // 水平只留 4：图标位本身已有 14 的内边距，再叠 8 会让
                    // 「＋」与首个字之间空出近 30px，像两个字块各站一边。
                    start: AppTheme.space1,
                    end: AppTheme.space2,
                    top: 10,
                    bottom: 10,
                  ),
                  isDense: true,
                ),
                onSubmitted: (_) => widget.onSubmit(),
              ),
            ),
            const SizedBox(width: AppTheme.space1),
            ValueListenableBuilder<TextEditingValue>(
              valueListenable: widget.controller,
              builder: (context, value, _) => ComposerSendButton(
                // 空文本只降亮度、不禁用：`onSubmit` 自己会拦掉空文本，
                // 而禁用一个只靠 controller 监听才刷新的按钮，会让「输入完
                // 立刻回车/点击」因为还没重建的那一帧而毫无反应。
                onPressed: widget.canSend ? widget.onSubmit : null,
                dimmed: value.text.trim().isEmpty,
                spinning: widget.spinning,
                tooltip: widget.sendTooltip,
              ),
            ),
          ],
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
    // 空文本（dimmed）走中性表面色而不是淡化的强调色：强调色压暗后
    // 会发闷、发脏（#7A5C3D 在深底上像一块泥），中性灰反而干净，
    // 也把「有字才亮」的信息交给唯一的强调色节点。
    final bg = !enabled || dimmed
        ? AppTheme.surface3.withValues(alpha: enabled ? 1 : 0.7)
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
        // 48 与左侧语境图标位同宽同高，胶囊两端的触控区与视觉留白都对称；
        // 视觉上的圆仍是里面的 34。
        child: SizedBox(
          width: 48,
          height: 48,
          child: Center(
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 140),
              width: 34,
              height: 34,
              decoration: BoxDecoration(color: bg, shape: BoxShape.circle),
              child: Center(
                child: spinning
                    ? SizedBox(
                        width: 18,
                        height: 18,
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