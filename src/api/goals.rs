//! 目标 FRB 门面（FR-PES-005-01/02）。
//!
//! 活跃目标上限由数据库触发器强制，本层只做参数校验与错误转译；
//! 达到上限时不做任何自动顶替，错误直接上抛由界面提示用户先归档。

use crate::storage::{Goal, GoalPhase, Store};
use anyhow::Result;

/// 目标 DTO
#[derive(Clone, Debug)]
pub struct GoalDto {
    pub id: String,
    pub content: String,
    /// near / mid / long
    pub phase: String,
    /// active / superseded
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub superseded_at: Option<String>,
}

impl From<Goal> for GoalDto {
    fn from(g: Goal) -> Self {
        Self {
            id: g.id,
            content: g.content,
            phase: g.phase.as_str().to_string(),
            status: g.status.as_str().to_string(),
            created_at: g.created_at,
            updated_at: g.updated_at,
            superseded_at: g.superseded_at,
        }
    }
}

fn parse_phase(phase: &str) -> Result<GoalPhase> {
    GoalPhase::parse(phase)
        .ok_or_else(|| anyhow::anyhow!("目标阶段只能是 near / mid / long，收到: {phase}"))
}

/// 活跃目标条数上限，供界面在达到上限时预先禁用新增。
pub const MAX_ACTIVE_GOALS: usize = crate::storage::MAX_ACTIVE_GOALS;

/// 列出全部活跃目标（最多 3 条，按近期→中期→长远排序）
pub fn list_active_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_active_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 列出已归档目标（历史，按归档时间倒序）
pub fn list_archived_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_archived_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 新建一条活跃目标
///
/// 活跃目标已满 3 条时拒绝并报错，界面须提示用户先归档一条，不得自动顶替既有目标。
pub fn create_goal(content: String, phase: String) -> Result<GoalDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if content.trim().is_empty() {
        anyhow::bail!("目标内容不能为空");
    }
    let goal = store.create_goal(content.trim(), parse_phase(&phase)?)?;
    Ok(GoalDto::from(goal))
}

/// 编辑一条目标的正文与阶段（状态不变，故不受上限限制）
pub fn update_goal(id: String, content: String, phase: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if content.trim().is_empty() {
        anyhow::bail!("目标内容不能为空");
    }
    store.update_goal(&id, content.trim(), parse_phase(&phase)?)
}

/// 归档一条目标，让出活跃名额但保留内容与阶段作为历史
pub fn archive_goal(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.archive_goal(&id)
}

/// 复活一条已归档目标。活跃目标已满 3 条时同样拒绝。
pub fn reactivate_goal(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.reactivate_goal(&id)
}
