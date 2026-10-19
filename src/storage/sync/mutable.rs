//! S05：可变小对象（待办/目标/规则）的复制。
//!
//! 与维基页同框架：整行 payload、head CAS（base_revision）、删除为全端
//! 物理删除、显式冲突不选边。差异点：
//! - 无 slug 身份锚，行身份就是 uuid；无证据依赖（松引用随流补齐）。
//! - 目标有「活跃上限 3」业务触发器；远端写入越限按显式冲突立案
//!   （`goal_active_limit`），不静默顶掉本端目标。
//! - 规则行复制绝不等于执行授权（不变式 4）：应用侧只动 `rules` 表。

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use super::{Apply, Store};
use crate::sync::protocol::{
    GoalRecord, KnowledgeReadingStateRecord, KnowledgeReviewRecord, RuleRecord, SyncMutation,
    SyncOperation, TodoRecord,
};

fn todo_record_from_row(
    id: String,
    title: String,
    status: String,
    priority: String,
    due_at: Option<String>,
    reminder: i64,
    related_event_id: Option<String>,
    related_wiki_slug: Option<String>,
    note: Option<String>,
    created_at: String,
    updated_at: String,
) -> TodoRecord {
    TodoRecord {
        id,
        title,
        status,
        priority,
        due_at,
        reminder: reminder != 0,
        related_event_id,
        related_wiki_slug,
        note,
        created_at,
        updated_at,
    }
}

/// 活跃目标条数上限，与 `goals_cap_active_*` 触发器内常量一致。
const MAX_ACTIVE_GOALS: i64 = 3;

impl Store {
    fn sync_todo_record(&self, id: &str) -> Result<Option<TodoRecord>> {
        self.connection
            .query_row(
                "SELECT id,title,status,priority,due_at,reminder,related_event_id,related_wiki_slug,note,created_at,updated_at \
                 FROM todos WHERE id=?1",
                [id],
                |r| {
                    Ok(todo_record_from_row(
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ))
                },
            )
            .optional()
            .context("sync_todo_record")
    }

    fn sync_goal_record(&self, id: &str) -> Result<Option<GoalRecord>> {
        self.connection
            .query_row(
                "SELECT id,content,phase,status,created_at,updated_at,superseded_at \
                 FROM goals WHERE id=?1",
                [id],
                |r| {
                    Ok(GoalRecord {
                        id: r.get(0)?,
                        content: r.get(1)?,
                        phase: r.get(2)?,
                        status: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                        superseded_at: r.get(6)?,
                    })
                },
            )
            .optional()
            .context("sync_goal_record")
    }

    fn sync_rule_record(&self, id: &str) -> Result<Option<RuleRecord>> {
        self.connection
            .query_row(
                "SELECT id,content,status,conversation_id,created_at,updated_at \
                 FROM rules WHERE id=?1",
                [id],
                |r| {
                    Ok(RuleRecord {
                        id: r.get(0)?,
                        content: r.get(1)?,
                        status: r.get(2)?,
                        conversation_id: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                    })
                },
            )
            .optional()
            .context("sync_rule_record")
    }

