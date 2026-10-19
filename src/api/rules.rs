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
    /// 触发场景（capture/conversation/daily_review；空 = 全场景）
    pub trigger_scene: String,
    /// 触发关键词（空 = 不参与事件预筛）
    pub trigger_keywords: Vec<String>,
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
            trigger_scene: r.trigger_scene.unwrap_or_default(),
            trigger_keywords: r
                .trigger_keywords
                .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                .unwrap_or_default(),
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

/// 配置规则的触发条件（决策辅助 D1）。keywords 为空 = 清除、不参与事件预筛；
/// scene 为空 = 全场景。规则触发是本地预筛 + 分析判定，不外发任何内容。
pub fn set_rule_trigger(
    rule_id: String,
    trigger_scene: String,
    trigger_keywords: Vec<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let scene = trigger_scene.trim();
    let scene_opt = if scene.is_empty() { None } else { Some(scene) };
    let keywords: Vec<String> = trigger_keywords
        .iter()
        .map(|kw| kw.trim().to_string())
        .filter(|kw| !kw.is_empty())
        .collect();
    let keywords_opt = if keywords.is_empty() {
        None
    } else {
        Some(keywords.as_slice())
    };
    store.set_rule_trigger(rule_id.trim(), scene_opt, keywords_opt)
}

/// 提醒偏好：instant（默认，高置信即时 + 其余聚合）/ review_only（全部聚合）/
/// off（彻底关闭，预筛不跑）。
pub fn set_rule_trigger_mode(mode: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_rule_trigger_mode(mode.trim())
}

pub fn get_rule_trigger_mode() -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.rule_trigger_mode()
}

/// 规则命中 DTO
#[derive(Clone, Debug)]
pub struct RuleHitDto {
    pub id: String,
    pub rule_id: String,
    pub rule_content: String,
    pub status: String,
    pub created_at: String,
    /// 采纳反馈（D6 闭环度量）：helpful / not_relevant / 未反馈 NULL。
    pub feedback: Option<String>,
}

fn hit_dto(hv: crate::storage::rule_triggers::RuleHitView) -> RuleHitDto {
    RuleHitDto {
        id: hv.id,
        rule_id: hv.rule_id,
        rule_content: hv.rule_content,
        status: hv.status,
        created_at: hv.created_at,
        feedback: hv.feedback,
    }
}

/// 事件旁标记：某事件的未拒绝命中（即时提醒 notoified + 聚合 aggregated）。
pub fn list_event_rule_hits(event_id: String) -> Result<Vec<RuleHitDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .rule_hits_for_event(event_id.trim())?
        .into_iter()
        .map(hit_dto)
        .collect())
}

/// 用户对单条提醒的采纳反馈（D6 只记录不推断）：Some("helpful"|"not_relevant")
/// 记录；None 清除反馈。命中行不存在或非已处置状态时 fail-closed。
pub fn set_rule_hit_feedback(id: String, feedback: Option<String>) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let id = id.trim();
    anyhow::ensure!(!id.is_empty(), "命中 id 不能为空");
    let fb = feedback.as_deref().map(str::trim).filter(|f| !f.is_empty());
    if feedback.is_some() && fb.is_none() {
        anyhow::bail!("feedback 不能为空字符串");
    }
    if !store.set_rule_hit_feedback(id, fb)? {
        anyhow::bail!("命中不存在或不可反馈");
    }
    Ok(())
}

/// 最近的即时提醒与聚合（今日提醒列表 / 每日回顾读面）。
pub fn list_recent_rule_hits(limit: i64) -> Result<Vec<RuleHitDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let limit = limit.clamp(1, 200);
    Ok(store
        .recent_rule_hits(limit)?
        .into_iter()
        .map(hit_dto)
        .collect())
}

/// 对话影子校验命中 DTO（迁移 54：聊天预筛 → 后台 LLM 判定）。
#[derive(Clone, Debug)]
pub struct ChatRuleHitDto {
    pub id: String,
    pub rule_id: String,
    pub rule_content: String,
    pub conversation_id: String,
    pub input_message_id: String,
    /// pending / applicable / excluded
    pub status: String,
    pub confidence: Option<f64>,
    pub created_at: String,
}

/// 最近的对话影子命中（规则管理页审查面读用）。
pub fn list_recent_chat_rule_hits(limit: i64) -> Result<Vec<ChatRuleHitDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let limit = limit.clamp(1, 200);
    Ok(store
        .recent_chat_rule_hits(limit)?
        .into_iter()
        .map(|h| ChatRuleHitDto {
            id: h.id,
            rule_id: h.rule_id,
            rule_content: h.rule_content,
            conversation_id: h.conversation_id,
            input_message_id: h.input_message_id,
            status: h.status,
            confidence: h.confidence,
            created_at: h.created_at,
        })
        .collect())
}
