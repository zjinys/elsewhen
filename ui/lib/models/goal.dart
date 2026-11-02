import '../bridge/api.dart' as api;

/// 目标阶段：近期 / 中期 / 长远
///
/// 是标签不是槽位：同一阶段可以有多条活跃目标，也不要求三个阶段都有。
enum GoalPhase {
  near,
  mid,
  long;

  static GoalPhase fromString(String s) {
    switch (s) {
      case 'mid':
        return GoalPhase.mid;
      case 'long':
        return GoalPhase.long;
      default:
        return GoalPhase.near;
    }
  }

  String get wire => name;

  /// 中文展示名
  String get label {
    switch (this) {
      case GoalPhase.near:
        return '近期';
      case GoalPhase.mid:
        return '中期';
      case GoalPhase.long:
        return '长远';
    }
  }
}

/// 目标状态：活跃 / 暂停搁置 / 已达成
enum GoalStatus {
  active,
  paused,
  completed;

  static GoalStatus fromString(String s) {
    switch (s) {
      case 'paused':
      case 'superseded':
        return GoalStatus.paused;
      case 'completed':
        return GoalStatus.completed;
      default:
        return GoalStatus.active;
    }
  }

  String get wire => name;

  String get label {
    switch (this) {
      case GoalStatus.active:
        return '进行中';
      case GoalStatus.paused:
        return '已暂停';
      case GoalStatus.completed:
        return '已达成';
    }
  }
}

/// 一条目标。活跃目标最多 [maxActiveGoals] 条，由数据库触发器强制。
class Goal {
  final String id;
  final String content;
  final GoalPhase phase;
  final GoalStatus status;
  final String? why;
  final String? currentFocus;
  final String? blocker;
  final String? doneCriteria;
  final DateTime createdAt;
  final DateTime updatedAt;
  final DateTime? supersededAt;

  const Goal({
    required this.id,
    required this.content,
    required this.phase,
    required this.status,
    this.why,
    this.currentFocus,
    this.blocker,
    this.doneCriteria,
    required this.createdAt,
    required this.updatedAt,
    this.supersededAt,
  });

  factory Goal.fromDto(api.GoalDto dto) {
    return Goal(
      id: dto.id,
      content: dto.content,
      phase: GoalPhase.fromString(dto.phase),
      status: GoalStatus.fromString(dto.status),
      why: dto.why,
      currentFocus: dto.currentFocus,
      blocker: dto.blocker,
      doneCriteria: dto.doneCriteria,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
      supersededAt: dto.supersededAt == null
          ? null
          : DateTime.parse(dto.supersededAt!).toLocal(),
    );
  }

  bool get isArchived => status != GoalStatus.active;
  bool get isPaused => status == GoalStatus.paused;
  bool get isCompleted => status == GoalStatus.completed;
}
