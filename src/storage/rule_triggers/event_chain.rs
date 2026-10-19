//! 事件提醒链（迁移 35，决策辅助 D1）。
//!
//! 职责链：事件入库时对参与预筛的规则做确定性关键词匹配，命中建
//! `rule_hits(pending)`；事件分析的 LLM 调用带上 pending 规则做适用性
//! 判定（v3 契约 `rule_confirmations`），回填为 notified（即时提醒）/
//! aggregated（每日回顾）/ declined。review_only 模式判定仍跑但不产生
//! 即时提醒（全部落 aggregated）。误报一次桌面提醒的代价高于漏报
//! （见置信阈值）。

use super::{
    RULE_CONFIRM_INSTANT_THRESHOLD, RULE_TRIGGER_MODE_OFF, RULE_TRIGGER_MODE_REVIEW_ONLY,
    match_rules_for_text, rule_trigger_mode,
};
use crate::storage::Store;
use anyhow::Result;
use rusqlite::{Connection, params};
use uuid::Uuid;

/// 事件入库/远端导入时的预筛挂钩。挂在调用方已有事务的 connection 上。
/// 规则未启用预筛（mode=off / 无触发规则）时零成本返回。
pub(crate) fn screen_event_on(conn: &Connection, event_id: &str, raw_text: &str) {
    let inner = || -> Result<()> {
        if rule_trigger_mode(conn)? == RULE_TRIGGER_MODE_OFF {
            return Ok(());
        }
        let matched = match_rules_for_text(conn, raw_text)?;
        if matched.is_empty() {
            return Ok(());
        }
        record_rule_hits_on(conn, event_id, &matched)?;
        Ok(())
    };
    // 预警绝不阻塞捕获（note 红线）：失败静默，事件照常入库。
    if let Err(_error) = inner() {
        // 无日志 crate（项目无依赖惯例）：静默放行。预筛缺失只损失提醒，
        // 不影响事件真源与分析队列；下次 reanalyze 不重跑预筛也不丢已有命中。
    }
}

/// 命中落库（幂等：UNIQUE(rule_id, event_id) OR IGNORE）。
pub(crate) fn record_rule_hits_on(
    conn: &Connection,
    event_id: &str,
    rule_ids: &[String],
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    for rule_id in rule_ids {
        conn.execute(
            "INSERT INTO rule_hits (id, rule_id, event_id, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'pending', ?4, ?4)
             ON CONFLICT(rule_id, event_id) DO NOTHING",
            params![Uuid::new_v4().to_string(), rule_id, event_id, now],
        )?;
    }
    Ok(())
}

