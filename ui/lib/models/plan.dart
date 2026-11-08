import '../l10n/app_localizations_zh.dart';
import '../bridge/api.dart' as api;

/// 计划状态机（与 Rust 侧 PlanStatus 同构）。
enum PlanStatus {
  proposed,
  active,
  paused,
  completed;

  static PlanStatus fromString(String s) {
    switch (s) {
      case 'proposed':
        return PlanStatus.proposed;
      case 'paused':
        return PlanStatus.paused;
      case 'completed':
        return PlanStatus.completed;
      default:
        return PlanStatus.active;
    }
  }

  String get wire => name;

  String get label {
    switch (this) {
      case PlanStatus.proposed:
        return AppLocalizationsZh().legacyPlanPending;
      case PlanStatus.active:
        return AppLocalizationsZh().legacyPlanActive;
      case PlanStatus.paused:
        return AppLocalizationsZh().pausedGoals;
      case PlanStatus.completed:
        return AppLocalizationsZh().achievedGoals;
    }
  }
}

/// 一条长程计划（P1：AI 为中心的意图拆解）。
///
/// `intentText` 是用户原话（意图原文，不被改写）；`assumptions` 保持 JSON
/// 数组原文（软时间窗/现状/路径/风险的结构约定由 AI 侧定义，UI 不做结构
/// 假设，渲染前自行 decode 并容错）。步骤就是 goals 行，经 planId/stepIndex
/// 挂接（见 [Goal.planId]）。
class GoalPlan {
  final String id;
  final String intentText;
  final String vision;
  final String assumptions;
  final PlanStatus status;
  final DateTime createdAt;
  final DateTime updatedAt;

  const GoalPlan({
    required this.id,
    required this.intentText,
    required this.vision,
    required this.assumptions,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
  });

  factory GoalPlan.fromDto(api.PlanDto dto) {
    return GoalPlan(
      id: dto.id,
      intentText: dto.intentText,
      vision: dto.vision,
      assumptions: dto.assumptions,
      status: PlanStatus.fromString(dto.status),
      createdAt: DateTime.parse(dto.createdAt).toLocal(),
      updatedAt: DateTime.parse(dto.updatedAt).toLocal(),
    );
  }

  bool get isActive => status == PlanStatus.active;
}
