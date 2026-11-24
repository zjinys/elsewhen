use super::*;

pub(crate) fn conversation_current(store: &Store, id: &str) -> Result<bool> {
    let Some(conversation) = store.get_conversation(id)? else {
        return Ok(false);
    };
    if conversation.archived {
        return Ok(false);
    }
    if let Some(slug) = conversation.wiki_page_slug {
        let Some(page) = store.get_wiki_page(&slug)? else {
            return Ok(false);
        };
        if page.status == "archived"
            || page.opinion.as_deref() == Some("reject")
            || crate::knowledge::dependencies::stale(store, &page.id)?
        {
            return Ok(false);
        }
    }
    if let Some(goal) = conversation.goal_id {
        let status: Option<String> = store
            .connection
            .query_row("SELECT status FROM goals WHERE id=?1", [goal], |row| {
                row.get(0)
            })
            .optional()?;
        if !status.is_some_and(|status| matches!(status.as_str(), "active" | "paused")) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn snapshot(store: &Store, source: &str, bound: i64) -> Result<String> {
    ensure!(conversation_current(store, source)?, "来源会话不可用");
    let mut statement = store.connection.prepare("SELECT id,role,content FROM messages WHERE conversation_id=?1 AND rowid<=?2 ORDER BY rowid")?;
    let rows = statement
        .query_map(params![source, bound], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(hash(&serde_json::to_string(&rows)?))
}

impl Store {
    pub(crate) fn memory_grant_basis(&self, source: &str, target: &str) -> Result<Value> {
        ensure!(
            source != target && conversation_current(self, target)?,
            "目标会话无效"
        );
        let bound: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(rowid),0) FROM messages WHERE conversation_id=?1",
            [source],
            |row| row.get(0),
        )?;
        let existing: Option<i64> = self
            .connection
            .query_row(
                "SELECT version FROM personal_memory_grants WHERE source_id=?1 AND target_id=?2",
                params![source, target],
                |row| row.get(0),
            )
            .optional()?;
        let conversation = self.get_conversation(source)?.context("来源会话不存在")?;
        Ok(
            json!({"source_id":source,"target_id":target,"source_title":conversation.title,"message_bound":bound,"source_hash":snapshot(self,source,bound)?,"grant_version":existing.unwrap_or(0)}),
        )
    }

    pub(crate) fn memory_grant(&self, source: &str, target: &str) -> Result<Option<Value>> {
        if !conversation_current(self, source)? || !conversation_current(self, target)? {
            return Ok(None);
        }
        let row = self.connection.query_row("SELECT message_bound,source_hash,version,expires_at FROM personal_memory_grants WHERE source_id=?1 AND target_id=?2 AND enabled=1", params![source,target], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,Option<String>>(3)?))).optional()?;
        let Some((bound, version_hash, version, expires)) = row else {
            return Ok(None);
        };
        if expires.as_deref().is_some_and(|expires| {
            chrono::DateTime::parse_from_rfc3339(expires)
                .map_or(true, |date| date <= chrono::Utc::now())
        }) || snapshot(self, source, bound)? != version_hash
        {
            return Ok(None);
        }
        Ok(Some(
            json!({"source_id":source,"target_id":target,"message_bound":bound,"source_hash":version_hash,"grant_version":version}),
        ))
    }

    pub(crate) fn memory_shared_sources(&self, target: &str) -> Result<Vec<String>> {
        let sources = self.connection.prepare("SELECT source_id FROM personal_memory_grants WHERE target_id=?1 AND enabled=1 ORDER BY source_id LIMIT 8")?.query_map([target], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut allowed = Vec::new();
        for source in sources {
            if self.memory_grant(&source, target)?.is_some() {
                allowed.push(source);
            }
        }
        Ok(allowed)
    }
}
