//! Store 读面：源快照/历史检索、预览、拒绝列表与页面绑定。

use super::*;

impl Store {
    pub(crate) fn check_source_preview(
        &self,
        locator: &str,
        content: &str,
        expected: Option<&str>,
    ) -> Result<()> {
        let page = self.matching_source_page(Some(locator), content)?;
        let current = page
            .as_ref()
            .map(|p| self.source_history(&p.slug))
            .transpose()?
            .and_then(|h| h.into_iter().next());
        if current.as_ref().map(|s| s.id.as_str()) != expected
            && !current.as_ref().is_some_and(|s| s.content_md == content)
        {
            bail!("来源已被更新，请重新预览后保存");
        }
        Ok(())
    }
    /// 再导入提示的查询面（负知识读面之一）：identity 命中「已否定」来源时
    /// 返回用户写下的否定原因；孵化闸门的其余入口见 knowledge::participating。
    pub fn rejection_for_source(
        &self,
        locator: Option<&str>,
        content: &str,
    ) -> Result<Option<String>> {
        let identity = source_identity(locator, content);
        Ok(self
            .connection
            .query_row(
                "SELECT reject_reason FROM knowledge_sources
             WHERE identity=?1 AND opinion='reject'",
                [&identity],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn rejected_sources(&self) -> Result<Vec<RejectedSourceRow>> {
        Ok(self
            .connection
            .prepare(
                "SELECT (SELECT p.slug FROM knowledge_source_pages m JOIN wiki_pages p ON p.id=m.page_id
                         WHERE m.source_id=s.id ORDER BY m.rowid LIMIT 1),
                        (SELECT p.title FROM knowledge_source_pages m JOIN wiki_pages p ON p.id=m.page_id
                         WHERE m.source_id=s.id ORDER BY m.rowid LIMIT 1),
                        s.identity,s.kind,s.locator,COALESCE(s.reject_reason,''),s.created_at
                 FROM knowledge_sources s WHERE s.opinion='reject' ORDER BY s.created_at DESC",
            )?
            .query_map([], |r| {
                Ok(RejectedSourceRow {
                    slug: r.get(0)?,
                    title: r.get(1)?,
                    identity: r.get(2)?,
                    source_kind: r.get(3)?,
                    locator: r.get(4)?,
                    reject_reason: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn matching_source_page(
        &self,
        locator: Option<&str>,
        content: &str,
    ) -> Result<Option<WikiPage>> {
        let identity = source_identity(locator, content);
        let slug: Option<String> = self
            .connection
            .query_row(
                "SELECT p.slug
            FROM knowledge_sources s JOIN knowledge_source_pages m ON m.source_id=s.id
            JOIN wiki_pages p ON p.id=m.page_id WHERE s.identity=?1
            ORDER BY p.created_at,p.id LIMIT 1",
                [&identity],
                |r| r.get(0),
            )
            .optional()?;
        slug.map(|s| self.get_wiki_page(&s))
            .transpose()
            .map(Option::flatten)
    }

    pub fn source_snapshot(&self, id: &str) -> Result<Option<SourceSnapshot>> {
        Ok(self
            .connection
            .query_row(
                &format!("{SNAPSHOT_SELECT} WHERE v.id=?1"),
                [id],
                snapshot_row,
            )
            .optional()?)
    }

    pub fn source_history(&self, slug: &str) -> Result<Vec<SourceSnapshot>> {
        let mut stmt = self.connection.prepare(&format!(
            "{SNAPSHOT_SELECT}
            WHERE s.id IN (SELECT m.source_id FROM knowledge_source_pages m
                JOIN wiki_pages p ON p.id=m.page_id WHERE p.slug=?1)
            ORDER BY v.version DESC"
        ))?;
        let rows = stmt
            .query_map([slug], snapshot_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn page_source_snapshots(&self, slug: &str) -> Result<Vec<SourceSnapshot>> {
        let own = self
            .connection
            .query_row(
                &format!(
                    "{SNAPSHOT_SELECT}
            WHERE s.id IN (SELECT m.source_id FROM knowledge_source_pages m
                JOIN wiki_pages p ON p.id=m.page_id WHERE p.slug=?1)
            AND v.content_hash=?2 ORDER BY v.version DESC LIMIT 1"
                ),
                params![
                    slug,
                    self.get_wiki_page(slug)?
                        .map(|p| content_hash(&p.content_md))
                        .unwrap_or_default()
                ],
                snapshot_row,
            )
            .optional()?;
        if let Some(latest) = own {
            return Ok(vec![latest]);
        }
        let mut stmt = self.connection.prepare(&format!(
            "{SNAPSHOT_SELECT}
            JOIN knowledge_page_sources e ON e.snapshot_id=v.id
            JOIN wiki_pages p ON p.id=e.page_id WHERE p.slug=?1 ORDER BY v.captured_at,v.id"
        ))?;
        let rows = stmt
            .query_map([slug], snapshot_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub(crate) fn bind_page_sources(&self, page_id: &str, snapshots: &[String]) -> Result<()> {
        for id in snapshots {
            self.source_snapshot(id)?
                .context("来源版本不存在，不能保存编译结果")?;
            self.connection.execute(
                "INSERT OR IGNORE INTO knowledge_page_sources(page_id,snapshot_id) VALUES (?1,?2)",
                params![page_id, id],
            )?;
        }
        Ok(())
    }
}
