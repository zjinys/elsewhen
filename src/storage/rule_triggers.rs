//! 规则触发引擎（决策辅助 D1）。
//!
//! 职责链：事件入库时对参与预筛的规则做**确定性关键词匹配**（大小写折叠、
//! 原文包含、无 regex 依赖），命中建 `rule_hits(pending)`；事件分析的 LLM
//! 调用会带上 pending 规则做适用性判定（v3 契约 `rule_confirmations`），
//! 回填为 notified（即时提醒）/ aggregated（每日回顾）/ declined。
//!
//! 硬边界：
//! * 预筛是副产品——挂在事件入库事务内，**任何失败都静默放行**（预警绝不
//!   阻塞捕获）；同一事件重复入库（幂等/重放）命中记录不变。
//! * rule_trigger_settings.mode = 'off' 时不预筛、不建记录；'review_only'
//!   仍预筛但不产生即时提醒（由分析判定落到 aggregated）。
//! * 命中记录只挂本地事件，不外发。

use super::Store;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

pub(crate) const RULE_TRIGGER_MODE_INSTANT: &str = "instant";
pub(crate) const RULE_TRIGGER_MODE_REVIEW_ONLY: &str = "review_only";
pub(crate) const RULE_TRIGGER_MODE_OFF: &str = "off";

/// 即时提醒的置信阈值：分析判定 applicable 且 confidence ≥ 此值才 notified，
/// 其余 applicable 落聚合。坚定保守——误报一次桌面提醒的代价高于漏报。
pub(crate) const RULE_CONFIRM_INSTANT_THRESHOLD: f64 = 0.8;

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

/// Store 包装（FRB API 面使用；内部函数走 conn 参数版本便于事务内复用）。
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

