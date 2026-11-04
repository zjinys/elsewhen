//! 长程计划 FRB 门面（P1，AI 为中心的意图拆解）。
//!
//! 计划本身不占活跃名额（「目标不设上限、注意力保护移驻并发步骤」，
//! 2026-11-04 定案）；步骤就是 goals 行，经 plan_id/step_index 挂接。
//! 本层只做参数校验与状态解析，错误直接上抛由界面提示。

use crate::api::goals::GoalDto;
use crate::storage::{GoalPlan, PlanStatus, Store};
use anyhow::Result;

/// 计划 DTO。`assumptions` 保持 JSON 数组文本原样透传（软时间窗/现状/
/// 路径/风险的结构约定由 P2 AI 侧定义；UI 与门面不做结构假设）。
#[derive(Clone, Debug)]
pub struct PlanDto {
    pub id: String,
    pub intent_text: String,
    pub vision: String,
    pub assumptions: String,
    /// proposed / active / paused / completed
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<GoalPlan> for PlanDto {
    fn from(p: GoalPlan) -> Self {
        Self {
            id: p.id,
            intent_text: p.intent_text,
            vision: p.vision,
            assumptions: p.assumptions,
            status: p.status.as_str().to_string(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

fn parse_status(status: &str) -> Result<PlanStatus> {
    PlanStatus::parse(status).ok_or_else(|| {
        anyhow::anyhow!("计划状态只能是 proposed / active / paused / completed，收到: {status}")
    })
}

/// 列出自建的全部计划（创建时间升序；数量由用户意图而来，不设上限）。
pub fn list_plans() -> Result<Vec<PlanDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.list_plans()?.into_iter().map(PlanDto::from).collect())
}

pub fn get_plan(id: String) -> Result<Option<PlanDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.get_plan(&id)?.map(PlanDto::from))
}

/// 新建计划。`intent_text` 是用户的原话（意图原文，不被改写）；
/// `assumptions` 必须是 JSON 数组文本（可传 "[]"）。
pub fn create_plan(intent_text: String, vision: String, assumptions: String) -> Result<PlanDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let plan = store.create_plan(&intent_text, &vision, &assumptions)?;
    Ok(PlanDto::from(plan))
}

/// 更新愿景与假设清单（拆解纠错路径：用户改假设即改这里）。
pub fn update_plan_meta(id: String, vision: String, assumptions: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_plan_meta(&id, &vision, &assumptions)
}

/// 计划状态流转（暂停搁置 / 读档复活 / 标记达成）。
pub fn set_plan_status(id: String, status: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_plan_status(&id, parse_status(&status)?)
}

/// 删除计划（步骤目标解绑保留，不随计划删除）。
pub fn delete_plan(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_plan(&id)
}

/// 列出计划的全部步骤（step_index 升序，无序号排末尾）。
pub fn list_plan_steps(plan_id: String) -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_plan_steps(&plan_id)?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 把一条目标挂接为计划的步骤（`step_index` 为展示排序键；缺省排末尾）。
pub fn attach_goal_to_plan(goal_id: String, plan_id: String, step_index: Option<i64>) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.attach_goal_to_plan(&goal_id, &plan_id, step_index)
}

/// 并发闸计数：计划内当前活跃的步骤数（供 AI 调度与界面展示）。
pub fn count_active_steps(plan_id: String) -> Result<i64> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.count_active_steps(&plan_id)
}

/// 等待队列：计划内已挂接但落暂停的步骤（step_index 升序）。
pub fn list_waiting_steps(plan_id: String) -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_waiting_steps(&plan_id)?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}
