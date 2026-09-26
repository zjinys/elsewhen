//! 人物 / 项目 / 主题实体层存取（从 `Store` 抽出）。
//!
//! 最小结构化事实（entity_facts，来源事件不可省略）、别名（entity_aliases）、
//! 合并与可撤销合并（entity_merges + entity_merge_snapshots 快照，undo 按快照回滚）。

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::{EntityFact, EntityMergeStatus, Store};

impl Store {
pub fn upsert_entity_fact(
    &self,
    entity_kind: &str,
    entity_slug: &str,
    fact_text: &str,
    occurred_at: &str,
    confidence: i64,
    source_event_id: &str,
) -> Result<EntityFact> {
    if !matches!(entity_kind, "person" | "project" | "topic") {
        anyhow::bail!("非法实体类型: {entity_kind}");
    }
    if entity_slug.trim().is_empty() || fact_text.trim().is_empty() {
        anyhow::bail!("实体标识和事实内容不能为空");
    }
    if !(0..=5).contains(&confidence) {
        anyhow::bail!("事实置信度必须在 0..5");
    }
    let now = chrono::Utc::now().to_rfc3339();
    self.connection.execute(
        "INSERT INTO entity_facts
         (id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)
         ON CONFLICT(entity_kind,entity_slug,fact_text,source_event_id) DO UPDATE SET
           confidence=MAX(entity_facts.confidence, excluded.confidence),
           last_seen_at=excluded.last_seen_at",
        params![Uuid::new_v4().to_string(), entity_kind, entity_slug.trim(), fact_text.trim(), occurred_at, confidence, source_event_id, now],
    )?;
    self.connection
        .query_row(
            "SELECT id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at
             FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 AND fact_text=?3 AND source_event_id=?4",
            params![entity_kind, entity_slug.trim(), fact_text.trim(), source_event_id],
            |row| Ok(EntityFact { id: row.get(0)?, entity_kind: row.get(1)?, entity_slug: row.get(2)?, fact_text: row.get(3)?, occurred_at: row.get(4)?, confidence: row.get(5)?, source_event_id: row.get(6)?, created_at: row.get(7)?, last_seen_at: row.get(8)? }),
        )
        .map_err(Into::into)
}

pub fn list_entity_facts(
    &self,
    entity_kind: &str,
    entity_slug: &str,
) -> Result<Vec<EntityFact>> {
    let mut statement = self.connection.prepare(
        "SELECT id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at
         FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 ORDER BY occurred_at DESC, id DESC",
    )?;
    let rows = statement.query_map(params![entity_kind, entity_slug], |row| {
        Ok(EntityFact {
            id: row.get(0)?,
            entity_kind: row.get(1)?,
            entity_slug: row.get(2)?,
            fact_text: row.get(3)?,
            occurred_at: row.get(4)?,
            confidence: row.get(5)?,
            source_event_id: row.get(6)?,
            created_at: row.get(7)?,
            last_seen_at: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn delete_entity_fact(&self, id: &str) -> Result<bool> {
    Ok(self
        .connection
        .execute("DELETE FROM entity_facts WHERE id=?1", [id])?
        > 0)
}

pub fn list_entity_aliases(&self, entity_kind: &str, entity_slug: &str) -> Result<Vec<String>> {
    let mut statement = self.connection.prepare("SELECT alias FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2 ORDER BY alias")?;
    let rows = statement.query_map(params![entity_kind, entity_slug], |row| row.get(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn add_entity_alias(
    &self,
    entity_kind: &str,
    entity_slug: &str,
    alias: &str,
) -> Result<()> {
    let alias = alias.trim();
    if alias.is_empty() {
        anyhow::bail!("别名不能为空");
    }
    self.connection.execute("INSERT OR IGNORE INTO entity_aliases (id,entity_kind,entity_slug,alias,created_at) VALUES (?1,?2,?3,?4,?5)", params![Uuid::new_v4().to_string(), entity_kind, entity_slug, alias, chrono::Utc::now().to_rfc3339()])?;
    Ok(())
}

/// 合并两个已确认的实体。原始事件不变，来源实体保留合并记录。
pub fn merge_entity(
    &self,
    entity_kind: &str,
    source_slug: &str,
    target_slug: &str,
) -> Result<bool> {
    if !matches!(entity_kind, "person" | "project" | "topic")
        || source_slug.trim().is_empty()
        || target_slug.trim().is_empty()
        || source_slug == target_slug
    {
        anyhow::bail!("实体合并参数无效");
    }
    let source = self.get_wiki_page(source_slug)?.context("来源实体不存在")?;
    let target = self.get_wiki_page(target_slug)?.context("目标实体不存在")?;
    if source.kind != entity_kind || target.kind != entity_kind {
        anyhow::bail!("来源和目标必须是同一种实体类型");
    }
    if source.status == "merged" || target.status == "merged" {
        anyhow::bail!("已合并实体不能再次合并");
    }
    if self.connection.query_row("SELECT EXISTS(SELECT 1 FROM entity_merges WHERE entity_kind=?1 AND source_slug=?2 AND undone_at IS NULL)", params![entity_kind, source_slug], |r| r.get::<_, bool>(0))? {
        anyhow::bail!("该实体已经有合并记录");
    }
    let tx = self.connection.unchecked_transaction()?;
    let now = chrono::Utc::now().to_rfc3339();
    let merge_id = Uuid::new_v4().to_string();
    tx.execute("INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at,undone_at) VALUES (?1,?2,?3,?4,?5,NULL)", params![merge_id, entity_kind, source_slug, target_slug, now])?;
    let mut facts = tx.prepare("SELECT id,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2")?;
    for row in facts.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_, String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"fact_text":r.get::<_,String>(1)?,"occurred_at":r.get::<_,String>(2)?,"confidence":r.get::<_,i64>(3)?,"source_event_id":r.get::<_,String>(4)?,"created_at":r.get::<_,String>(5)?,"last_seen_at":r.get::<_,String>(6)?}))))? {
        let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'entity_facts',?2,?3)", params![merge_id, id, payload.to_string()])?;
    }
    let mut aliases = tx.prepare("SELECT id,alias,created_at FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2")?;
    for row in aliases.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"alias":r.get::<_,String>(1)?,"created_at":r.get::<_,String>(2)?}))))? {
        let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'entity_aliases',?2,?3)", params![merge_id, id, payload.to_string()])?;
    }
    let mut relations = tx.prepare("SELECT id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,source_conversation_id,source_event_id,created_at,last_seen_at FROM relations WHERE (from_kind=?1 AND from_slug=?2) OR (to_kind=?1 AND to_slug=?2)")?;
    for row in relations.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"from_slug":r.get::<_,String>(1)?,"from_kind":r.get::<_,String>(2)?,"to_slug":r.get::<_,String>(3)?,"to_kind":r.get::<_,String>(4)?,"relation":r.get::<_,String>(5)?,"note":r.get::<_,Option<String>>(6)?,"confidence":r.get::<_,i64>(7)?,"source_conversation_id":r.get::<_,Option<String>>(8)?,"source_event_id":r.get::<_,Option<String>>(9)?,"created_at":r.get::<_,String>(10)?,"last_seen_at":r.get::<_,String>(11)?}))))? {
        let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'relations',?2,?3)", params![merge_id, id, payload.to_string()])?;
    }
    let mut todos = tx.prepare("SELECT id,title,status,priority,due_at,related_event_id,note,created_at,updated_at FROM todos WHERE related_wiki_slug=?1")?;
    for row in todos.query_map([source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"priority":r.get::<_,String>(3)?,"due_at":r.get::<_,Option<String>>(4)?,"related_event_id":r.get::<_,Option<String>>(5)?,"note":r.get::<_,Option<String>>(6)?,"created_at":r.get::<_,String>(7)?,"updated_at":r.get::<_,String>(8)?}))))? {
        let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'todos',?2,?3)", params![merge_id,id,payload.to_string()])?;
    }
    drop(facts);
    drop(aliases);
    drop(relations);
    drop(todos);
    tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='entity_facts' AND row_id IN (SELECT s.id FROM entity_facts s WHERE s.entity_kind=?2 AND s.entity_slug=?3 AND EXISTS (SELECT 1 FROM entity_facts t WHERE t.entity_kind=?2 AND t.entity_slug=?4 AND t.fact_text=s.fact_text AND t.source_event_id=s.source_event_id))", params![merge_id, entity_kind, source_slug, target_slug])?;
    tx.execute("DELETE FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 AND EXISTS (SELECT 1 FROM entity_facts t WHERE t.entity_kind=?1 AND t.entity_slug=?3 AND t.fact_text=entity_facts.fact_text AND t.source_event_id=entity_facts.source_event_id)", params![entity_kind, source_slug, target_slug])?;
    tx.execute(
        "UPDATE entity_facts SET entity_slug=?1 WHERE entity_kind=?2 AND entity_slug=?3",
        params![target_slug, entity_kind, source_slug],
    )?;
    tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='entity_aliases' AND row_id IN (SELECT s.id FROM entity_aliases s WHERE s.entity_kind=?2 AND s.entity_slug=?3 AND EXISTS (SELECT 1 FROM entity_aliases t WHERE t.entity_kind=?2 AND t.entity_slug=?4 AND t.alias=s.alias))", params![merge_id, entity_kind, source_slug, target_slug])?;
    tx.execute("DELETE FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2 AND EXISTS (SELECT 1 FROM entity_aliases t WHERE t.entity_kind=?1 AND t.entity_slug=?3 AND t.alias=entity_aliases.alias)", params![entity_kind, source_slug, target_slug])?;
    tx.execute(
        "UPDATE entity_aliases SET entity_slug=?1 WHERE entity_kind=?2 AND entity_slug=?3",
        params![target_slug, entity_kind, source_slug],
    )?;
    tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='relations' AND row_id IN (SELECT s.id FROM relations s WHERE ((s.from_kind=?2 AND s.from_slug=?3) OR (s.to_kind=?2 AND s.to_slug=?3)) AND EXISTS (SELECT 1 FROM relations t WHERE t.id<>s.id AND t.from_slug=CASE WHEN s.from_kind=?2 AND s.from_slug=?3 THEN ?4 ELSE s.from_slug END AND t.to_slug=CASE WHEN s.to_kind=?2 AND s.to_slug=?3 THEN ?4 ELSE s.to_slug END AND t.relation=s.relation))", params![merge_id, entity_kind, source_slug, target_slug])?;
    tx.execute("DELETE FROM relations WHERE id IN (SELECT row_id FROM entity_merge_snapshots WHERE merge_id=?1 AND table_name='relations' AND disposition='deduplicated')", [&merge_id])?;
    tx.execute("UPDATE relations SET from_slug=CASE WHEN from_kind=?1 AND from_slug=?2 THEN ?3 ELSE from_slug END, to_slug=CASE WHEN to_kind=?1 AND to_slug=?2 THEN ?3 ELSE to_slug END WHERE (from_kind=?1 AND from_slug=?2) OR (to_kind=?1 AND to_slug=?2)", params![entity_kind, source_slug, target_slug])?;
    tx.execute(
        "UPDATE todos SET related_wiki_slug=?1 WHERE related_wiki_slug=?2",
        params![target_slug, source_slug],
    )?;
    tx.execute(
        "UPDATE wiki_pages SET status='merged', updated_at=?1 WHERE slug=?2",
        params![now, source_slug],
    )?;
    tx.commit()?;
    Ok(true)
}

