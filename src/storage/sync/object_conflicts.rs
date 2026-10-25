//! S05：可变小对象（待办/目标/规则）同步冲突审阅。
//!
//! 通用化 wiki 页审阅框架（page_conflicts.rs）：候选 payload 原文留在
//! `sync_operations`（state='conflict'），`sync_conflicts` 只是立案索引。
//! 候选 reason：`todo_deleted` / `goal_deleted` / `rule_deleted`（删除后
//! 离线编辑）、`base_revision_changed`（双端并发编辑）、`goal_active_limit`
//! （远端写入越活跃上限）。审阅把双端状态并列展示后由人显式裁决：
//! 「保留本端」= 重新捕获本地现状并广播（对端看到反向候选，对称收敛）；
//! 「采纳远端」= 按候选 op 语义强制落库整行。不做字段拼接，不比时间；
//! 双绑定失败关闭。`goal_active_limit` 的采纳被显式拒绝（活跃上限是业务
//! 不变式，触发器兜底），只能保留本端或先归档本端目标后等待重放。

use anyhow::{Result, anyhow, ensure};
use rusqlite::OptionalExtension;
use rusqlite::params;

use super::Store;
use crate::sync::protocol::{SyncMutation, SyncOperation};

/// 候选中远端一方的记录快照（供审阅 UI 并列展示；summary 是业务主文本：
/// 待办标题 / 目标内容 / 规则内容）。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ObjectConflictIncoming {
    pub summary: String,
    pub status: String,
    pub deleted: bool,
    pub base_revision: Option<String>,
    pub updated_at: String,
}

/// 一条待审阅的可变小对象同步冲突（本端行缺失 → local 字段为 None）。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ObjectConflictReview {
    pub op_id: String,
    pub object_kind: String,
    pub object_id: String,
    pub reason: String,
    pub sender_device_id: String,
    pub incoming: ObjectConflictIncoming,
    pub local_summary: Option<String>,
    pub local_status: Option<String>,
    pub local_updated_at: Option<String>,
    /// 本端 head 是否处于已删状态（删除后离线编辑场景的本端侧佐证）。
    pub local_head_deleted: bool,
}

struct ConflictRow {
    op_id: String,
    object_kind: String,
    object_id: String,
    reason: String,
    sender_device_id: String,
    payload: String,
}

fn mutation_incoming(mutation: &SyncMutation) -> Result<ObjectConflictIncoming> {
    Ok(match mutation {
        SyncMutation::TodoChanged {
            todo,
            base_revision,
            deleted,
        } => ObjectConflictIncoming {
            summary: todo.title.clone(),
            status: todo.status.clone(),
            deleted: *deleted,
            base_revision: base_revision.clone(),
            updated_at: todo.updated_at.clone(),
        },
        SyncMutation::HabitNudgeChanged {
            nudge,
            base_revision,
            deleted,
        } => ObjectConflictIncoming {
            summary: nudge.question.clone(),
            status: if nudge.enabled { "enabled" } else { "disabled" }.to_string(),
            deleted: *deleted,
            base_revision: base_revision.clone(),
            updated_at: nudge.updated_at.clone(),
        },
        SyncMutation::HabitPageLinkChanged {
            link,
            base_revision,
            deleted,
        } => ObjectConflictIncoming {
            summary: link.page_slug.clone(),
            status: link.status.clone(),
            deleted: *deleted,
            base_revision: base_revision.clone(),
            updated_at: link.updated_at.clone(),
        },
        SyncMutation::GoalChanged {
            goal,
            base_revision,
            ..
        } => ObjectConflictIncoming {
            summary: goal.content.clone(),
            status: goal.status.clone(),
            deleted: false,
            base_revision: base_revision.clone(),
            updated_at: goal.updated_at.clone(),
        },
        SyncMutation::RuleChanged {
            rule,
            base_revision,
            deleted,
        } => ObjectConflictIncoming {
            summary: rule.content.clone(),
            status: rule.status.clone(),
            deleted: *deleted,
            base_revision: base_revision.clone(),
            updated_at: rule.updated_at.clone(),
        },
        _ => return Err(anyhow!("sync_object_conflict_unexpected")),
    })
}

