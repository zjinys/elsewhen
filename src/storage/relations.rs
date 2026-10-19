//! knowledge_relations（提案 2026-10-19 §7，P3 瘦身版）。
//!
//! 边界（评审定论）：关系网用来**找上下文**，依赖网才传播变更。v1 只写
//! 两类边——`mentions` 的确定性派生（origin='citation'，来自 tags 命中
//! 标题/别名，不调模型）与人工「相关页面」（origin='human'）；不写 AI 边、
//! 不做级联刷新、不做 1-hop 之外的遍历。本表是 local-knowledge-pipeline
//! 本地物，不进快照。

use anyhow::{Result, bail};
use rusqlite::params;

pub(crate) const MAX_RELATIONS_PER_PAGE: i64 = 12;

impl crate::storage::Store {
    /// 人工添加「相关页面」边（去重、禁自环、每页上限、页面必须存在）。
    /// 返回边 id；已存在的等价边原样返回其 id（幂等）。
    pub fn add_knowledge_relation(
        &self,
        from_slug: &str,
        to_slug: &str,
        relation: &str,
        note: Option<&str>,
    ) -> Result<String> {
        if from_slug == to_slug {
            bail!("不能把页面与自身关联");
        }
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT r.id FROM knowledge_relations r
                 JOIN wiki_pages f ON f.id = r.from_page_id
                 JOIN wiki_pages t ON t.id = r.to_page_id
                 WHERE f.slug = ?1 AND t.slug = ?2 AND r.relation = ?3",
                params![from_slug, to_slug, relation],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        self.assert_relation_capacity(from_slug)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let changed = self.connection.execute(
            "INSERT INTO knowledge_relations(id, from_page_id, to_page_id, relation,
               note, confidence, origin, source_event_ids, created_at, last_confirmed_at)
             SELECT ?1, f.id, t.id, ?4, ?5, 3, 'human', '[]', ?6, ?6
             FROM wiki_pages f JOIN wiki_pages t
               ON t.slug = ?3 AND t.status != 'archived'
             WHERE f.slug = ?2 AND f.status != 'archived'",
            params![id, from_slug, to_slug, relation, note, now],
        )?;
        if changed == 0 {
            bail!("页面不存在或已归档");
        }
        Ok(id)
    }

    /// 人工删除边。派生边（origin='citation'）不允许手删——下一轮重建
    /// 还会回来；确要断开就去改页面 tags，让事实消失，边才消失。
    pub fn remove_knowledge_relation_from_page(&self, slug: &str, relation_id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "DELETE FROM knowledge_relations
             WHERE id = ?2 AND origin = 'human'
               AND EXISTS(SELECT 1 FROM wiki_pages p
                 WHERE p.slug = ?1 AND p.id IN (from_page_id, to_page_id))",
            params![slug, relation_id],
        )?;
        if changed == 0 {
            bail!("边不存在，或它是派生边（改 tags 而非删边）");
        }
        Ok(())
    }

    /// 页面相关边（无向视角聚合）：from 或 to 任一侧命中都返回，另一侧
    /// 是对方页面。按 created_at 新→旧。derive 之前调用方负责重建。
    pub fn list_knowledge_relations(&self, slug: &str) -> Result<Vec<KnowledgeRelationRow>> {
        let mut statement = self.connection.prepare(
            "SELECT r.id, r.from_page_id, r.to_page_id, r.relation, r.origin, r.note,
               f.slug, f.title, t.slug, t.title
             FROM knowledge_relations r
             JOIN wiki_pages f ON f.id = r.from_page_id
             JOIN wiki_pages t ON t.id = r.to_page_id
             WHERE (f.slug = ?1 OR t.slug = ?1)
               AND f.status != 'archived' AND t.status != 'archived'
             ORDER BY r.created_at DESC",
        )?;
        let rows = statement
            .query_map([slug], |row| {
                Ok(KnowledgeRelationRow {
                    id: row.get(0)?,
                    from_page_id: row.get(1)?,
                    to_page_id: row.get(2)?,
                    relation: row.get(3)?,
                    origin: row.get(4)?,
                    note: row.get(5)?,
                    from_slug: row.get(6)?,
                    from_title: row.get(7)?,
                    to_slug: row.get(8)?,
                    to_title: row.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 确定性派生 `mentions` 边（origin='citation'）：页面 tags 逐字命中
    /// 另一页标题（trim 后不分大小写）。全量对全量重算，与现存
    /// citation 边做差量——多退少补，不动人工/AI 边。归档与衍生页不参与。
    /// 返回 (新增, 移除)。
    pub fn rebuild_mention_relations(&self) -> Result<(usize, usize)> {
        let pages: Vec<(String, String, String)> = self
            .connection
            .prepare(
                "SELECT id, slug, title FROM wiki_pages
                 WHERE status != 'archived' AND COALESCE(area, 'insight') = 'insight'",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut desired: Vec<(String, String)> = Vec::new(); // (from_id, to_id)
        let norm = |s: &str| s.trim().to_lowercase();
        for (from_id, from_slug, from_title_normless) in &pages {
            let _ = from_title_normless;
            let tags: Vec<String> = self
                .connection
                .prepare("SELECT tags FROM wiki_pages WHERE id = ?1")?
                .query_row([from_id], |row| row.get::<_, String>(0))
                .map(|raw| serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default())?;
            for (to_id, _to_slug, to_title) in &pages {
                if to_id == from_id {
                    continue;
                }
                let title = norm(to_title);
                if title.is_empty() {
                    continue;
                }
                if tags.iter().any(|t| norm(t) == title) {
                    desired.push((from_id.clone(), to_id.clone()));
                }
            }
            let _ = from_slug;
        }
        // 差量：desired - 现存 citation = 插入；现存 citation - desired = 删除。
        let existing: Vec<(String, String)> = self
            .connection
            .prepare(
                "SELECT from_page_id, to_page_id FROM knowledge_relations
                 WHERE origin = 'citation' AND relation = 'mentions'",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut added = 0;
        let mut removed = 0;
        let now = chrono::Utc::now().to_rfc3339();
        for edge in &desired {
            if !existing.contains(edge) {
                added += self.connection.execute(
                    "INSERT OR IGNORE INTO knowledge_relations(id, from_page_id,
                       to_page_id, relation, confidence, origin, source_event_ids,
                       created_at, last_confirmed_at)
                     VALUES(?1, ?2, ?3, 'mentions', 3, 'citation', '[]', ?4, ?4)",
                    params![uuid::Uuid::new_v4().to_string(), edge.0, edge.1, now],
                )?;
            } else {
                // 仍成立：refresh 确认时间，供后续「人工提升」参考。
                self.connection.execute(
                    "UPDATE knowledge_relations SET last_confirmed_at = ?3
                     WHERE origin = 'citation' AND relation = 'mentions'
                       AND from_page_id = ?1 AND to_page_id = ?2",
                    params![edge.0, edge.1, now],
                )?;
            }
        }
        for edge in &existing {
            if !desired.contains(edge) {
                removed += self.connection.execute(
                    "DELETE FROM knowledge_relations
                     WHERE origin = 'citation' AND relation = 'mentions'
                       AND from_page_id = ?1 AND to_page_id = ?2",
                    params![edge.0, edge.1],
                )?;
            }
        }
        Ok((added, removed))
    }

    fn assert_relation_capacity(&self, from_slug: &str) -> Result<()> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM knowledge_relations r
             JOIN wiki_pages p ON p.id = r.from_page_id WHERE p.slug = ?1",
            [from_slug],
            |row| row.get(0),
        )?;
        if count >= MAX_RELATIONS_PER_PAGE {
            bail!("每页最多 {MAX_RELATIONS_PER_PAGE} 条相关页面");
        }
        Ok(())
    }
}

use rusqlite::OptionalExtension;

/// 关系边行（含双侧 slug/标题便于直接导航）。
pub struct KnowledgeRelationRow {
    pub id: String,
    pub from_page_id: String,
    pub to_page_id: String,
    pub relation: String,
    pub origin: String,
    pub note: Option<String>,
    pub from_slug: String,
    pub from_title: String,
    pub to_slug: String,
    pub to_title: String,
}

impl KnowledgeRelationRow {
    /// 视角归一：给定观察页返回 (对方 slug, 对方标题, 关系语义方向)。
    pub fn other_side(&self, slug: &str) -> Option<(String, String, String)> {
        if self.from_slug == slug {
            Some((
                self.to_slug.clone(),
                self.to_title.clone(),
                self.relation.clone(),
            ))
        } else {
            // 入向 mentions：对方的 A 页 tags 提到了本观察页 → 展示「被提及」。
            Some((
                self.from_slug.clone(),
                self.from_title.clone(),
                format!("被{}", self.relation),
            ))
        }
    }
}