pub fn undo_entity_merge(&self, source_slug: &str) -> Result<bool> {
    let (merge_id, kind, target): (String, String, String) = self.connection.query_row(
        "SELECT id,entity_kind,target_slug FROM entity_merges WHERE source_slug=?1 AND undone_at IS NULL ORDER BY created_at DESC LIMIT 1",
        [source_slug],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).optional()?.context("找不到可撤销的合并记录")?;
    let tx = self.connection.unchecked_transaction()?;
    let status: String = tx.query_row(
        "SELECT status FROM wiki_pages WHERE slug=?1",
        [source_slug],
        |r| r.get(0),
    )?;
    if status != "merged" {
        anyhow::bail!("来源实体状态已变化，不能安全撤销");
    }
    let snapshots = {
        let mut statement = tx.prepare("SELECT table_name,row_id,disposition,payload FROM entity_merge_snapshots WHERE merge_id=?1 ORDER BY table_name,row_id")?;
        let rows = statement
            .query_map([&merge_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (table, row_id, disposition, payload) in snapshots {
        let value: serde_json::Value = serde_json::from_str(&payload)?;
        let text = |key: &str| -> Result<String> {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .with_context(|| format!("合并快照缺少 {key}"))
        };
        let number = |key: &str| -> Result<i64> {
            value
                .get(key)
                .and_then(|v| v.as_i64())
                .with_context(|| format!("合并快照缺少 {key}"))
        };
        match table.as_str() {
            "entity_facts" => {
                let fact_text = text("fact_text")?;
                let occurred_at = text("occurred_at")?;
                let confidence = number("confidence")?;
                let event = text("source_event_id")?;
                let created = text("created_at")?;
                let seen = text("last_seen_at")?;
                if disposition == "moved" {
                    let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM entity_facts WHERE id=?1 AND entity_kind=?2 AND entity_slug=?3 AND fact_text=?4 AND occurred_at=?5 AND confidence=?6 AND source_event_id=?7 AND created_at=?8 AND last_seen_at=?9)", params![row_id,kind,target,fact_text,occurred_at,confidence,event,created,seen], |r| r.get(0))?;
                    if !matches {
                        anyhow::bail!("合并后的事实已被修改或删除，不能安全撤销");
                    }
                    tx.execute(
                        "UPDATE entity_facts SET entity_slug=?1 WHERE id=?2",
                        params![source_slug, row_id],
                    )?;
                } else {
                    tx.execute("INSERT INTO entity_facts (id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![row_id,kind,source_slug,fact_text,occurred_at,confidence,event,created,seen])?;
                }
            }
            "entity_aliases" => {
                let alias = text("alias")?;
                let created = text("created_at")?;
                if disposition == "moved" {
                    let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM entity_aliases WHERE id=?1 AND entity_kind=?2 AND entity_slug=?3 AND alias=?4 AND created_at=?5)", params![row_id,kind,target,alias,created], |r| r.get(0))?;
                    if !matches {
                        anyhow::bail!("合并后的别名已被修改或删除，不能安全撤销");
                    }
                    tx.execute(
                        "UPDATE entity_aliases SET entity_slug=?1 WHERE id=?2",
                        params![source_slug, row_id],
                    )?;
                } else {
                    tx.execute("INSERT INTO entity_aliases (id,entity_kind,entity_slug,alias,created_at) VALUES (?1,?2,?3,?4,?5)", params![row_id,kind,source_slug,alias,created])?;
                }
            }
            "relations" => {
                let from_slug = text("from_slug")?;
                let from_kind = text("from_kind")?;
                let to_slug = text("to_slug")?;
                let to_kind = text("to_kind")?;
                let relation = text("relation")?;
                let confidence = number("confidence")?;
                let created = text("created_at")?;
                let seen = text("last_seen_at")?;
                let note = value
                    .get("note")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let conversation = value
                    .get("source_conversation_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let event = value
                    .get("source_event_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let merged_from = if from_kind == kind && from_slug == source_slug {
                    target.clone()
                } else {
                    from_slug.clone()
                };
                let merged_to = if to_kind == kind && to_slug == source_slug {
                    target.clone()
                } else {
                    to_slug.clone()
                };
                if disposition == "moved" {
                    let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM relations WHERE id=?1 AND from_slug=?2 AND from_kind=?3 AND to_slug=?4 AND to_kind=?5 AND relation=?6 AND note IS ?7 AND confidence=?8 AND source_conversation_id IS ?9 AND source_event_id IS ?10 AND created_at=?11 AND last_seen_at=?12)", params![row_id,merged_from,from_kind,merged_to,to_kind,relation,note,confidence,conversation,event,created,seen], |r| r.get(0))?;
                    if !matches {
                        anyhow::bail!("合并后的关系已被修改或删除，不能安全撤销");
                    }
                    tx.execute(
                        "UPDATE relations SET from_slug=?1,to_slug=?2 WHERE id=?3",
                        params![from_slug, to_slug, row_id],
                    )?;
                } else {
                    tx.execute("INSERT INTO relations (id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,source_conversation_id,source_event_id,created_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![row_id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,conversation,event,created,seen])?;
                }
            }
            "todos" => {
                let title = text("title")?;
                let todo_status = text("status")?;
                let priority = text("priority")?;
                let created = text("created_at")?;
                let updated = text("updated_at")?;
                let due = value
                    .get("due_at")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let event = value
                    .get("related_event_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let note = value
                    .get("note")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM todos WHERE id=?1 AND title=?2 AND status=?3 AND priority=?4 AND due_at IS ?5 AND related_event_id IS ?6 AND related_wiki_slug=?7 AND note IS ?8 AND created_at=?9 AND updated_at=?10)", params![row_id,title,todo_status,priority,due,event,target,note,created,updated], |r| r.get(0))?;
                if !matches {
                    anyhow::bail!("合并后的关联待办已被修改，不能安全撤销");
                }
                tx.execute(
                    "UPDATE todos SET related_wiki_slug=?1 WHERE id=?2",
                    params![source_slug, row_id],
                )?;
            }
            _ => anyhow::bail!("未知的合并快照类型"),
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "UPDATE wiki_pages SET status='active',updated_at=?1 WHERE slug=?2",
        params![now, source_slug],
    )?;
    tx.execute(
        "UPDATE entity_merges SET undone_at=?1 WHERE id=?2",
        params![now, merge_id],
    )?;
    tx.commit()?;
    Ok(true)
}

pub fn entity_merge_status(&self, source_slug: &str) -> Result<Option<EntityMergeStatus>> {
    self.connection.query_row(
        "SELECT source_slug,target_slug,entity_kind,created_at FROM entity_merges WHERE source_slug=?1 AND undone_at IS NULL ORDER BY created_at DESC LIMIT 1",
        [source_slug],
        |r| Ok(EntityMergeStatus { source_slug:r.get(0)?, target_slug:r.get(1)?, entity_kind:r.get(2)?, created_at:r.get(3)? }),
    ).optional().map_err(Into::into)
}
}
