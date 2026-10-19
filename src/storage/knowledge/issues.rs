//! 知识问题（issues）：列表、recordable 过滤与 dismiss。

use super::*;

impl Store {
    pub(crate) fn recordable_event(&self, id: &str) -> Result<bool> {
        Ok(self.connection.query_row("SELECT COALESCE(
            (SELECT d.recordable FROM event_recordability_decisions d WHERE d.event_id=e.id ORDER BY d.created_at DESC,d.rowid DESC LIMIT 1),
            (SELECT json_extract(a.result_json,'$.recordable') FROM event_analyses a WHERE a.event_id=e.id ORDER BY a.created_at DESC,a.id DESC LIMIT 1),1)
            FROM events e WHERE e.id=?1", [id], |r|r.get::<_,bool>(0)).optional()?.unwrap_or(false))
    }

    pub fn knowledge_issues(&self, slug: Option<&str>) -> Result<Vec<KnowledgeIssue>> {
        let pages = if let Some(slug) = slug {
            self.get_wiki_page(slug)?.into_iter().collect()
        } else {
            let mut all = self.list_wiki_pages(None, None)?;
            all.extend(self.list_wiki_pages(None, Some("derivative"))?);
            all
        };
        // 批量预取点（原为 O(N) 逐条查询）：已审指纹集合、事件可记录性映射。
        // 页数增长时这里是维护面板打开成本的大头——两条 SQL 换掉循环里的
        // 每指纹一条 EXISTS、每事件一条 recordable。
        let reviewed: std::collections::HashSet<String> = {
            let mut stmt = self
                .connection
                .prepare("SELECT fingerprint FROM knowledge_maintenance_reviews")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().collect()
        };
        let recordable: std::collections::HashMap<String, bool> = {
            let mut map = std::collections::HashMap::new();
            let mut stmt = self.connection.prepare(
                "SELECT e.id, COALESCE(
                    (SELECT d.recordable FROM event_recordability_decisions d WHERE d.event_id=e.id ORDER BY d.created_at DESC,d.rowid DESC LIMIT 1),
                    (SELECT json_extract(a.result_json,'$.recordable') FROM event_analyses a WHERE a.event_id=e.id ORDER BY a.created_at DESC,a.id DESC LIMIT 1),1)
                 FROM events e",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, ok) in rows {
                map.insert(id, ok);
            }
            map
        };
        let mut issues = Vec::new();
        for page in pages.into_iter().filter(|p| p.status != "archived") {
            let mut add = |kind: &str, detail: String, key: String| -> Result<()> {
                let fingerprint = content_hash(&format!("{}|{kind}|{key}", page.id));
                let reviewed = reviewed.contains(&fingerprint);
                if !reviewed {
                    issues.push(KnowledgeIssue {
                        fingerprint,
                        page_slug: page.slug.clone(),
                        kind: kind.into(),
                        description: detail,
                    });
                }
                Ok(())
            };
            let sources = self.page_source_snapshots(&page.slug)?;
            if crate::knowledge::dependencies::stale(self, &page.id)? {
                add(
                    "upstream_changed",
                    "上游知识已纠正或历史依据版本未知；本页暂停作为依据，请检查更新并审阅。".into(),
                    serde_json::to_string(&crate::knowledge::dependencies::state(self, &page.id)?)?,
                )?;
            }
            if page.source_event_ids.is_empty() && sources.is_empty() && page.source_url.is_none() {
                add(
                    "no_evidence",
                    "尚无可核验来源；这是一条维护提示，不代表内容错误。".into(),
                    content_hash(&page.content_md),
                )?;
            }
            for id in &page.source_event_ids {
                if !recordable.get(id).copied().unwrap_or(false) {
                    add(
                        "invalid_event",
                        format!("来源事件 {id} 不存在或已标为讨论。"),
                        id.clone(),
                    )?;
                }
            }
            if page.kind == "topic"
                && (sources.len() > 8
                    || page.source_event_ids.len() > 100
                    || page.content_md.chars().count() > 12000)
            {
                add(
                    "oversized_topic",
                    "主题超过单次维护容量，请在产出页预览拆分；已有内容和来源保留。".into(),
                    format!(
                        "{}:{}:{}",
                        content_hash(&page.content_md),
                        sources.len(),
                        page.source_event_ids.len()
                    ),
                )?;
            }
            for source in sources {
                if source.opinion.as_deref() == Some("reject") {
                    add(
                        "rejected_source",
                        format!("来源《{}》已被标为不认可。", source.title),
                        source.id.clone(),
                    )?;
                }
                let version: i64 = self.connection.query_row(
                    "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
                    [&source.source_id],
                    |r| r.get(0),
                )?;
                if source.version < version {
                    add(
                        "source_changed",
                        format!(
                            "来源《{}》已从 v{} 更新至 v{version}，正文保留，请审阅。",
                            source.title, source.version
                        ),
                        format!("{}:{version}", source.source_id),
                    )?;
                }
            }
            for part in page.content_md.split("[[").skip(1).take(100) {
                if let Some((link, _)) = part.split_once("]]") {
                    let target = link.split('|').next().unwrap_or("").trim();
                    if !target.is_empty() && self.get_wiki_page(target)?.is_none() {
                        add(
                            "broken_link",
                            format!("引用页面不存在：{target}"),
                            target.into(),
                        )?;
                    }
                }
            }
        }
        let rows = self.connection.prepare(
            "SELECT i.fingerprint,p.slug,i.kind,i.description,i.page_hash,p.content_md,i.snapshot_ids
             FROM knowledge_semantic_issues i JOIN wiki_pages p ON p.id=i.page_id
             WHERE p.status<>'archived' AND (?1 IS NULL OR p.slug=?1)
             AND NOT EXISTS(SELECT 1 FROM knowledge_maintenance_reviews r WHERE r.fingerprint=i.fingerprint)
             ORDER BY i.created_at DESC LIMIT 200"
        )?.query_map([slug], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (fingerprint, page_slug, kind, description, hash, content, snapshots) in rows {
            if hash != content_hash(&content) {
                continue;
            }
            let ids: Vec<String> = serde_json::from_str(&snapshots)?;
            let mut current = !ids.is_empty();
            for id in ids {
                let Some(source) = self.source_snapshot(&id)? else {
                    current = false;
                    break;
                };
                let latest: i64 = self.connection.query_row(
                    "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
                    [&source.source_id],
                    |r| r.get(0),
                )?;
                let page = source
                    .page_slug
                    .as_deref()
                    .map(|s| self.get_wiki_page(s))
                    .transpose()?
                    .flatten();
                if latest != source.version
                    || source.opinion.as_deref() == Some("reject")
                    || page.is_none_or(|p| p.status == "archived")
                {
                    current = false;
                    break;
                }
            }
            if current {
                issues.push(KnowledgeIssue {
                    fingerprint,
                    page_slug,
                    kind,
                    description,
                });
            }
        }
        Ok(issues)
    }

    pub fn dismiss_knowledge_issue(&self, fingerprint: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let issue = self
            .knowledge_issues(None)?
            .into_iter()
            .find(|i| i.fingerprint == fingerprint)
            .context("提示已变化，请刷新")?;
        let page = self
            .get_wiki_page(&issue.page_slug)?
            .context("页面不存在")?;
        let changed = self.connection.execute("INSERT OR IGNORE INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,issue_description)
            VALUES (?1,?2,'dismissed',?3,?4)",params![fingerprint,page.id,chrono::Utc::now().to_rfc3339(),issue.description])?;
        // 已阅行是跨设备语义（B 设备不再重复弹同一 issue）：本端首次落行
        // 时同事务广播；重复 dismiss 幂等，不再产生 op。
        if changed > 0 {
            self.sync_record_knowledge_review(fingerprint)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}
