/// 个人经验规则库中的一条规则
class Rule {
  final String id;
  final String content;

  /// active（已生效）/ pending（待确认）
  final String status;
  final DateTime createdAt;

  const Rule({
    required this.id,
    required this.content,
    required this.status,
    required this.createdAt,
  });

  bool get isActive => status == 'active';
}