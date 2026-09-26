//! 人物关系 FRB 门面。

use super::*;
use crate::storage::RelationDraft;

/// 一条人物关系 DTO（AI 从对话识别「人 ↔ 事情/项目」，用户确认后保存）
#[derive(Clone, Debug)]
pub struct RelationDto {
    pub id: String,
    pub from_slug: String,
    pub from_kind: String,
    pub to_slug: String,
    pub to_kind: String,
    pub relation: String,
    pub note: Option<String>,
    pub confidence: i64,
    pub created_at: String,
    pub last_seen_at: String,
}

impl From<crate::storage::Relation> for RelationDto {
    fn from(r: crate::storage::Relation) -> Self {
        Self {
            id: r.id,
            from_slug: r.from_slug,
            from_kind: r.from_kind,
            to_slug: r.to_slug,
            to_kind: r.to_kind,
            relation: r.relation,
            note: r.note,
            confidence: r.confidence,
            created_at: r.created_at,
            last_seen_at: r.last_seen_at,
        }
    }
}

/// 与某页相关的人物关系（双向：作为人物方或作为事情/项目方）
pub fn list_relations_for_page(slug: String) -> Result<Vec<RelationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let rows = store.list_relations_for_page(&slug)?;
    Ok(rows.into_iter().map(RelationDto::from).collect())
}

/// 全部人物关系（备用：未来人物视图）
pub fn list_relations() -> Result<Vec<RelationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let rows = store.list_relations()?;
    Ok(rows.into_iter().map(RelationDto::from).collect())
}

/// 手动添加/刷新一条人物关系（应用内修复合用；一般由 AI 草拟、用户确认后生成）。
/// 两侧页面必须已存在；返回落库后的关系（含真实 id）。
pub fn add_relation(
    from_slug: String,
    to_slug: String,
    relation: String,
    note: Option<String>,
) -> Result<RelationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let from = store
        .get_wiki_page(&from_slug)?
        .ok_or_else(|| anyhow::anyhow!("起始页面不存在：{from_slug}"))?;
    let to = store
        .get_wiki_page(&to_slug)?
        .ok_or_else(|| anyhow::anyhow!("目标页面不存在：{to_slug}"))?;
    let rel = store.upsert_relation(&RelationDraft {
        from_slug,
        from_kind: from.kind,
        to_slug,
        to_kind: to.kind,
        relation,
        note,
        confidence: 3,
        source_conversation_id: None,
        source_event_id: None,
    })?;
    Ok(RelationDto::from(rel))
}

/// 删除一条人物关系（修正误识别时用）。返回是否真的删掉了。
pub fn delete_relation(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.delete_relation(&id)?)
}
