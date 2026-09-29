import '../bridge/api.dart' as api;

/// 一条人物关系：`from`（一般是人物页）↔ `to`（事情/项目页等），带关系类型。
/// 由 AI 在对话中识别草拟、用户确认后保存；也可手动补充。
class Relation {
  final String id;
  final String fromSlug;
  final String fromKind;
  final String toSlug;
  final String toKind;
  final String relation;
  final String? note;
  final int confidence;
  final DateTime createdAt;
  final DateTime lastSeenAt;

  const Relation({
    required this.id,
    required this.fromSlug,
    required this.fromKind,
    required this.toSlug,
    required this.toKind,
    required this.relation,
    this.note,
    required this.confidence,
    required this.createdAt,
    required this.lastSeenAt,
  });

  factory Relation.fromDto(api.RelationDto dto) {
    return Relation(
      id: dto.id,
      fromSlug: dto.fromSlug,
      fromKind: dto.fromKind,
      toSlug: dto.toSlug,
      toKind: dto.toKind,
      relation: dto.relation,
      note: dto.note,
      confidence: dto.confidence.toInt(),
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      lastSeenAt: DateTime.parse(dto.lastSeenAt).toLocal(),
    );
  }
}
