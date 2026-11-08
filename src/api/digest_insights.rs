//! 知识库洞见浮现 FRB 门面：tick、读面与四重闭环动作。

use crate::storage::{DigestInsightSurfacing, Store};
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct DigestInsightSurfacingDto {
    pub id: String,
    pub insight_id: Option<String>,
    pub page_slug: String,
    pub kind: String,
    pub title: String,
    pub observation: String,
    pub action_suggestion: Option<String>,
    pub status: String,
    pub surfaced_at: Option<String>,
    pub surfaced_date: Option<String>,
    pub message_id: Option<String>,
    pub action_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 页面仍存在时的 kind（供「开启习惯问候」出口判断可用性）。
    pub page_kind: Option<String>,
    /// 页面仍在生效。
    pub page_active: bool,
}

impl From<DigestInsightSurfacing> for DigestInsightSurfacingDto {
    fn from(value: DigestInsightSurfacing) -> Self {
        Self {
            id: value.id,
            insight_id: value.insight_id,
            page_slug: value.page_slug,
            kind: value.kind,
            title: value.title,
            observation: value.observation,
            action_suggestion: value.action_suggestion,
            status: value.status,
            surfaced_at: value.surfaced_at,
            surfaced_date: value.surfaced_date,
            message_id: value.message_id,
            action_type: value.action_type,
            created_at: value.created_at,
            updated_at: value.updated_at,
            page_kind: value.page_kind,
            page_active: value.page_active,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DigestInsightTickDto {
    /// 本次可见的卡片消息 id（新插入或已存在的同 ref 消息）。
    pub message_ids: Vec<String>,
    /// 未动作的已浮现卡片数。
    pub pending_count: i32,
}

fn open_store() -> Result<Store> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)
}

/// 5s worker tick：过期超时卡片，至多浮现一条候选（owner、频控、静默窗
/// 全部在存储层判定；这里只做组装，没有手动触发入口）。
pub fn tick_digest_insights() -> Result<DigestInsightTickDto> {
    let store = open_store()?;
    let message_ids = store.tick_digest_insights()?;
    let pending_count = store.pending_digest_insight_count()?;
    Ok(DigestInsightTickDto {
        message_ids,
        pending_count,
    })
}

pub fn list_digest_insight_surfacings() -> Result<Vec<DigestInsightSurfacingDto>> {
    let store = open_store()?;
    Ok(store
        .list_digest_insight_surfacings()?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn pending_digest_insight_count() -> Result<i32> {
    let store = open_store()?;
    store.pending_digest_insight_count()
}

/// 四重闭环出口：采纳为规则 / 标记已开启习惯问候 / 转为待办 / 知道了。
pub fn act_on_digest_insight(
    message_id: String,
    action: String,
) -> Result<DigestInsightSurfacingDto> {
    let store = open_store()?;
    Ok(store.act_on_digest_insight(&message_id, &action)?.into())
}
