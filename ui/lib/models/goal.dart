import '../l10n/app_localizations_zh.dart';
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
        return AppLocalizationsZh().nearTerm;
      case GoalPhase.mid:
        return AppLocalizationsZh().midTerm;
      case GoalPhase.long:
        return AppLocalizationsZh().longTerm;
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
        return AppLocalizationsZh().todoStatusOpen;
      case GoalStatus.paused:
        return AppLocalizationsZh().pausedGoalState;
      case GoalStatus.completed:
        return AppLocalizationsZh().achievedGoals;
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

  /// 所属长程计划（P1 意图拆解；null = 未挂计划）
  final String? planId;

  /// 计划内步骤序号（null = 未挂计划）
  final int? stepIndex;
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
    this.planId,
    this.stepIndex,
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
      planId: dto.planId,
      stepIndex: dto.stepIndex?.toInt(),
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

/// 航向偏离校准提醒（Compass Nudge，FR-PES-005-04）
class GoalCompassNudge {
  final String id;
  final String goalId;
  final String goalContent;

  /// 所属长程计划（P4：停滞步骤属于计划时的计划意图；null = 无计划归属）
  final String? planIntent;
  final String currentFocus;
  final int stalledDays;
  final String recentTopics;
  final String status;
  final DateTime createdAt;
  final DateTime? dismissedUntil;
  final api.GoalCompassAssessmentDto? assessment;

  const GoalCompassNudge({
    required this.id,
    required this.goalId,
    required this.goalContent,
    this.planIntent,
    required this.currentFocus,
    required this.stalledDays,
    required this.recentTopics,
    required this.status,
    required this.createdAt,
    this.dismissedUntil,
    this.assessment,
  });

  factory GoalCompassNudge.fromDto(api.GoalCompassNudgeDto dto) {
    return GoalCompassNudge(
      id: dto.id,
      goalId: dto.goalId,
      goalContent: dto.goalContent,
      planIntent: dto.planIntent,
      assessment: dto.assessment,
      currentFocus: dto.currentFocus,
      stalledDays: dto.stalledDays.toInt(),
      recentTopics: dto.recentTopics,
      status: dto.status,
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      dismissedUntil: dto.dismissedUntil == null
          ? null
          : DateTime.parse(dto.dismissedUntil!).toLocal(),
    );
  }
}
