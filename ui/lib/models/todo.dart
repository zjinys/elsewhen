import '../bridge/generated.dart/api.dart' as api;

/// 待办状态
enum TodoStatus {
  open,
  done,
  archived;

  static TodoStatus fromString(String s) {
    switch (s) {
      case 'done':
        return TodoStatus.done;
      case 'archived':
        return TodoStatus.archived;
      default:
        return TodoStatus.open;
    }
  }

  String get wire => name;

  /// 中文展示名
  String get label {
    switch (this) {
      case TodoStatus.open:
        return '进行中';
      case TodoStatus.done:
        return '已完成';
      case TodoStatus.archived:
        return '已归档';
    }
  }
}

/// 个人待办（AI 提议确认后创建，或手动创建）
class Todo {
  final String id;
  final String title;
  final TodoStatus status;
  final String priority; // high / normal / low
  final String? dueAt;
  final String? relatedEventId;
  final String? relatedWikiSlug;
  final String? note;
  final DateTime createdAt;
  final DateTime updatedAt;

  const Todo({
    required this.id,
    required this.title,
    required this.status,
    required this.priority,
    this.dueAt,
    this.relatedEventId,
    this.relatedWikiSlug,
    this.note,
    required this.createdAt,
    required this.updatedAt,
  });

  factory Todo.fromDto(api.TodoDto dto) {
    return Todo(
      id: dto.id,
      title: dto.title,
      status: TodoStatus.fromString(dto.status),
      priority: dto.priority,
      dueAt: dto.dueAt,
      relatedEventId: dto.relatedEventId,
      relatedWikiSlug: dto.relatedWikiSlug,
      note: dto.note,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
    );
  }

  bool get isDone => status == TodoStatus.done;

  /// 优先级排序权重（高 > 普通 > 低），列表用
  int get priorityRank {
    switch (priority) {
      case 'high':
        return 0;
      case 'low':
        return 2;
      default:
        return 1;
    }
  }
}