/// 某事件待判定的命中（分析 prompt 附带段）。
pub(crate) fn pending_hits_for_event(
    conn: &Connection,
    event_id: &str,
) -> Result<Vec<PendingRuleHit>> {
    let mut statement = conn.prepare(
        "SELECT h.id, h.rule_id, r.content
         FROM rule_hits h JOIN rules r ON r.id = h.rule_id
         WHERE h.event_id = ?1 AND h.status = 'pending'
         ORDER BY h.created_at ASC",
    )?;
    let rows = statement.query_map([event_id], |row| {
        Ok(PendingRuleHit {
            hit_id: row.get(0)?,
            rule_id: row.get(1)?,
            rule_content: row.get(2)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(crate) struct PendingRuleHit {
    pub hit_id: String,
    pub rule_id: String,
    pub rule_content: String,
}

/// 分析判定的回填：applicable 且高置信 → notified；applicable 低置信 →
/// aggregated（review_only 模式下全部聚合）；不适用 → declined。
/// 只处理仍为 pending 的行——重复回放不会改写已处置记录。
pub(crate) fn apply_rule_confirmations_on(
    conn: &Connection,
    event_id: &str,
    confirmations: &[RuleConfirmation],
) -> Result<()> {
    let mode = rule_trigger_mode(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    for confirmation in confirmations {
        let next = if !confirmation.applicable {
            "declined"
        } else if mode == RULE_TRIGGER_MODE_REVIEW_ONLY
            || (confirmation.confidence) < RULE_CONFIRM_INSTANT_THRESHOLD
        {
            "aggregated"
        } else {
            "notified"
        };
        conn.execute(
            "UPDATE rule_hits SET status=?2, confidence=?3, updated_at=?4
             WHERE event_id=?1 AND rule_id=?5 AND status='pending'",
            params![
                event_id,
                next,
                confirmation.confidence,
                now,
                confirmation.rule_id
            ],
        )?;
    }
    Ok(())
}

pub(crate) struct RuleConfirmation {
    pub rule_id: String,
    pub applicable: bool,
    pub confidence: f64,
}

impl Store {
    /// 某事件待判定的命中（分析 worker 组 prompt 用）。
    pub(crate) fn pending_rule_hits_for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<PendingRuleHit>> {
        pending_hits_for_event(&self.connection, event_id)
    }

    /// 分析判定回填（独立事务；失败不影响已入库的分析结果）。
    pub(crate) fn apply_rule_confirmations(
        &self,
        event_id: &str,
        confirmations: &[RuleConfirmation],
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        apply_rule_confirmations_on(&self.connection, event_id, confirmations)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 指定事件的命中（事件旁标记读面）。
    pub fn rule_hits_for_event(&self, event_id: &str) -> Result<Vec<RuleHitView>> {
        let mut statement = self.connection.prepare(
            "SELECT h.id, h.rule_id, r.content, h.status, h.confidence, h.created_at, h.feedback
             FROM rule_hits h JOIN rules r ON r.id = h.rule_id
             WHERE h.event_id = ?1 AND h.status != 'declined'
             ORDER BY h.created_at ASC",
        )?;
        let rows = statement.query_map([event_id], |row| {
            Ok(RuleHitView {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                rule_content: row.get(2)?,
                status: row.get(3)?,
                confidence: row.get(4)?,
                created_at: row.get(5)?,
                feedback: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 最近的已处置命中（今日提醒列表 / 每日回顾聚合读面）。
    pub fn recent_rule_hits(&self, limit: i64) -> Result<Vec<RuleHitView>> {
        let mut statement = self.connection.prepare(
            "SELECT h.id, h.rule_id, r.content, h.status, h.confidence, h.created_at, h.feedback
             FROM rule_hits h JOIN rules r ON r.id = h.rule_id
             WHERE h.status IN ('notified','aggregated')
             ORDER BY h.created_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(RuleHitView {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                rule_content: row.get(2)?,
                status: row.get(3)?,
                confidence: row.get(4)?,
                created_at: row.get(5)?,
                feedback: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 设置单条命中的采纳反馈（D6 只记录不推断）；None = 清除反馈。
    /// 返回是否命中行（不存在/已删除 → false）。
    pub fn set_rule_hit_feedback(&self, id: &str, feedback: Option<&str>) -> Result<bool> {
        if let Some(feedback) = feedback {
            anyhow::ensure!(
                matches!(feedback, "helpful" | "not_relevant"),
                "feedback 只能是 helpful 或 not_relevant"
            );
        }
        let changed = if feedback.is_some() {
            self.connection.execute(
                "UPDATE rule_hits SET feedback=?1, feedback_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2 AND status IN ('notified','aggregated')",
                params![feedback, id],
            )?
        } else {
            self.connection.execute(
                "UPDATE rule_hits SET feedback=NULL, feedback_at=NULL WHERE id=?1",
                params![id],
            )?
        };
        Ok(changed > 0)
    }

    /// 当日已通知/已聚合命中（每日回顾勾连读面：按本地日历日过滤）。
    pub fn rule_hits_on_day(&self, day_start: &str, day_end: &str) -> Result<Vec<RuleHitView>> {
        let mut statement = self.connection.prepare(
            "SELECT h.id, h.rule_id, r.content, h.status, h.confidence, h.created_at, h.feedback
             FROM rule_hits h JOIN rules r ON r.id = h.rule_id
             WHERE h.status IN ('notified','aggregated')
               AND h.created_at >= ?1 AND h.created_at < ?2
             ORDER BY h.created_at DESC LIMIT 50",
        )?;
        let rows = statement.query_map(params![day_start, day_end], |row| {
            Ok(RuleHitView {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                rule_content: row.get(2)?,
                status: row.get(3)?,
                confidence: row.get(4)?,
                created_at: row.get(5)?,
                feedback: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

/// 命中记录视图（API DTO 来源）。规则行只挂事件命中（含 D6 反馈列）。
#[derive(Debug, Clone)]
pub struct RuleHitView {
    pub id: String,
    pub rule_id: String,
    pub rule_content: String,
    pub status: String,
    pub confidence: Option<f64>,
    pub created_at: String,
    /// 采纳反馈（D6）：helpful / not_relevant / 未反馈 NULL。
    pub feedback: Option<String>,
}
