/// 后台 AI 活动类型：标题栏指示器按调用方显示不同文案。
enum AiActivity {
  chat('回复中', 'AI 正在生成回复'),
  analysis('分析中', 'AI 正在分析记录'),
  digest('消化中', 'AI 正在消化新记录'),
  insights('整理中', 'AI 正在整理知识库'),
  periodReport('检测中', 'AI 正在检测周期报告');

  const AiActivity(this.label, this.tooltip);

  /// 标题栏胶囊里的短文案。
  final String label;

  /// 悬浮提示里的完整说明。
  final String tooltip;
}
