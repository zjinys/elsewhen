//! 实体（人物/项目/主题）FRB 门面。

use super::*;

pub fn list_entity_facts(entity_kind: String, entity_slug: String) -> Result<Vec<EntityFactDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_entity_facts(entity_kind.trim(), entity_slug.trim())?
        .into_iter()
        .map(|fact| EntityFactDto {
            id: fact.id,
            entity_kind: fact.entity_kind,
            entity_slug: fact.entity_slug,
            fact_text: fact.fact_text,
            occurred_at: fact.occurred_at,
            confidence: fact.confidence,
            source_event_id: fact.source_event_id,
            created_at: fact.created_at,
            last_seen_at: fact.last_seen_at,
        })
        .collect())
}

pub fn delete_entity_fact(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_entity_fact(id.trim())
}

pub fn list_entity_aliases(entity_kind: String, entity_slug: String) -> Result<Vec<String>> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.list_entity_aliases(entity_kind.trim(), entity_slug.trim())
}

pub fn add_entity_alias(entity_kind: String, entity_slug: String, alias: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.add_entity_alias(
        entity_kind.trim(),
        entity_slug.trim(),
        alias.trim(),
    )
}

/// Merge two confirmed entities after explicit user confirmation.
pub fn merge_entity(entity_kind: String, source_slug: String, target_slug: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.merge_entity(
        entity_kind.trim(),
        source_slug.trim(),
        target_slug.trim(),
    )
}

#[derive(Clone, Debug)]
pub struct EntityMergeStatusDto {
    pub source_slug: String,
    pub target_slug: String,
    pub entity_kind: String,
    pub created_at: String,
}

pub fn get_entity_merge_status(source_slug: String) -> Result<Option<EntityMergeStatusDto>> {
    let config = crate::config::AppConfig::load()?;
    Ok(Store::open(&config.database_path)?
        .entity_merge_status(source_slug.trim())?
        .map(|m| EntityMergeStatusDto {
            source_slug: m.source_slug,
            target_slug: m.target_slug,
            entity_kind: m.entity_kind,
            created_at: m.created_at,
        }))
}

/// Undo one merge only when every moved row still matches its merge snapshot.
pub fn undo_entity_merge(source_slug: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.undo_entity_merge(source_slug.trim())
}
