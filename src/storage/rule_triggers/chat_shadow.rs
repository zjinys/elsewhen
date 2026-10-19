//! 对话影子校验链（迁移 54）。
//!
//! 与事件链同构但独立成表：聊天消息不进 events，rule_hits 的 FK 锚定
//! 事件，不能混用。两阶段：
//! ① 主回复完成后对用户消息跑关键词预筛（零 LLM 成本）落 pending；
//! ② 后台 worker 把 pending 命中喂给 LLM 判定适用性，回填
//!    applicable/excluded。
//! 产出只落本表，绝不改写主回复、不向对话注入内容。

use super::{RULE_TRIGGER_MODE_OFF, match_rules_for_text, rule_trigger_mode};
use crate::storage::Store;
use anyhow::Result;
use rusqlite::{Connection, params};
use uuid::Uuid;

// ── 对话影子校验（迁移 54）：聊天轮次预筛 + 后台 LLM 判定 ──────────
//
// 与事件链同构但独立成表：聊天消息不进 events，rule_hits 的 FK 锚定事件，
// 不能混用。两阶段：① 主回复完成后对用户消息跑关键词预筛（零 LLM 成本）
// 落 pending；② 后台 worker 把 pending 命中喂给 LLM 判定适用性，回填
// applicable/excluded。产出只落本表，绝不改写主回复、不向对话注入内容。

/// 一次聊天命中记录。
pub struct ChatRuleHitView {
    pub id: String,
    pub rule_id: String,
    pub rule_content: String,
    pub conversation_id: String,
    pub input_message_id: String,
    pub status: String,
    pub confidence: Option<f64>,
    pub created_at: String,
}

/// 主回复完成后调用：对当轮用户消息做确定性预筛，命中落 pending。
/// mode=off 或无触发规则零成本返回；失败静默（预警不阻塞对话）。
/// 注：raw_text 只读，任何失败不影响生成链路——与 screen_event_on 同惯例。
pub(crate) fn screen_chat_message_on(
    conn: &Connection,
    conversation_id: &str,
    input_message_id: &str,
    raw_text: &str,
) {
    let inner = || -> Result<()> {
        if rule_trigger_mode(conn)? == RULE_TRIGGER_MODE_OFF {
            return Ok(());
        }
        let matched = match_rules_for_text(conn, raw_text)?;
        if matched.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().to_rfc3339();
        for rule_id in matched {
            conn.execute(
                "INSERT INTO chat_rule_hits
                 (id,rule_id,conversation_id,input_message_id,status,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,'pending',?5,?5)
                 ON CONFLICT(rule_id, input_message_id) DO NOTHING",
                params![
                    Uuid::new_v4().to_string(),
                    rule_id,
                    conversation_id,
                    input_message_id,
                    now
                ],
            )?;
        }
        Ok(())
    };
    // 记录失败原因到 stderr 太吵：静默放行（缺失只损失提醒，不影响对话）。
    if let Err(_error) = inner() {}
}

/// 后台判定用的待处理命中（按规则与消息联查原文）。
pub(crate) struct PendingChatHit {
    pub id: String,
    pub rule_id: String,
    pub rule_content: String,
    pub conversation_id: String,
    pub message_text: String,
}

/// 取一批 pending 命中用于 LLM 判定（最旧优先，封顶防单轮过载）。
pub(crate) fn pending_chat_hits(conn: &Connection, limit: i64) -> Result<Vec<PendingChatHit>> {
    let mut statement = conn.prepare(
        "SELECT h.id, h.rule_id, r.content, h.conversation_id, m.content
         FROM chat_rule_hits h
         JOIN rules r ON r.id = h.rule_id
         JOIN messages m ON m.id = h.input_message_id
         WHERE h.status = 'pending'
         ORDER BY h.created_at ASC
         LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        Ok(PendingChatHit {
            id: row.get(0)?,
            rule_id: row.get(1)?,
            rule_content: row.get(2)?,
            conversation_id: row.get(3)?,
            message_text: row.get(4)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// 回填判定结果（幂等：只动 pending 行，重放不改写已定状态）。
pub(crate) fn resolve_chat_hit_on(
    conn: &Connection,
    hit_id: &str,
    applicable: bool,
    confidence: Option<f64>,
) -> Result<()> {
    conn.execute(
        "UPDATE chat_rule_hits
         SET status = CASE WHEN ?2 = 1 THEN 'applicable' ELSE 'excluded' END,
             confidence = ?3, updated_at = ?4
         WHERE id = ?1 AND status = 'pending'",
        params![
            hit_id,
            applicable as i64,
            confidence,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

impl Store {
    /// 对话影子校验预筛（主回复完成后由生成链调用；失败静默不阻塞对话）。
    pub(crate) fn screen_chat_message(
        &self,
        conversation_id: &str,
        input_message_id: &str,
        raw_text: &str,
    ) {
        screen_chat_message_on(
            &self.connection,
            conversation_id,
            input_message_id,
            raw_text,
        );
    }

    /// 后台判定用的待处理影子命中。
    pub(crate) fn pending_chat_hits(&self, limit: i64) -> Result<Vec<PendingChatHit>> {
        pending_chat_hits(&self.connection, limit)
    }

    /// 回填影子判定结果（幂等，只动 pending）。
    pub(crate) fn resolve_chat_hit(
        &self,
        hit_id: &str,
        applicable: bool,
        confidence: Option<f64>,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        resolve_chat_hit_on(&self.connection, hit_id, applicable, confidence)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 最近的对话影子命中（规则管理页审查面读用）。
    pub fn recent_chat_rule_hits(&self, limit: i64) -> Result<Vec<ChatRuleHitView>> {
        let mut statement = self.connection.prepare(
            "SELECT h.id, h.rule_id, r.content, h.conversation_id, h.input_message_id,
                    h.status, h.confidence, h.created_at
             FROM chat_rule_hits h
             JOIN rules r ON r.id = h.rule_id
             ORDER BY h.created_at DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(ChatRuleHitView {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                rule_content: row.get(2)?,
                conversation_id: row.get(3)?,
                input_message_id: row.get(4)?,
                status: row.get(5)?,
                confidence: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}
