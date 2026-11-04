part of '../rust_bridge_repository.dart';

//! RustBridgeRepository 便捷薄委托（长程计划域，P1 意图拆解）。
//!
//! 与 goals 域同规则：纯委托放 part extension；若将来有测试 Fake 需要
//! 覆写拦截（如计划页 widget 测试要假落库），把对应方法移到类内保证虚分派。
extension RepoPlans on RustBridgeRepository {
  /// 列出自建的全部计划（创建时间升序；数量由用户意图而来，不设上限）
  Future<List<GoalPlan>> listPlans() async {
    final dtos = await api.listPlans();
    return dtos.map(GoalPlan.fromDto).toList();
  }

  /// 获取单个计划
  Future<GoalPlan?> getPlan(String id) async {
    final dto = await api.getPlan(id: id);
    return dto == null ? null : GoalPlan.fromDto(dto);
  }

  /// 新建计划。`intentText` 是用户原话（意图原文）；`assumptions` 必须是
  /// JSON 数组文本（可传 '[]'，软时间窗等结构由 AI 侧约定）。
  Future<GoalPlan> createPlan({
    required String intentText,
    String vision = '',
    String assumptions = '[]',
  }) async {
    final dto = await api.createPlan(
      intentText: intentText,
      vision: vision,
      assumptions: assumptions,
    );
    return GoalPlan.fromDto(dto);
  }

  /// 更新愿景与假设清单（拆解纠错路径：用户改假设即改这里）
  Future<void> updatePlanMeta({
    required String id,
    required String vision,
    required String assumptions,
  }) => api.updatePlanMeta(id: id, vision: vision, assumptions: assumptions);

  /// 计划状态流转（暂停搁置 / 读档复活 / 标记达成）
  Future<void> setPlanStatus({
    required String id,
    required PlanStatus status,
  }) => api.setPlanStatus(id: id, status: status.wire);

  /// 删除计划（步骤目标解绑保留，不随计划删除）
  Future<void> deletePlan(String id) => api.deletePlan(id: id);

  /// 列出计划的全部步骤（stepIndex 升序，无序号排末尾）
  Future<List<Goal>> listPlanSteps(String planId) async {
    final dtos = await api.listPlanSteps(planId: planId);
    return dtos.map(Goal.fromDto).toList();
  }

  /// 把一条目标挂接为计划的步骤（stepIndex 为展示排序键；缺省排末尾）
  Future<void> attachGoalToPlan({
    required String goalId,
    required String planId,
    int? stepIndex,
  }) => api.attachGoalToPlan(
    goalId: goalId,
    planId: planId,
    stepIndex: stepIndex,
  );

  /// 并发闸计数：计划内当前活跃的步骤数（供 AI 调度与界面展示）
  Future<int> countActiveSteps(String planId) async =>
      (await api.countActiveSteps(planId: planId)).toInt();

  /// 等待队列：计划内已挂接但落暂停的步骤（stepIndex 升序）
  Future<List<Goal>> listWaitingSteps(String planId) async {
    final dtos = await api.listWaitingSteps(planId: planId);
    return dtos.map(Goal.fromDto).toList();
  }
}
