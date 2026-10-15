/// 个人经验规则库中的一条规则
class Rule {
  final String id;
  final String content;

  /// active（已生效）/ pending（待确认）
  final String status;
  final DateTime createdAt;

  /// 触发场景（'capture'/'conversation'/'daily_review'；空 = 全场景）
  final String triggerScene;

  /// 触发关键词（空 = 不参与事件预筛）
  final List<String> triggerKeywords;

  const Rule({
    required this.id,
    required this.content,
    required this.status,
    required this.createdAt,
    this.triggerScene = '',
    this.triggerKeywords = const [],
  });

  bool get isActive => status == 'active';

  bool get hasTrigger => triggerKeywords.isNotEmpty;
}

/// 规则命中（事件 × 规则的提醒记录视图）。
class RuleHit {
  final String id;
  final String ruleId;
  final String ruleContent;

  /// notified（即时提醒）/ aggregated（每日回顾聚合）
  final String status;
  final DateTime createdAt;

  const RuleHit({
    required this.id,
    required this.ruleId,
    required this.ruleContent,
    required this.status,
    required this.createdAt,
    this.feedback,
  });

  /// （D6 闭环度量）：helpful / not_relevant / 未反馈 null。
  final String? feedback;

  bool get isNotified => status == 'notified';
}