/// 命中记录视图（API DTO 来源）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RuleStatus;
    use crate::storage::Store;

    fn memory_store() -> Store {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-rule-triggers-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Store::open(&path).unwrap()
    }

    fn add_rule_with_keywords(store: &Store, content: &str, keywords: &[&str]) -> String {
        let id = store.add_rule(content, RuleStatus::Active, None).unwrap();
        let keywords_json =
            serde_json::to_string(&keywords.iter().map(|k| k.to_string()).collect::<Vec<_>>())
                .unwrap();
        store
            .connection
            .execute(
                "UPDATE rules SET trigger_keywords=?2 WHERE id=?1",
                params![id, keywords_json],
            )
            .unwrap();
        id
    }

    fn hit_status(store: &Store, rule_id: &str, event_id: &str) -> Option<String> {
        store
            .connection
            .query_row(
                "SELECT status FROM rule_hits WHERE rule_id=?1 AND event_id=?2",
                params![rule_id, event_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .unwrap()
    }

    #[test]
    fn keyword_match_is_case_insensitive_deterministic_and_bad_json_tolerant() {
        let store = memory_store();
        let good = add_rule_with_keywords(&store, "X 项目相关要提醒", &["X项目", "deadline"]);
        // 坏 keywords（数组元素非字符串）：该规则被跳过，预筛整体不崩。
        store
            .connection
            .execute(
                "UPDATE rules SET trigger_keywords='[1,2]' WHERE id=?1",
                params![good],
            )
            .unwrap();
        assert!(
            match_rules_for_text(&store.connection, "X项目")
                .unwrap()
                .is_empty()
        );
        // 恢复正常关键词后命中；大小写折叠；同输入同结果。
        store
            .connection
            .execute(
                "UPDATE rules SET trigger_keywords=?2 WHERE id=?1",
                params![good, r#"["X项目","Deadline"]"#],
            )
            .unwrap();
        let first = match_rules_for_text(&store.connection, "推进 X项目 联调").unwrap();
        assert_eq!(first, vec![good.clone()]);
        assert_eq!(
            first,
            match_rules_for_text(&store.connection, "推进 x项目 联调").unwrap()
        );
    }

    #[test]
    fn screen_is_idempotent_and_respects_off_mode() {
        let store = memory_store();
        let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
        let event_id = store
            .insert_event(crate::event::NewEvent::now("X项目 有了新进展"))
            .unwrap();
        screen_event_on(&store.connection, &event_id, "X项目 有了新进展");
        assert_eq!(
            hit_status(&store, &rule, &event_id).as_deref(),
            Some("pending")
        );
        // 重复入库/重放：仍是同一条 pending，不重复计数。
        screen_event_on(&store.connection, &event_id, "X项目 有了新进展");
        let count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM rule_hits WHERE rule_id=?1 AND event_id=?2",
                params![rule, event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // off 模式：预筛零成本直接放行（不应删已有记录，只是不新增）。
        set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_OFF).unwrap();
        let event2 = store
            .insert_event(crate::event::NewEvent::now("X项目 第二条"))
            .unwrap();
        screen_event_on(&store.connection, &event2, "X项目 第二条");
        assert_eq!(hit_status(&store, &rule, &event2), None);
        set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_INSTANT).unwrap();
    }

    #[test]
    fn confirmations_follow_threshold_and_review_only_mode() {
        let store = memory_store();
        let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
        let e1 = store
            .insert_event(crate::event::NewEvent::now("X项目 A"))
            .unwrap();
        let e2 = store
            .insert_event(crate::event::NewEvent::now("X项目 B"))
            .unwrap();
        let e3 = store
            .insert_event(crate::event::NewEvent::now("X项目 C"))
            .unwrap();
        screen_event_on(&store.connection, &e1, "X项目 A");
        screen_event_on(&store.connection, &e2, "X项目 B");
        screen_event_on(&store.connection, &e3, "X项目 C");

        let conf = |applicable: bool, confidence: f64| RuleConfirmation {
            rule_id: rule.clone(),
            applicable,
            confidence,
        };
        // 高置信 → notified；低置信 → aggregated；不适用 → declined。
        let confirmations = [conf(true, 0.9), conf(true, 0.5), conf(false, 0.9)];
        apply_rule_confirmations_on(&store.connection, &e1, &confirmations[0..1]).unwrap();
        apply_rule_confirmations_on(&store.connection, &e2, &confirmations[1..2]).unwrap();
        apply_rule_confirmations_on(&store.connection, &e3, &confirmations[2..3]).unwrap();
        assert_eq!(hit_status(&store, &rule, &e1).as_deref(), Some("notified"));
        assert_eq!(
            hit_status(&store, &rule, &e2).as_deref(),
            Some("aggregated")
        );
        assert_eq!(hit_status(&store, &rule, &e3).as_deref(), Some("declined"));

        // review_only：applicable 高置信也聚合。
        set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_REVIEW_ONLY).unwrap();
        let e4 = store
            .insert_event(crate::event::NewEvent::now("X项目 D"))
            .unwrap();
        screen_event_on(&store.connection, &e4, "X项目 D");
        apply_rule_confirmations_on(&store.connection, &e4, &confirmations[0..1]).unwrap();
        assert_eq!(
            hit_status(&store, &rule, &e4).as_deref(),
            Some("aggregated")
        );

        // 只处理 pending：已 notified 的记录不被重放改写。
        set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_INSTANT).unwrap();
        apply_rule_confirmations_on(&store.connection, &e1, &confirmations[2..3]).unwrap();
        assert_eq!(hit_status(&store, &rule, &e1).as_deref(), Some("notified"));
    }

    #[test]
    fn cascade_delete_and_event_screen_notification_surface() {
        let store = memory_store();
        let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
        let event_id = store
            .insert_event(crate::event::NewEvent::now("X项目 E"))
            .unwrap();
        screen_event_on(&store.connection, &event_id, "X项目 E");
        assert!(
            pending_hits_for_event(&store.connection, &event_id)
                .unwrap()
                .len()
                == 1
        );
        // 删除规则 → 命中级联消失（外键 ON DELETE CASCADE）。
        store.delete_rule(&rule).unwrap();
        assert!(
            pending_hits_for_event(&store.connection, &event_id)
                .unwrap()
                .is_empty()
        );
    }
}