    /// 待办捕获点：业务写后同事务调用（调用方必须包 begin_write_transaction）。
    /// 行已物理删除时读不到 → 无法捕获；删除语义用「先捕获后删行」路径。
    pub(crate) fn sync_record_todo(&self, id: &str, deleted: bool) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let todo = self.sync_todo_record(id)?.context("sync_todo_missing")?;
        let base_revision = self.sync_object_head("todo", id)?.map(|h| h.0);
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::TodoChanged {
            todo,
            base_revision,
            deleted,
        })? {
            self.sync_set_head("todo", id, &op_id, deleted)?;
        }
        Ok(())
    }

    /// 目标捕获点（归档/复活/编辑/新建同一入口）。目标没有删除语义，
    /// deleted 恒为 false；归档是状态迁移不是删除。
    pub(crate) fn sync_record_goal(&self, id: &str) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let goal = self.sync_goal_record(id)?.context("sync_goal_missing")?;
        let base_revision = self.sync_object_head("goal", id)?.map(|h| h.0);
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::GoalChanged {
            goal,
            base_revision,
            deleted: false,
        })? {
            self.sync_set_head("goal", id, &op_id, false)?;
        }
        Ok(())
    }

    pub(crate) fn sync_record_rule(&self, id: &str, deleted: bool) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let Some(rule) = self.sync_rule_record(id)? else {
            // 丢弃/删除路径先捕获后删行；到这里读不到说明同事务已删，
            // 无法构造整行——删除捕获必须在删行之前进行。
            anyhow::bail!("sync_rule_missing");
        };
        let base_revision = self.sync_object_head("rule", id)?.map(|h| h.0);
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::RuleChanged {
            rule,
            base_revision,
            deleted,
        })? {
            self.sync_set_head("rule", id, &op_id, deleted)?;
        }
        Ok(())
    }

    // ── 应用端 ─────────────────────────────────────────────────────────

    /// 公共判序。返回 None = 通过 CAS，继续落库；Some(Apply) = 终局结果
    /// （双端同删的幂等 Applied，或显式 Conflict）。判序固定为：
    /// ① head 已删 + 来 op 是删除 → 幂等收敛（两端各自删：先到者删行，
    ///    后到者确认同删，不立案——同删是常见离线场景，不是分歧）；
    /// ② head 已删 + 来 op 是编辑 → object_deleted 候选（优先于 base CAS
    ///    报告，否则原因被 base_revision_changed 掩盖）；
    /// ③ base 未知 → Waiting；base 不匹配 → base_revision_changed。
    fn mutable_cas_guard(
        &self,
        kind: &'static str,
        table: &'static str,
        id: &str,
        base_revision: &Option<String>,
        op_id: &str,
        deleted: bool,
    ) -> Result<Option<Apply>> {
        let head = self.sync_object_head(kind, id)?;
        if head.as_ref().is_some_and(|h| h.1) {
            if deleted {
                self.connection
                    .execute(&format!("DELETE FROM {table} WHERE id=?1"), [id])?;
                self.sync_set_head(kind, id, op_id, true)?;
                return Ok(Some(Apply::Applied));
            }
            let deleted_reason = match kind {
                "todo" => "todo_deleted",
                "goal" => "goal_deleted",
                _ => "rule_deleted",
            };
            return Ok(Some(Apply::Conflict {
                kind,
                id: id.to_string(),
                revision: head.map(|h| h.0),
                reason: deleted_reason,
            }));
        }
        if let Some(base) = base_revision {
            let known: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_operations WHERE op_id=?1 AND state IN ('applied','conflict') UNION ALL SELECT 1 FROM sync_known_revisions WHERE object_kind=?2 AND object_id=?3 AND revision_id=?1)",
                params![base, kind, id],
                |r| r.get(0),
            )?;
            if !known && head.as_ref().map(|h| &h.0) != Some(base) {
                return Ok(Some(Apply::Waiting));
            }
        }
        if head.as_ref().map(|h| h.0.as_str()) != base_revision.as_deref() {
            return Ok(Some(Apply::Conflict {
                kind,
                id: id.to_string(),
                revision: head.map(|h| h.0),
                reason: "base_revision_changed",
            }));
        }
        Ok(None)
    }

    pub(super) fn apply_sync_todo(
        &self,
        op: &SyncOperation,
        todo: &TodoRecord,
        base_revision: &Option<String>,
        deleted: bool,
    ) -> Result<Apply> {
        if let Some(result) =
            self.mutable_cas_guard("todo", "todos", &todo.id, base_revision, &op.op_id, deleted)?
        {
            return Ok(result);
        }
        if deleted {
            self.connection
                .execute("DELETE FROM todos WHERE id=?1", [&todo.id])?;
            self.sync_set_head("todo", &todo.id, &op.op_id, true)?;
            return Ok(Apply::Applied);
        }
        let changed = self.connection.execute(
            "INSERT INTO todos(id,title,status,priority,due_at,reminder,related_event_id,related_wiki_slug,note,created_at,updated_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) \
             ON CONFLICT(id) DO UPDATE SET title=excluded.title,status=excluded.status,priority=excluded.priority, \
             due_at=excluded.due_at,reminder=excluded.reminder,related_event_id=excluded.related_event_id,related_wiki_slug=excluded.related_wiki_slug, \
             note=excluded.note,updated_at=excluded.updated_at",
            params![
                todo.id,
                todo.title,
                todo.status,
                todo.priority,
                todo.due_at,
                todo.reminder as i64,
                todo.related_event_id,
                todo.related_wiki_slug,
                todo.note,
                todo.created_at,
                todo.updated_at
            ],
        )?;
        ensure!(changed == 1, "sync_todo_apply_failed");
        self.sync_set_head("todo", &todo.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }

    pub(super) fn apply_sync_goal(
        &self,
        op: &SyncOperation,
        goal: &GoalRecord,
        base_revision: &Option<String>,
        deleted: bool,
    ) -> Result<Apply> {
        if let Some(result) =
            self.mutable_cas_guard("goal", "goals", &goal.id, base_revision, &op.op_id, deleted)?
        {
            return Ok(result);
        }
        if deleted {
            self.connection
                .execute("DELETE FROM goals WHERE id=?1", [&goal.id])?;
            self.sync_set_head("goal", &goal.id, &op.op_id, true)?;
            return Ok(Apply::Applied);
        }
        // 活跃上限由本端触发器强制：这里先把「远端写入会越限」判成显式
        // 冲突（不静默顶掉本端目标），与或不与触发器等价都必须人工审。
        // 触发器本身兜底（apply 库与业务库同一 schema）。
        if goal.status == "active" {
            let current: Option<String> = self
                .connection
                .query_row("SELECT status FROM goals WHERE id=?1", [&goal.id], |r| {
                    r.get(0)
                })
                .optional()?;
            let turning_active = current.as_deref() != Some("active");
            if turning_active {
                let (count, exists): (i64, bool) = self.connection.query_row(
                    "SELECT (SELECT COUNT(*) FROM goals WHERE status='active'), \
                     EXISTS(SELECT 1 FROM goals WHERE id=?1)",
                    [&goal.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let _ = exists;
                if count >= MAX_ACTIVE_GOALS {
                    return Ok(Apply::Conflict {
                        kind: "goal",
                        id: goal.id.clone(),
                        revision: self.sync_object_head("goal", &goal.id)?.map(|h| h.0),
                        reason: "goal_active_limit",
                    });
                }
            }
        }
        self.connection.execute(
            "INSERT INTO goals(id,content,phase,status,created_at,updated_at,superseded_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7) \
             ON CONFLICT(id) DO UPDATE SET content=excluded.content,phase=excluded.phase,status=excluded.status, \
             updated_at=excluded.updated_at,superseded_at=excluded.superseded_at",
            params![
                goal.id,
                goal.content,
                goal.phase,
                goal.status,
                goal.created_at,
                goal.updated_at,
                goal.superseded_at
            ],
        )?;
        self.sync_set_head("goal", &goal.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }

    pub(super) fn apply_sync_rule(
        &self,
        op: &SyncOperation,
        rule: &RuleRecord,
        base_revision: &Option<String>,
        deleted: bool,
    ) -> Result<Apply> {
        if let Some(result) =
            self.mutable_cas_guard("rule", "rules", &rule.id, base_revision, &op.op_id, deleted)?
        {
            return Ok(result);
        }
        if deleted {
            self.connection
                .execute("DELETE FROM rules WHERE id=?1", [&rule.id])?;
            self.sync_set_head("rule", &rule.id, &op.op_id, true)?;
            return Ok(Apply::Applied);
        }
        self.connection.execute(
            "INSERT INTO rules(id,content,status,conversation_id,created_at,updated_at) \
             VALUES(?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET content=excluded.content,status=excluded.status, \
             conversation_id=excluded.conversation_id,updated_at=excluded.updated_at",
            params![
                rule.id,
                rule.content,
                rule.status,
                rule.conversation_id,
                rule.created_at,
                rule.updated_at
            ],
        )?;
        self.sync_set_head("rule", &rule.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }
}

impl Store {
    // —— knowledge 用户语义状态（切片：maintenance_reviews / reading_states）——
    // 纪律：业务写后同事务捕获；调用方必须包 begin_write_transaction。

    /// 维护评审行回读（dismiss 已 INSERT 后调用）。
    fn sync_knowledge_review_record(
        row: &rusqlite::Row,
    ) -> rusqlite::Result<KnowledgeReviewRecord> {
        Ok(KnowledgeReviewRecord {
            fingerprint: row.get(0)?,
            page_id: row.get(1)?,
            resolution: row.get(2)?,
            created_at: row.get(3)?,
            issue_description: row.get(4)?,
        })
    }

    /// 知识维护评审捕获点。append-only：无 head/CAS，行只随流到达；收端
    /// INSERT OR IGNORE（fingerprint 幂等，重复 op 与双端各自关同一 issue 都
    /// 收敛为一行）。
    pub(crate) fn sync_record_knowledge_review(&self, fingerprint: &str) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let review = self
            .connection
            .query_row(
                "SELECT fingerprint,page_id,resolution,created_at,issue_description \
             FROM knowledge_maintenance_reviews WHERE fingerprint=?1",
                [fingerprint],
                Self::sync_knowledge_review_record,
            )
            .optional()?
            .context("sync_knowledge_review_missing")?;
        self.append_sync_mutation(SyncMutation::KnowledgeReviewAppended { review })?;
        Ok(())
    }

    /// 阅读状态捕获点。整行复制 + updated_at 随行，收端 LWW 决胜（不设 head）。
    pub(crate) fn sync_record_reading_state(&self, page_id: &str) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let state = self
            .connection
            .query_row(
                "SELECT page_id,state,updated_at FROM knowledge_reading_states WHERE page_id=?1",
                [page_id],
                |r| {
                    Ok(KnowledgeReadingStateRecord {
                        page_id: r.get(0)?,
                        state: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                },
            )
            .optional()?
            .context("sync_knowledge_reading_state_missing")?;
        self.append_sync_mutation(SyncMutation::KnowledgeReadingStateChanged { state })?;
        Ok(())
    }

    /// 评审行收端应用：append-only 幂等合并。页面未同步到（op 先于页到达）时
    /// fail-closed 挂起等待，绝不造悬空引用。
    pub(super) fn apply_sync_knowledge_review(
        &self,
        review: &KnowledgeReviewRecord,
    ) -> Result<Apply> {
        let page: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM wiki_pages WHERE id=?1",
                [&review.page_id],
                |r| r.get(0),
            )
            .optional()?;
        ensure!(
            page.as_deref() == Some(review.page_id.as_str()),
            "sync_knowledge_review_orphaned_page"
        );
        self.connection.execute(
            "INSERT OR IGNORE INTO knowledge_maintenance_reviews\
         (fingerprint,page_id,resolution,created_at,issue_description)\
         VALUES(?1,?2,?3,?4,?5)",
            params![
                review.fingerprint,
                review.page_id,
                review.resolution,
                review.created_at,
                review.issue_description
            ],
        )?;
        Ok(Apply::Applied)
    }

    /// 阅读状态收端应用：updated_at 新者胜（重放同一 op 时同值再写，幂等）。
    pub(super) fn apply_sync_knowledge_reading_state(
        &self,
        record: &KnowledgeReadingStateRecord,
    ) -> Result<Apply> {
        let page: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM wiki_pages WHERE id=?1",
                [&record.page_id],
                |r| r.get(0),
            )
            .optional()?;
        ensure!(
            page.as_deref() == Some(record.page_id.as_str()),
            "sync_knowledge_state_orphaned_page"
        );
        self.connection.execute(
            "INSERT INTO knowledge_reading_states(page_id,state,updated_at)\
         VALUES(?1,?2,?3)\
         ON CONFLICT(page_id) DO UPDATE SET state=excluded.state,updated_at=excluded.updated_at \
         WHERE excluded.updated_at > knowledge_reading_states.updated_at",
            params![record.page_id, record.state, record.updated_at],
        )?;
        Ok(Apply::Applied)
    }
}
