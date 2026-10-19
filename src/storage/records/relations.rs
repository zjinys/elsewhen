//! 页间关系：upsert、列表与删除。

use super::*;

impl Store {
    /// 新建或刷新一条人物关系（(from, to, relation) 唯一，重复则更新 note / 时间戳）。
    pub fn upsert_relation(&self, draft: &RelationDraft) -> Result<Relation> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO relations
           (id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
            source_conversation_id, source_event_id, created_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(from_slug, to_slug, relation) DO UPDATE SET
           note = ?7,
           -- 增量合并（D2）：同一事件重复贡献（重放/reanalyze）不重复加权；
           -- 新事件贡献走渐进增强，封顶 5（宁缺毋滥——置信只认来源多样）。
           confidence = CASE
             WHEN ?10 IS NOT NULL AND relations.source_event_id = ?10 THEN relations.confidence
             ELSE MIN(5, relations.confidence + 1) END,
           source_event_id = COALESCE(?10, relations.source_event_id), last_seen_at = ?11",
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
}
