//! 工作流记录存取（从 `Store` 抽出）。
//!
//! 规则（rules，pending → active 确认门）、个人待办（todos）、人物关系（relations）、
//! AI 写类工具的待确认动作（pending_actions：草拟 → 用户确认 → 执行/拒绝）、
//! 应用元数据（app_meta）与全量分析列表。

use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::adapter::AnalysisSummary;
use super::{
    PendingAction, Relation, RelationDraft, RuleStatus, RuleSummary, Store, Todo, TodoStatus,
};

impl Store {
pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
    let mut statement = self
        .connection
        .prepare("SELECT value FROM app_meta WHERE key = ?1")?;
    statement
        .query_row(params![key], |row| row.get(0))
        .optional()
        .map_err(Into::into)
}

pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
    self.connection.execute(
        "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 删除 app_meta 键（覆盖层「跟随全局」时清除覆盖值，回落到全局层）
pub fn remove_meta(&self, key: &str) -> Result<()> {
    self.connection
        .execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
    Ok(())
}

pub fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
    let mut statement = self.connection.prepare(
        "SELECT e.raw_text,
                coalesce(json_extract(a.result_json,'$.event_type'),'unknown'),
                coalesce(json_extract(a.result_json,'$.confidence'),0),
                coalesce(json_extract(a.result_json,'$.clarifications'),'[]')
         FROM event_analyses a JOIN events e ON e.id=a.event_id
         ORDER BY a.created_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(AnalysisSummary {
            raw_text: row.get(0)?,
            event_type: row.get(1)?,
            confidence: row.get(2)?,
            clarifications: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

// Rules（个人经验规则库）

/// 列出规则；status 为 None 时返回全部（active + pending）。
/// conversation_id 为 None 时列出全局；为 Some 时只列该会话提出的 pending
/// （v13 前遗留 conversation_id IS NULL 的规则始终包含，保证兼容）。
pub fn list_rules(
    &self,
    status: Option<RuleStatus>,
    conversation_id: Option<&str>,
) -> Result<Vec<RuleSummary>> {
    let mut sql = "SELECT id, content, status, created_at FROM rules".to_string();
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = status {
        conditions.push("status = ?".to_string());
        params.push(Box::new(s.as_str().to_string()));
    }
    if let Some(cid) = conversation_id {
        conditions.push("(conversation_id = ? OR conversation_id IS NULL)".to_string());
        params.push(Box::new(cid.to_string()));
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY created_at ASC");
    let mut statement = self.connection.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = statement.query_map(rusqlite::params_from_iter(param_refs), |row| {
        let status_str: String = row.get(2)?;
        Ok(RuleSummary {
            id: row.get(0)?,
            content: row.get(1)?,
            status: if status_str == "pending" {
                RuleStatus::Pending
            } else {
                RuleStatus::Active
            },
            created_at: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// 已生效的规则（注入 AI 回复上下文用）
pub fn list_active_rules(&self) -> Result<Vec<RuleSummary>> {
    self.list_rules(Some(RuleStatus::Active), None)
}

/// 新增一条规则。
/// conversation_id：来源于哪个会话（pending 确认门按会话隔离用）；None 表示不归属
pub fn add_rule(
    &self,
    content: &str,
    status: RuleStatus,
    conversation_id: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "INSERT INTO rules (id, content, status, conversation_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, content, status.as_str(), conversation_id, now],
    )?;
    Ok(id)
}

/// 把本会话的待确认规则升级为生效（含 v13 前遗留的无归属规则）
pub fn promote_pending_rules(&self, conversation_id: &str) -> Result<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = self.connection.execute(
        "UPDATE rules SET status = 'active', updated_at = ?1
         WHERE status = 'pending'
           AND (conversation_id = ?2 OR conversation_id IS NULL)",
        params![now, conversation_id],
    )?;
    Ok(affected)
}

/// 丢弃本会话的待确认规则（用户明确拒绝；含 v13 前遗留的无归属规则）
pub fn discard_pending_rules(&self, conversation_id: &str) -> Result<usize> {
    let affected = self.connection.execute(
        "DELETE FROM rules
         WHERE status = 'pending'
           AND (conversation_id = ?1 OR conversation_id IS NULL)",
        params![conversation_id],
    )?;
    Ok(affected)
}

/// 删除一条规则；返回是否存在并删除。
pub fn delete_rule(&self, id: &str) -> Result<bool> {
    let affected = self
        .connection
        .execute("DELETE FROM rules WHERE id = ?1", [id])?;
    Ok(affected > 0)
}

/// 新增一条待确认动作（写类工具的草拟阶段）
pub fn create_pending_action(
    &self,
    conversation_id: &str,
    action: &str,
    args_json: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "INSERT INTO pending_actions (id, conversation_id, action, args_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, conversation_id, action, args_json, now],
    )?;
    Ok(id)
}

// ── 个人待办 ────────────────────────────────────────────────────────

pub fn create_todo(
    &self,
    title: &str,
    priority: &str,
    due_at: Option<&str>,
    related_event_id: Option<&str>,
    related_wiki_slug: Option<&str>,
    note: Option<&str>,
) -> Result<Todo> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "INSERT INTO todos (id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at)
         VALUES (?1, ?2, 'open', ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, title, priority, due_at, related_event_id, related_wiki_slug, note, now],
    )?;
    Ok(Todo {
        id,
        title: title.to_string(),
        status: TodoStatus::Open,
        priority: priority.to_string(),
        due_at: due_at.map(|s| s.to_string()),
        related_event_id: related_event_id.map(|s| s.to_string()),
        related_wiki_slug: related_wiki_slug.map(|s| s.to_string()),
        note: note.map(|s| s.to_string()),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_todos(&self, status_filter: Option<&str>) -> Result<Vec<Todo>> {
    let sql = match status_filter {
        Some(_) => "SELECT id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at
                    FROM todos WHERE status = ?1 ORDER BY created_at DESC",
        None => "SELECT id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at
                FROM todos WHERE status != 'archived' ORDER BY status, created_at DESC",
    };
    let mut statement = self.connection.prepare(sql)?;
    let mapper = |row: &rusqlite::Row| -> rusqlite::Result<Todo> {
        Ok(Todo {
            id: row.get(0)?,
            title: row.get(1)?,
            status: TodoStatus::parse(&row.get::<_, String>(2)?),
            priority: row.get(3)?,
            due_at: row.get(4)?,
            related_event_id: row.get(5)?,
            related_wiki_slug: row.get(6)?,
            note: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    };
    let rows = match status_filter {
        Some(s) => statement.query_map(params![s], mapper)?,
        None => statement.query_map([], mapper)?,
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// Fetch a todo by id, including archived history. Used by on-demand
/// work-item migration so archived todos are never silently lost.
pub fn get_todo(&self, id: &str) -> Result<Option<Todo>> {
    // 单行直查：不做两遍全表 list 再内存 find（P2-5）
    let mapper = |row: &rusqlite::Row| -> rusqlite::Result<Todo> {
        Ok(Todo {
            id: row.get(0)?,
            title: row.get(1)?,
            status: TodoStatus::parse(&row.get::<_, String>(2)?),
            priority: row.get(3)?,
            due_at: row.get(4)?,
            related_event_id: row.get(5)?,
            related_wiki_slug: row.get(6)?,
            note: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    };
    self.connection
        .query_row(
            "SELECT id, title, status, priority, due_at, related_event_id,
                    related_wiki_slug, note, created_at, updated_at
             FROM todos WHERE id = ?1",
            [id],
            mapper,
        )
        .optional()
        .map_err(Into::into)
}

pub fn update_todo_status(&self, id: &str, status: TodoStatus) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
        params![status.as_str(), now, id],
    )?;
    Ok(())
}

pub fn set_todo_related_wiki_slug(&self, id: &str, slug: &str) -> Result<()> {
    let changed = self.connection.execute(
        "UPDATE todos SET related_wiki_slug=?1, updated_at=?2 WHERE id=?3",
        params![slug, chrono::Utc::now().to_rfc3339(), id],
    )?;
    if changed == 0 {
        anyhow::bail!("未找到待办（id={id}）");
    }
    Ok(())
}

/// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
/// 状态与关联字段保持不变；note/due_at 传 None 表示清除；
/// priority 传 None 时回落到默认值 "normal"（列 NOT NULL）。
pub fn update_todo(
    &self,
    id: &str,
    title: &str,
    note: Option<&str>,
    priority: Option<&str>,
    due_at: Option<&str>,
) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        anyhow::bail!("待办内容不能为空");
    }
    let priority = priority.unwrap_or("normal");
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "UPDATE todos SET title=?1, note=?2, priority=?3, due_at=?4, updated_at=?5 WHERE id=?6",
        params![title, note, priority, due_at, now, id],
    )?;
    Ok(())
}

pub fn delete_todo(&self, id: &str) -> Result<bool> {
    let n = self
        .connection
        .execute("DELETE FROM todos WHERE id=?1", [id])?;
    Ok(n > 0)
}

// ── 人物关系 ────────────────────────────────────────────────────────

/// 新建或刷新一条人物关系（(from, to, relation) 唯一，重复则更新 note / 时间戳）。
pub fn upsert_relation(&self, draft: &RelationDraft) -> Result<Relation> {
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "INSERT INTO relations
           (id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
            source_conversation_id, source_event_id, created_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(from_slug, to_slug, relation) DO UPDATE SET
           note = ?7, confidence = ?8, source_event_id = COALESCE(?10, relations.source_event_id), last_seen_at = ?11",
        params![
            Uuid::new_v4().to_string(),
            draft.from_slug,
            draft.from_kind,
            draft.to_slug,
            draft.to_kind,
            draft.relation,
            draft.note,
            draft.confidence,
            draft.source_conversation_id,
            draft.source_event_id,
            now.clone(),
        ],
    )?;
    // 读回真实行（拿到真实 id）
    let mut statement = self.connection.prepare(
        "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                created_at, last_seen_at
         FROM relations
         WHERE from_slug = ?1 AND to_slug = ?2 AND relation = ?3",
    )?;
    let rel = statement
        .query_row(
            params![draft.from_slug, draft.to_slug, draft.relation],
            Self::map_relation,
        )
        .optional()?
        .unwrap_or(Relation {
            id: String::new(),
            from_slug: draft.from_slug.clone(),
            from_kind: draft.from_kind.clone(),
            to_slug: draft.to_slug.clone(),
            to_kind: draft.to_kind.clone(),
            relation: draft.relation.clone(),
            note: draft.note.clone(),
            confidence: draft.confidence,
            created_at: now.clone(),
            last_seen_at: now,
        });
    Ok(rel)
}

fn map_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
    Ok(Relation {
        id: row.get(0)?,
        from_slug: row.get(1)?,
        from_kind: row.get(2)?,
        to_slug: row.get(3)?,
        to_kind: row.get(4)?,
        relation: row.get(5)?,
        note: row.get(6)?,
        confidence: row.get(7)?,
        created_at: row.get(8)?,
        last_seen_at: row.get(9)?,
    })
}

/// 与某页相关的关系（双向：作为人物方或作为事情/项目方）
pub fn list_relations_for_page(&self, slug: &str) -> Result<Vec<Relation>> {
    let mut statement = self.connection.prepare(
        "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                created_at, last_seen_at
         FROM relations WHERE from_slug = ?1 OR to_slug = ?1
         ORDER BY last_seen_at DESC",
    )?;
    let rows = statement
        .query_map(params![slug], Self::map_relation)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 全部人物关系（备用：未来人物视图用）
pub fn list_relations(&self) -> Result<Vec<Relation>> {
    let mut statement = self.connection.prepare(
        "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                created_at, last_seen_at
         FROM relations ORDER BY last_seen_at DESC",
    )?;
    let rows = statement
        .query_map([], Self::map_relation)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_relation(&self, id: &str) -> Result<bool> {
    let n = self
        .connection
        .execute("DELETE FROM relations WHERE id=?1", [id])?;
    Ok(n > 0)
}

// ── pending actions (continued) ─────────────────────────────────────
/// 列出某个对话里待确认的动作（按时间先后）
pub fn pending_actions_for_conversation(
    &self,
    conversation_id: &str,
) -> Result<Vec<PendingAction>> {
    let mut statement = self.connection.prepare(
        "SELECT id, conversation_id, action, args_json, created_at
         FROM pending_actions
         WHERE conversation_id = ?1 AND status = 'pending'
         ORDER BY created_at ASC",
    )?;
    let rows = statement
        .query_map(params![conversation_id], |row| {
            Ok(PendingAction {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                action: row.get(2)?,
                args_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 判断本会话是否已有同一动作、同一标题的待确认草稿。
/// 标题是知识页创建的业务幂等键；正文允许在确认前继续完善，但不应因此产生第二个页面。
pub fn pending_action_for_title(
    &self,
    conversation_id: &str,
    action: &str,
    title: &str,
) -> Result<Option<PendingAction>> {
    let actions = self.pending_actions_for_conversation(conversation_id)?;
    Ok(actions.into_iter().find(|pa| {
        if pa.action != action {
            return false;
        }
        serde_json::from_str::<serde_json::Value>(&pa.args_json)
            .ok()
            .and_then(|args| args.get("title").and_then(|v| v.as_str()).map(str::trim).map(String::from))
            .is_some_and(|pending_title| pending_title.eq_ignore_ascii_case(title.trim()))
    }))
}

pub fn pending_action_by_id(
    &self,
    conversation_id: &str,
    id: &str,
) -> Result<Option<PendingAction>> {
    Ok(self.pending_actions_for_conversation(conversation_id)?
        .into_iter()
        .find(|action| action.id == id))
}

pub fn action_exists_for_event(
    &self,
    conversation_id: &str,
    action: &str,
    event_id: &str,
) -> Result<bool> {
    let needle = format!("%{}%", event_id);
    Ok(self.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pending_actions WHERE conversation_id=?1 AND action=?2 AND args_json LIKE ?3)",
        params![conversation_id, action, needle], |row| row.get(0))?)
}

/// 删除一条待确认动作（执行完或用户拒绝后清理）
pub fn delete_pending_action(&self, id: &str) -> Result<()> {
    self.connection
        .execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
    Ok(())
}

pub fn update_pending_action_args(&self, id: &str, args_json: &str) -> Result<bool> {
    Ok(self.connection.execute(
        "UPDATE pending_actions SET args_json=?1 WHERE id=?2 AND status='pending'",
        params![args_json, id],
    )? > 0)
}

pub fn decline_pending_action(&self, id: &str) -> Result<bool> {
    Ok(self.connection.execute(
        "UPDATE pending_actions SET status='declined' WHERE id=?1 AND status='pending'",
        [id],
    )? > 0)
}

/// 清空某个对话的全部待确认动作，返回删除条数
pub fn delete_pending_actions_for_conversation(&self, conversation_id: &str) -> Result<usize> {
    let affected = self.connection.execute(
        "DELETE FROM pending_actions WHERE conversation_id = ?1",
        params![conversation_id],
    )?;
    Ok(affected)
}
}