impl Store {
    /// 列出待审阅的待办/目标/规则同步冲突（object_kind IN todo/goal/rule，
    /// 按流内序）。
    pub fn list_object_conflicts(&self) -> Result<Vec<ObjectConflictReview>> {
        let rows: Vec<ConflictRow> = {
            let mut statement = self.connection.prepare(
                "SELECT c.op_id,c.object_kind,c.object_id,c.reason,o.origin_device_id,o.payload \
                 FROM sync_conflicts c JOIN sync_operations o ON o.op_id=c.op_id \
                 WHERE c.object_kind IN ('todo','habit_nudge','goal','rule') AND o.state='conflict' \
                 ORDER BY o.origin_stream_id,o.origin_sequence",
            )?;
            statement
                .query_map([], |r| {
                    Ok(ConflictRow {
                        op_id: r.get(0)?,
                        object_kind: r.get(1)?,
                        object_id: r.get(2)?,
                        reason: r.get(3)?,
                        sender_device_id: r.get(4)?,
                        payload: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut reviews = Vec::with_capacity(rows.len());
        for row in rows {
            let op = SyncOperation::decode(row.payload.as_bytes())?;
            let incoming = mutation_incoming(&op.mutation)?;
            let head = self.sync_object_head(&row.object_kind, &row.object_id)?;
            // query_row 闭包内不用 ?：先读裸值出闭包再组装（惯例）。
            let local: Option<(String, String, String)> = match row.object_kind.as_str() {
                "todo" => self
                    .connection
                    .query_row(
                        "SELECT title,status,updated_at FROM todos WHERE id=?1",
                        [&row.object_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?,
                "habit_nudge" => self
                    .connection
                    .query_row(
                        "SELECT question,CASE WHEN enabled=1 THEN 'enabled' ELSE 'disabled' END,updated_at FROM habit_nudges WHERE id=?1",
                        [&row.object_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?,
                "goal" => self
                    .connection
                    .query_row(
                        "SELECT content,status,updated_at FROM goals WHERE id=?1",
                        [&row.object_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?,
                _ => self
                    .connection
                    .query_row(
                        "SELECT content,status,updated_at FROM rules WHERE id=?1",
                        [&row.object_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?,
            };
            reviews.push(ObjectConflictReview {
                op_id: row.op_id,
                object_kind: row.object_kind,
                object_id: row.object_id,
                reason: row.reason,
                sender_device_id: row.sender_device_id,
                incoming,
                local_summary: local.as_ref().map(|l| l.0.clone()),
                local_status: local.as_ref().map(|l| l.1.clone()),
                local_updated_at: local.map(|l| l.2),
                local_head_deleted: head.as_ref().is_some_and(|h| h.1),
            });
        }
        Ok(reviews)
    }

    /// 显式裁决一条可变小对象冲突。`expected_local_updated_at` 是审阅时展示
    /// 的本端行 updated_at（行已物理删除传 None）——裁决时必须原样回传，
    /// 任一变化失败关闭；op 状态保持 'conflict'（durable 标记）。
    pub fn resolve_object_conflict(
        &self,
        op_id: &str,
        expected_local_updated_at: Option<&str>,
        accept_remote: bool,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        ensure!(!op_id.trim().is_empty(), "object_conflict_op_invalid");
        let (kind, object_id, reason, payload) = self
            .connection
            .query_row(
                "SELECT c.object_kind,c.object_id,c.reason,o.payload \
                 FROM sync_conflicts c JOIN sync_operations o ON o.op_id=c.op_id \
                 WHERE c.op_id=?1 AND c.object_kind IN ('todo','habit_nudge','goal','rule') \
                   AND o.state='conflict'",
                [op_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| anyhow!("object_conflict_not_pending"))?;
        let op = SyncOperation::decode(payload.as_bytes())?;
        // 本端现状绑定：与审阅时展示的行一致才允许裁决。
        let table = match kind.as_str() {
            "todo" => "todos",
            "habit_nudge" => "habit_nudges",
            "goal" => "goals",
            _ => "rules",
        };
        let local_now: Option<String> = self
            .connection
            .query_row(
                &format!("SELECT updated_at FROM {table} WHERE id=?1"),
                [&object_id],
                |r| r.get(0),
            )
            .optional()?;
        if local_now.as_deref() != expected_local_updated_at {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Err(anyhow!(
                "object_conflict_local_changed（本端记录已变化，请刷新后重新审阅）"
            ));
        }
        match (&op.mutation, accept_remote) {
            (_, false) => {
                // 保留本端 = 本端现状为准：head 已删则删除态就是本端裁决
                // （行已物理删除，无从重捕）；head 活跃则重新捕获并广播——
                // 对端会出现反向候选，由对端各自裁决（对称收敛）。
                let head = self.sync_object_head(&kind, &object_id)?;
                let row_present: bool = self.connection.query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
                    [&object_id],
                    |r| r.get(0),
                )?;
                if !head.as_ref().is_some_and(|h| h.1) && row_present {
                    match kind.as_str() {
                        "todo" => self.sync_record_todo(&object_id, false)?,
                        "habit_nudge" => self.sync_record_habit_nudge(&object_id, false)?,
                        "goal" => self.sync_record_goal(&object_id)?,
                        _ => self.sync_record_rule(&object_id, false)?,
                    }
                }
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::GoalChanged { .. }, true) if reason == "goal_active_limit" => {
                // 活跃上限是业务不变式：直接采纳会顶掉本端第 N 个活跃目标，
                // 显式拒绝（触发器也会兜底）。要求先归档本端目标或保留本端。
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Err(anyhow!(
                    "goal_active_limit（本端活跃目标已满，无法直接采纳对端；请先归档本端目标或选择保留本端）"
                ));
            }
            (SyncMutation::TodoChanged { todo, deleted, .. }, true) => {
                if *deleted {
                    // 采纳远端删除：物理删除（行可已不存在，删除幂等）。
                    self.connection
                        .execute("DELETE FROM todos WHERE id=?1", [&todo.id])?;
                } else {
                    // 采纳远端编辑：整行采纳；行已删除则复活（未删态）。
                    self.connection.execute(
                        "INSERT INTO todos(id,title,status,priority,due_at,reminder,related_event_id,related_wiki_slug,note,created_at,updated_at) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) \
                         ON CONFLICT(id) DO UPDATE SET title=excluded.title,status=excluded.status,priority=excluded.priority, \
                         due_at=excluded.due_at,reminder=excluded.reminder,related_event_id=excluded.related_event_id,related_wiki_slug=excluded.related_wiki_slug, \
                         note=excluded.note,updated_at=excluded.updated_at",
                        params![todo.id, todo.title, todo.status, todo.priority, todo.due_at,
                                todo.reminder as i64, todo.related_event_id, todo.related_wiki_slug, todo.note,
                                todo.created_at, todo.updated_at],
                    )?;
                }
                self.sync_set_head("todo", &todo.id, &op.op_id, *deleted)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::GoalChanged { goal, .. }, true) => {
                self.connection.execute(
                    "INSERT INTO goals(id,content,phase,status,created_at,updated_at,superseded_at) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7) \
                     ON CONFLICT(id) DO UPDATE SET content=excluded.content,phase=excluded.phase,status=excluded.status, \
                     updated_at=excluded.updated_at,superseded_at=excluded.superseded_at",
                    params![goal.id, goal.content, goal.phase, goal.status,
                            goal.created_at, goal.updated_at, goal.superseded_at],
                )?;
                self.sync_set_head("goal", &goal.id, &op.op_id, false)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::HabitNudgeChanged { nudge, deleted, .. }, true) => {
                if *deleted {
                    self.connection
                        .execute("DELETE FROM habit_nudges WHERE id=?1", [&nudge.id])?;
                    self.connection.execute(
                        "DELETE FROM habit_nudge_fires WHERE nudge_id=?1",
                        [&nudge.id],
                    )?;
                } else {
                    self.connection.execute(
                        "INSERT INTO habit_nudges(id,page_slug,question,done_label,done_text,missed_label,missed_text,weekdays,local_time,enabled,created_at,updated_at) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
                         ON CONFLICT(id) DO UPDATE SET page_slug=excluded.page_slug,question=excluded.question, \
                         done_label=excluded.done_label,done_text=excluded.done_text,missed_label=excluded.missed_label, \
                         missed_text=excluded.missed_text,weekdays=excluded.weekdays,local_time=excluded.local_time, \
                         enabled=excluded.enabled,updated_at=excluded.updated_at",
                        params![
                            nudge.id,
                            nudge.page_slug,
                            nudge.question,
                            nudge.done_label,
                            nudge.done_text,
                            nudge.missed_label,
                            nudge.missed_text,
                            nudge.weekdays,
                            nudge.local_time,
                            nudge.enabled as i64,
                            nudge.created_at,
                            nudge.updated_at
                        ],
                    )?;
                }
                self.sync_set_head("habit_nudge", &nudge.id, &op.op_id, *deleted)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::HabitPageLinkChanged { link, deleted, .. }, true) => {
                if *deleted {
                    self.connection
                        .execute("DELETE FROM habit_page_links WHERE id=?1", [&link.id])?;
                } else {
                    self.connection.execute(
                        "INSERT INTO habit_page_links(id,page_slug,nudge_id,status,source,created_at,updated_at) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7) \
                         ON CONFLICT(id) DO UPDATE SET page_slug=excluded.page_slug,nudge_id=excluded.nudge_id, \
                         status=excluded.status,source=excluded.source,updated_at=excluded.updated_at",
                        params![
                            link.id,
                            link.page_slug,
                            link.nudge_id,
                            link.status,
                            link.source,
                            link.created_at,
                            link.updated_at
                        ],
                    )?;
                }
                self.sync_set_head("habit_page_link", &link.id, &op.op_id, *deleted)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::RuleChanged { rule, deleted, .. }, true) => {
                if *deleted {
                    self.connection
                        .execute("DELETE FROM rules WHERE id=?1", [&rule.id])?;
                } else {
                    // 规则行复制不是执行授权（不变式 4）：只动 rules 表。
                    self.connection.execute(
                        "INSERT INTO rules(id,content,status,conversation_id,essence,created_at,updated_at) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7) \
                         ON CONFLICT(id) DO UPDATE SET content=excluded.content,status=excluded.status, \
                         conversation_id=excluded.conversation_id,essence=excluded.essence,updated_at=excluded.updated_at",
                        params![rule.id, rule.content, rule.status, rule.conversation_id,
                                rule.essence, rule.created_at, rule.updated_at],
                    )?;
                }
                self.sync_set_head("rule", &rule.id, &op.op_id, *deleted)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            _ => {
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Err(anyhow!("sync_object_conflict_unexpected"));
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}
