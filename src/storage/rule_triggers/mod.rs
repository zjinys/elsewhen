//! 规则触发引擎——两条链共享的闸门层与组织入口。
//!
//! 两条链共用一套确定性关键词预筛（`match_rules_for_text`）与全局模式
//! 开关（`rule_trigger_settings.mode`），但产物表/消费方刻意分离：
//!
//! * **事件提醒链**（`event_chain`，迁移 35/D1）：事件入库时预筛，命中
//!   挂 `rule_hits(pending)`；事件分析 LLM 按 v3 契约 `rule_confirmations`
//!   回填 notified（即时提醒）/ aggregated（每日回顾）/ declined。
//! * **对话影子链**（`chat_shadow`，迁移 54）：聊天轮不落事件卷宗，主
//!   回复完成后预筛，命中挂 `chat_rule_hits(pending)`；后台判定队列回填
//!   applicable / excluded。
//!
//! 共同硬边界：
//! * 预筛是副产品——挂在调用方已有事务/流程内，**任何失败都静默放行**
//!  （预警绝不阻塞主流程）。
//! * mode = 'off' 时两条链都不预筛、不建记录。
//! * 命中记录只挂本地数据，不外发（快照归类 local-event-reminders）。
//!
//! 本文件只放共享闸门；各链自有逻辑与测试见子模块文件。

use super::Store;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) mod chat_shadow;
pub(crate) mod event_chain;
#[cfg(test)]
mod tests;

// crate 内部按平铺路径引用（src/api 等），re-export 保持既有路径不变。
pub(crate) use chat_shadow::PendingChatHit;
pub(crate) use event_chain::{RuleConfirmation, RuleHitView, screen_event_on};

pub(crate) const RULE_TRIGGER_MODE_INSTANT: &str = "instant";
pub(crate) const RULE_TRIGGER_MODE_REVIEW_ONLY: &str = "review_only";
pub(crate) const RULE_TRIGGER_MODE_OFF: &str = "off";

/// 即时提醒的置信阈值：分析判定 applicable 且 confidence ≥ 此值才 notified，
/// 其余 applicable 落聚合。坚定保守——误报一次桌面提醒的代价高于漏报。
pub(crate) const RULE_CONFIRM_INSTANT_THRESHOLD: f64 = 0.8;

/// 当前提醒偏好（settings singleton；缺行按 instant 处理）。
pub fn rule_trigger_mode(conn: &Connection) -> Result<String> {
    Ok(conn
        .query_row(
            "SELECT mode FROM rule_trigger_settings WHERE singleton=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| RULE_TRIGGER_MODE_INSTANT.to_string()))
}

/// 设置提醒偏好（API 调用方自管事务/写事务惯例由上层决定，这里只做校验+写）。
pub fn set_rule_trigger_mode(conn: &Connection, mode: &str) -> Result<()> {
    ensure_mode(mode)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE rule_trigger_settings SET mode=?1, updated_at=?2 WHERE singleton=1",
        params![mode, now],
    )?;
    Ok(())
}

fn ensure_mode(mode: &str) -> Result<()> {
    match mode {
        RULE_TRIGGER_MODE_INSTANT | RULE_TRIGGER_MODE_REVIEW_ONLY | RULE_TRIGGER_MODE_OFF => Ok(()),
        other => anyhow::bail!("rule_trigger_mode 必须是 instant/review_only/off，收到 {other}"),
    }
}

/// 确定性预筛：大小写折叠 + 包含匹配。返回命中的规则 id（trigger_keywords
/// 为 JSON 字符串数组；任一关键词命中即整条规则命中）。
pub(crate) fn match_rules_for_text(conn: &Connection, raw_text: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT id, trigger_keywords FROM rules
         WHERE status='active' AND trigger_keywords IS NOT NULL",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let haystack = raw_text.to_lowercase();
    let mut matched = Vec::new();
    for row in rows {
        let (rule_id, keywords_json) = row?;
        let Ok(keywords) = serde_json::from_str::<Vec<String>>(&keywords_json) else {
            // 迁移 CHECK 保证 json_valid，但数组内元素类型不设防（如数字）：
            // 单条规则格式坏 → 跳过该规则，不拖垮整个预筛。
            continue;
        };
        let hit = keywords
            .iter()
            .any(|kw| !kw.trim().is_empty() && haystack.contains(&kw.trim().to_lowercase()));
        if hit {
            matched.push(rule_id);
        }
    }
    Ok(matched)
}

impl Store {
    pub fn set_rule_trigger_mode(&self, mode: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        set_rule_trigger_mode(&self.connection, mode)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub fn rule_trigger_mode(&self) -> Result<String> {
        rule_trigger_mode(&self.connection)
    }
}
