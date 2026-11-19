import 'package:material_ui/material_ui.dart';

import '../theme/app_theme.dart';

/// 统一的操作反馈 toast：替代散落各处直接 new SnackBar 的写法。
///
/// 定位纪律：右上角（惯例通知位）——离主操作区（底部输入条/按钮）
/// 最远，不盖操作区，也不和手势条打架；移动端窄屏时右上角与居中
/// 近似等效（胶囊几乎占满宽度）。胶囊 + 语义色发光边，深浅模式都可读。
///
/// 视觉跟随全局 snackBarTheme（胶囊 + accent 边），这里补语义色与图标：
/// success = success 色边 + 勾；error = error 色边 + 感叹号；info 走
/// 默认 accent 边。duration 默认 3 秒，错误给 4 秒（caller 可覆盖）。
Future<void> showFeedbackToast(
  BuildContext context, {
  required String message,
  FeedbackKind kind = FeedbackKind.info,
  Duration? duration,
}) async {
  final color = switch (kind) {
    FeedbackKind.success => AppTheme.success,
    FeedbackKind.error => AppTheme.error,
    FeedbackKind.info => AppTheme.accentPrimary,
  };
  final icon = switch (kind) {
    FeedbackKind.success => Icons.check_circle_outline,
    FeedbackKind.error => Icons.error_outline,
    FeedbackKind.info => Icons.info_outline,
  };
  // 顶栏（CustomTitleBar）已吞掉系统状态栏内垫，悬浮层用 Scaffold 的
  // 实际可视顶边即可；桌面为 0，移动端把状态栏高度加上避免被裁。
  final topInset = MediaQuery.paddingOf(context).top;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Row(
        children: [
          Icon(icon, size: 16, color: color),
          const SizedBox(width: AppTheme.space2),
          Expanded(
            child: Text(
              message,
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 13,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
        ],
      ),
      behavior: SnackBarBehavior.floating,
      // 右上角惯例位：贴顶贴右。SnackBar 无 alignment 参数（版本限制），
      // 用大左 margin 把胶囊推到右侧实现；长文案在窄屏下自动占满可用
      // 宽度，不会溢出。
      margin: EdgeInsets.fromLTRB(120, 12 + topInset, 16, 0),
      duration: duration ?? const Duration(seconds: 3),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
        side: BorderSide(color: color.withValues(alpha: 0.55)),
      ),
      backgroundColor: AppTheme.surface2.withValues(alpha: 0.98),
      elevation: 8,
    ),
  );
}

enum FeedbackKind { success, error, info }
