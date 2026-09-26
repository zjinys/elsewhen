//! 个人经验规则 FRB 门面。

use super::*;
use crate::storage::RuleStatus;

/// 个人经验规则 DTO
#[derive(Clone, Debug)]
pub struct RuleDto {
    pub id: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
}

/// 列出规则库（含已生效与待确认）
pub fn list_rules() -> Result<Vec<RuleDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let rules = store.list_rules(None, None)?;
    Ok(rules
        .into_iter()
        .map(|r| RuleDto {
            id: r.id,
            content: r.content,
            status: r.status.as_str().to_string(),
            created_at: r.created_at,
        })
        .collect())
}

/// 新增一条规则（手动添加，直接生效）
pub fn add_rule(content: String) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.add_rule(&content, RuleStatus::Active, None)
}

/// 删除一条规则
pub fn delete_rule(rule_id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.delete_rule(&rule_id)?;
    Ok(())
}
