import '../models/settings.dart';

/// 对话流字号阶梯。
///
/// 以「外观 → 正文字号」（[AppSettings.contentFontSize]，桌面 AI 正文原 15）
/// 为基准，按原有各元素的相对差动态推算对话流里所有文字的字号：
/// 正文字号调整时，气泡、日期线、习惯问候、洞察卡、输入框整体跟随，
/// 不再各自写死。字号一律经 [AppSettings.clampFontSize] 的同一档位约束
/// （contentFontSize 已 clamp，这里只做 ±5 以内的相对偏移，不会越界）。
class ChatTypeScale {
  const ChatTypeScale(this.base);

  /// 基准：AI 消息正文字号（原桌面 15）。
  final double base;

  /// AI 消息正文（原 15）。
  double get ai => base;

  /// 用户消息正文，比 AI 小一号维持层次差（原 14）。
  double get user => base - 1;

  /// 习惯问候 / 洞察气泡正文（基准 15）。
  double get bubble => base;

  /// 洞察气泡的观察段次级文本（基准 13）。
  double get bubbleSecondary => base - 2;

  /// 对话输入框文字（原 14）。
  double get input => base - 1;

  /// 日期分隔线（原 12）。
  double get divider => base - 3;

  /// 角色名 / 时间戳 / 徽标行（原 11）。
  double get meta => base - 4;

  /// 建议 chips / 生成中指示（原 13）。
  double get chip => base - 2;

  /// 行内按钮与错误提示文案（原 12）。
  double get button => base - 3;

  /// 副面板气泡（知识页 AI 对话 / 目标讨论 / 导入预览，原 13）：
  /// 整体比主对话流小两号，保持次要对话的层次差。
  double get panel => base - 2;
}

/// 从设置求值对话流字号阶梯（Riverpod select 友好：值变化才重建）。
ChatTypeScale chatTypeScaleOf(AppSettings s) =>
    ChatTypeScale(s.contentFontSize);
