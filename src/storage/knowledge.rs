//! Exact source versions, reviewable compilation and knowledge provenance.
use super::{map_wiki_page, ContentPolicy, Store, WikiPage, WikiPageDraft, WIKI_PAGE_COLS};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) fn content_hash(text: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, text.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn source_change_preview(previous: &str, incoming: &str) -> String {
    if previous == incoming {
        return "原文内容相同，不增加版本。".into();
    }
    let offset = previous
        .chars()
        .zip(incoming.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let start = offset.saturating_sub(60);
    let old: String = previous.chars().skip(start).take(300).collect();
    let new: String = incoming.chars().skip(start).take(300).collect();
    format!(
        "首处变化附近（第 {} 字起，限长预览）：\n旧：{old}\n新：{new}",
        start + 1
    )
}

pub(crate) fn canonical_locator(locator: &str) -> String {
    match reqwest::Url::parse(locator.trim()) {
        Ok(mut url) => {
            if matches!(
                url.host_str(),
                Some(
                    "x.com"
                        | "www.x.com"
                        | "twitter.com"
                        | "www.twitter.com"
                        | "mobile.twitter.com"
                )
            ) {
                if let Some(id) = crate::wiki::extract_tweet_id(url.as_str()) {
                    return format!("https://x.com/i/status/{id}");
                }
            }
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => locator.trim().to_string(),
    }
}

fn source_identity(locator: Option<&str>, content: &str) -> String {
    locator
        .filter(|s| !s.trim().is_empty())
        .map(canonical_locator)
        .unwrap_or_else(|| format!("text:sha256:{}", content_hash(content)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub id: String,
    pub source_id: String,
    pub version: i64,
    pub title: String,
    pub content_md: String,
    pub content_hash: String,
    pub captured_at: String,
    pub source_kind: String,
    pub locator: Option<String>,
    pub opinion: Option<String>,
    pub page_slug: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct KnowledgeMetadata {
    pub applicable_when: String,
    pub strength: String,
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeProposal {
    pub id: String,
    pub page_id: Option<String>,
    pub target_slug: String,
    pub kind: String,
    pub title: String,
    pub content_md: String,
    pub applicable_when: String,
    pub snapshot_ids: Vec<String>,
    pub event_ids: Vec<String>,
    pub base_hash: Option<String>,
    pub reason: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeIssue {
    pub fingerprint: String,
    pub page_slug: String,
    pub kind: String,
    pub description: String,
}

const SNAPSHOT_SELECT: &str = "SELECT v.id,v.source_id,v.version,v.title,v.content_md,
 v.content_hash,v.captured_at,s.kind,s.locator,s.opinion,
 (SELECT p.slug FROM knowledge_source_pages m JOIN wiki_pages p ON p.id=m.page_id
  WHERE m.source_id=s.id ORDER BY p.created_at,p.id LIMIT 1)
 FROM knowledge_snapshots v JOIN knowledge_sources s ON s.id=v.source_id";

fn snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceSnapshot> {
    Ok(SourceSnapshot {
        id: row.get(0)?,
        source_id: row.get(1)?,
        version: row.get(2)?,
        title: row.get(3)?,
        content_md: row.get(4)?,
        content_hash: row.get(5)?,
        captured_at: row.get(6)?,
        source_kind: row.get(7)?,
        locator: row.get(8)?,
        opinion: row.get(9)?,
        page_slug: row.get(10)?,
    })
}

/// Runs once after migrations in its own transaction. Page IDs never move.
/// Duplicate URLs share one identity; all differing originals remain snapshots.
pub(crate) fn backfill_sources(conn: &Connection) -> Result<()> {
    let pages = conn
        .prepare(&format!(
            "SELECT {WIKI_PAGE_COLS} FROM wiki_pages
        WHERE kind IN ('source','note') ORDER BY updated_at,created_at,id"
        ))?
        .query_map([], map_wiki_page)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut originals = Vec::new();
    for mut page in pages {
        // Legacy explicit file imports stored the path in a fixed Markdown field.
        // Recover that locator without reading or scanning the user's filesystem.
        if page.source_url.is_none() && page.tags.iter().any(|t| t == "local-source") {
            if let Some(path) = page
                .content_md
                .lines()
                .find_map(|line| line.strip_prefix("- 本地路径：`")?.strip_suffix('`'))
            {
                let url = crate::wiki::path_to_file_url(std::path::Path::new(path));
                conn.execute(
                    "UPDATE wiki_pages SET source_url=?1 WHERE id=?2",
                    params![url, page.id],
                )?;
                page.source_url = Some(url);
            }
        }
        ensure_source_on(conn, &page)?;
        let revisions=conn.prepare("SELECT content_md,created_at FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at,rowid")?
            .query_map([&page.id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (body, at) in revisions {
            let mut revision = page.clone();
            revision.content_md = body;
            revision.updated_at = at;
            originals.push(revision);
        }
        originals.push(page);
    }
    originals.sort_by(|a, b| a.updated_at.cmp(&b.updated_at).then(a.id.cmp(&b.id)));
    for original in originals {
        capture_source_on(conn, &original)?;
    }
    // Preserve the original version for pre-existing derivatives, rather than
    // inventing an event source for imported material.
    conn.execute(
        "INSERT OR IGNORE INTO knowledge_page_sources(page_id,snapshot_id)
        SELECT d.id,v.id FROM wiki_pages d JOIN wiki_pages b ON b.slug=d.based_on
        JOIN knowledge_source_pages m ON m.page_id=b.id
        JOIN knowledge_snapshots v ON v.source_id=m.source_id
        WHERE v.version=(SELECT MAX(v2.version) FROM knowledge_snapshots v2
            WHERE v2.source_id=v.source_id AND v2.captured_at<=d.created_at)",
        [],
    )?;
    Ok(())
}

pub(crate) fn backfill_sources_once(conn: &Connection) -> Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let done: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_meta WHERE key='knowledge_sources_backfill_v1')",
        [],
        |r| r.get(0),
    )?;
    if !done {
        backfill_sources(conn)?;
        tx.execute(
            "INSERT INTO app_meta(key,value) VALUES ('knowledge_sources_backfill_v1','done')",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn ensure_source_on(conn: &Connection, page: &WikiPage) -> Result<String> {
    let identity = source_identity(page.source_url.as_deref(), &page.content_md);
    let bound: Option<String> = conn
        .query_row(
            "SELECT source_id FROM knowledge_source_pages WHERE page_id=?1",
            [&page.id],
            |r| r.get(0),
        )
        .optional()?;
    let source_id = match bound {
        Some(id) => id,
        None => {
            let kind = if page
                .source_url
                .as_deref()
                .is_some_and(|s| s.starts_with("file:"))
            {
                "file"
            } else if page.tags.iter().any(|t| t == "tweet") {
                "tweet"
            } else if page.source_url.is_some() {
                "webpage"
            } else {
                "text"
            };
            conn.execute(
                "INSERT OR IGNORE INTO knowledge_sources
                (id,identity,kind,locator,opinion,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    Uuid::new_v4().to_string(),
                    identity,
                    kind,
                    page.source_url.as_deref().map(canonical_locator),
                    page.opinion,
                    page.created_at
                ],
            )?;
            conn.query_row(
                "SELECT id FROM knowledge_sources WHERE identity=?1",
                [&identity],
                |r| r.get::<_, String>(0),
            )?
        }
    };
    conn.execute(
        "INSERT OR IGNORE INTO knowledge_source_pages(page_id,source_id) VALUES (?1,?2)",
        params![page.id, source_id],
    )?;
    // A rejection on any legacy alias is respected across the source identity.
    if page.opinion.as_deref() == Some("reject") {
        conn.execute(
            "UPDATE knowledge_sources SET opinion='reject' WHERE id=?1",
            [&source_id],
        )?;
    }
    Ok(source_id)
}

pub(crate) fn capture_source_on(conn: &Connection, page: &WikiPage) -> Result<()> {
    if !matches!(page.kind.as_str(), "source" | "note") {
        return Ok(());
    }
    let source_id = ensure_source_on(conn, page)?;
    let latest = conn
        .query_row(
            &format!("{SNAPSHOT_SELECT} WHERE s.id=?1 ORDER BY v.version DESC LIMIT 1"),
            [&source_id],
            snapshot_row,
        )
        .optional()?;
    let hash = content_hash(&page.content_md);
    if latest.as_ref().is_some_and(|s| {
        s.content_hash == hash && s.content_md == page.content_md && s.title == page.title
    }) {
        return Ok(());
    }
    conn.execute("INSERT INTO knowledge_snapshots
        (id,source_id,version,title,content_md,content_hash,captured_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![Uuid::new_v4().to_string(),source_id,latest.map_or(1, |s| s.version+1),
            page.title,page.content_md,hash,page.updated_at])?;
    Ok(())
}

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

    pub fn knowledge_metadata(&self, slug: &str) -> Result<KnowledgeMetadata> {
        Ok(self
            .connection
            .query_row(
                "SELECT m.applicable_when,m.strength,m.confirmed_at
            FROM knowledge_metadata m JOIN wiki_pages p ON p.id=m.page_id WHERE p.slug=?1",
                [slug],
                |r| {
                    Ok(KnowledgeMetadata {
                        applicable_when: r.get(0)?,
                        strength: r.get(1)?,
                        confirmed_at: r.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(KnowledgeMetadata {
                strength: "reference".into(),
                ..Default::default()
            }))
    }

    /// Only the explicit GUI confirmation endpoint calls this; model tools do not.
    pub fn set_knowledge_metadata(
        &self,
        slug: &str,
        applicable_when: &str,
        strength: &str,
    ) -> Result<()> {
        if !matches!(strength, "reference" | "method" | "rule") {
            bail!("无效的知识强度");
        }
        if applicable_when.chars().count() > 600 {
            bail!("适用条件最多 600 字");
        }
        let page = self.get_wiki_page(slug)?.context("知识页不存在")?;
        if !matches!(page.kind.as_str(), "method" | "case" | "principle") {
            bail!("仅方法、案例、规律页可设置适用条件与强度");
        }
        if strength != "reference" && applicable_when.trim().is_empty() {
            bail!("请先填写适用条件");
        }
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        self.connection.execute("INSERT INTO knowledge_metadata(page_id,applicable_when,strength,confirmed_at)
            VALUES (?1,?2,?3,?4) ON CONFLICT(page_id) DO UPDATE SET
            applicable_when=excluded.applicable_when,strength=excluded.strength,confirmed_at=excluded.confirmed_at",
            params![page.id,applicable_when.trim(),strength,chrono::Utc::now().to_rfc3339()])?;
        self.append_wiki_log(&format!(
            "知识适用条件/强度由用户确认：{slug} → {strength}；{applicable_when}"
        ))?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn record_proposal(
        &self,
        draft: &WikiPageDraft,
        applicable_when: &str,
        snapshots: &[String],
        base: Option<&WikiPage>,
        reason: &str,
    ) -> Result<String> {
        self.record_proposal_with_origin(draft, applicable_when, snapshots, base, reason, "manual")
    }

    pub(crate) fn record_proposal_with_origin(
        &self,
        draft: &WikiPageDraft,
        applicable_when: &str,
        snapshots: &[String],
        base: Option<&WikiPage>,
        reason: &str,
        origin: &str,
    ) -> Result<String> {
        if draft.content_md.trim().is_empty() {
            bail!("建议正文为空");
        }
        if snapshots.is_empty() && draft.source_event_ids.is_empty() {
            bail!("知识建议缺少真实来源");
        }
        for id in snapshots {
            self.source_snapshot(id)?.context("建议引用的来源不存在")?;
        }
        for id in &draft.source_event_ids {
            if !self.recordable_event(id)? {
                bail!("建议包含不存在或不可记录的事件");
            }
        }
        let mut source_keys = snapshots.to_vec();
        source_keys.sort();
        source_keys.dedup();
        let mut event_keys = draft.source_event_ids.clone();
        event_keys.sort();
        event_keys.dedup();
        // Same input version + same target is a single decision. Rejected
        // suggestions cannot reappear merely because the model rephrases them.
        let base_state = base
            .map(|p| {
                crate::knowledge::authoring::revision_basis(self, p)
                    .and_then(|v| Ok(serde_json::to_string(&v)?))
            })
            .transpose()?;
        let key = content_hash(&format!(
            "{}|{}|{}|{}",
            base.map_or(draft.slug.as_str(), |p| p.id.as_str()),
            source_keys.join(","),
            event_keys.join(","),
            format!(
                "{}|{}",
                base_state.as_deref().unwrap_or_default(),
                if reason.starts_with("恢复历史正文") {
                    content_hash(&draft.content_md)
                } else {
                    String::new()
                }
            )
        ));
        self.connection.execute(
            "INSERT OR IGNORE INTO knowledge_proposals
            (id,dedupe_key,page_id,target_slug,kind,title,content_md,applicable_when,
             snapshot_ids,event_ids,base_hash,reason,created_at,origin,base_state)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                Uuid::new_v4().to_string(),
                key,
                base.map(|p| &p.id),
                draft.slug,
                draft.kind,
                draft.title,
                draft.content_md,
                applicable_when,
                serde_json::to_string(&source_keys)?,
                serde_json::to_string(&event_keys)?,
                base.map(|p| content_hash(&p.content_md)),
                reason,
                chrono::Utc::now().to_rfc3339(),
                origin,
                base_state
            ],
        )?;
        Ok(self.connection.query_row(
            "SELECT id FROM knowledge_proposals WHERE dedupe_key=?1",
            [&key],
            |r| r.get(0),
        )?)
    }

    pub fn knowledge_proposals(&self, slug: Option<&str>) -> Result<Vec<KnowledgeProposal>> {
        self.find_knowledge_proposals(slug, None)
    }

    /// Navigate the existing provenance graph, including pages compiled before
    /// the UI exposed these links. Never invent a single based_on for many sources.
    pub fn knowledge_origin_pages(&self, slug: &str) -> Result<Vec<WikiPage>> {
        let sql = format!(
            "SELECT {WIKI_PAGE_COLS} FROM wiki_pages WHERE slug<>?1 AND id IN (
            SELECT origin.page_id FROM knowledge_source_pages origin
            JOIN knowledge_snapshots s ON s.source_id=origin.source_id
            JOIN knowledge_page_sources cited ON cited.snapshot_id=s.id
            JOIN wiki_pages target ON target.id=cited.page_id WHERE target.slug=?1
            UNION SELECT d.upstream_id FROM knowledge_dependencies d JOIN wiki_pages target ON target.id=d.page_id WHERE target.slug=?1
            UNION SELECT r.old_page_id FROM knowledge_topic_replacements r JOIN wiki_pages target ON target.id=r.new_page_id WHERE target.slug=?1)
            ORDER BY title,slug"
        );
        let mut stmt = self.connection.prepare(&sql)?;
        let rows = stmt.query_map([slug], map_wiki_page)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn knowledge_output_pages(&self, slug: &str) -> Result<Vec<WikiPage>> {
        let sql = format!(
            "SELECT {WIKI_PAGE_COLS} FROM wiki_pages WHERE slug<>?1 AND id IN (
            SELECT cited.page_id FROM knowledge_page_sources cited
            JOIN knowledge_snapshots s ON s.id=cited.snapshot_id
            JOIN knowledge_source_pages origin ON origin.source_id=s.source_id
            JOIN wiki_pages material ON material.id=origin.page_id WHERE material.slug=?1
            UNION SELECT d.page_id FROM knowledge_dependencies d JOIN wiki_pages original ON original.id=d.upstream_id WHERE original.slug=?1
            UNION SELECT r.new_page_id FROM knowledge_topic_replacements r JOIN wiki_pages original ON original.id=r.old_page_id WHERE original.slug=?1)
            ORDER BY updated_at DESC,slug"
        );
        let mut stmt = self.connection.prepare(&sql)?;
        let rows = stmt.query_map([slug], map_wiki_page)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn find_knowledge_proposals(
        &self,
        slug: Option<&str>,
        id: Option<&str>,
    ) -> Result<Vec<KnowledgeProposal>> {
        let mut stmt = self.connection.prepare("SELECT k.id,k.page_id,COALESCE(p.slug,k.target_slug),k.kind,k.title,
            k.content_md,k.applicable_when,k.snapshot_ids,k.event_ids,k.base_hash,k.reason,k.status,k.created_at
            FROM knowledge_proposals k LEFT JOIN wiki_pages p ON p.id=k.page_id
            WHERE (?2 IS NULL OR k.id=?2) AND (?1 IS NULL OR p.slug=?1 OR k.target_slug=?1 OR EXISTS (
                SELECT 1 FROM json_each(k.snapshot_ids) j JOIN knowledge_snapshots s ON s.id=j.value
                JOIN knowledge_source_pages m ON m.source_id=s.source_id JOIN wiki_pages origin ON origin.id=m.page_id
                WHERE origin.slug=?1))
            ORDER BY (k.status='pending') DESC,k.created_at ASC LIMIT 200")?;
        let rows = stmt
            .query_map(params![slug, id], |r| {
                Ok(KnowledgeProposal {
                    id: r.get(0)?,
                    page_id: r.get(1)?,
                    target_slug: r.get(2)?,
                    kind: r.get(3)?,
                    title: r.get(4)?,
                    content_md: r.get(5)?,
                    applicable_when: r.get(6)?,
                    snapshot_ids: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
                    event_ids: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
                    base_hash: r.get(9)?,
                    reason: r.get(10)?,
                    status: r.get(11)?,
                    created_at: r.get(12)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn resolve_knowledge_proposal(&self, id: &str, accept: bool) -> Result<Option<WikiPage>> {
        self.apply_knowledge_proposal(id, accept, true)
    }

    /// Only the automatic source compiler uses this path. Its result remains a
    /// reference, and can never overwrite a human decision or edit.
    pub(crate) fn save_automatic_reference(&self, id: &str) -> Result<Option<WikiPage>> {
        self.apply_knowledge_proposal(id, true, false)
    }

    fn apply_knowledge_proposal(
        &self,
        id: &str,
        accept: bool,
        confirmed: bool,
    ) -> Result<Option<WikiPage>> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let page = self.apply_knowledge_proposal_in_tx(id, accept, confirmed)?;
        tx.commit()?;
        Ok(page)
    }

    pub(crate) fn reference_is_unprotected(&self, slug: &str) -> Result<bool> {
        let Some(page) = self.get_wiki_page(slug)? else {
            return Ok(true);
        };
        let metadata = self.knowledge_metadata(slug)?;
        Ok(page.human_edited_at.is_none()
            && page.status != "archived"
            && page.opinion.as_deref() != Some("reject")
            && !matches!(page.kind.as_str(), "source" | "note")
            && metadata.confirmed_at.is_none()
            && metadata.strength == "reference")
    }

    /// Keep old manual decisions and concurrent edits pending, while fresh,
    /// unprotected reference output can be published within the caller's batch.
    pub(crate) fn publish_reference_in_tx(&self, id: &str) -> Result<Option<WikiPage>> {
        let proposal = self
            .find_knowledge_proposals(None, Some(id))?
            .into_iter()
            .next()
            .context("建议不存在")?;
        let origin: String = self.connection.query_row(
            "SELECT origin FROM knowledge_proposals WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if origin != "automatic" || proposal.status == "rejected" {
            return Ok(None);
        }
        if proposal.status == "accepted" {
            return self.get_wiki_page(&proposal.target_slug);
        }
        if !self.reference_is_unprotected(&proposal.target_slug)? {
            return Ok(None);
        }
        let base = self.get_wiki_page(&proposal.target_slug)?;
        if base.as_ref().map(|p| &p.id) != proposal.page_id.as_ref()
            || base.as_ref().map(|p| content_hash(&p.content_md)) != proposal.base_hash
        {
            return Ok(None);
        }
        let blocked: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_proposals WHERE id<>?1 AND target_slug=?2
             AND (status='rejected' OR (status='pending' AND origin='manual')))",
            params![id, proposal.target_slug],
            |r| r.get(0),
        )?;
        if blocked {
            return Ok(None);
        }
        self.apply_knowledge_proposal_in_tx(id, true, false)
    }

    pub(crate) fn apply_knowledge_proposal_in_tx(
        &self,
        id: &str,
        accept: bool,
        confirmed: bool,
    ) -> Result<Option<WikiPage>> {
        if !confirmed {
            let origin: String = self.connection.query_row(
                "SELECT origin FROM knowledge_proposals WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            if origin != "automatic" {
                bail!("人工待审建议不能自动采纳");
            }
        }
        let proposal = self
            .find_knowledge_proposals(None, Some(id))?
            .into_iter()
            .next()
            .context("建议不存在或已过期")?;
        if !confirmed {
            let manual_choice: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_proposals other
                 JOIN json_each(other.snapshot_ids) j JOIN knowledge_snapshots s ON s.id=j.value
                 WHERE other.id<>?1 AND other.target_slug=?3 AND (other.status='rejected' OR (other.status='pending' AND other.origin='manual'))
                 AND s.source_id IN (SELECT source_id FROM knowledge_snapshots WHERE id IN (SELECT value FROM json_each(?2))))",
                params![id,serde_json::to_string(&proposal.snapshot_ids)?,proposal.target_slug], |r| r.get(0))?;
            if manual_choice {
                bail!("整理期间有新的人工决定，保留待审状态");
            }
        }
        if proposal.status != "pending" {
            if accept && proposal.status == "accepted" {
                return self.get_wiki_page(&proposal.target_slug);
            }
            if !accept && proposal.status == "rejected" {
                return Ok(None);
            }
            bail!("建议已经处理，不能更改历史决定");
        }
        let previous_content = self
            .get_wiki_page(&proposal.target_slug)?
            .map(|p| p.content_md);
        let page = if accept {
            let base = self.get_wiki_page(&proposal.target_slug)?;
            let raw_dependencies: Option<String> = self.connection.query_row(
                "SELECT dependency_bases FROM knowledge_proposals WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            let dependencies: Option<Vec<crate::knowledge::dependencies::Basis>> = raw_dependencies
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?;
            if let Some(bases) = &dependencies {
                crate::knowledge::dependencies::validate(self, bases)?;
            } else if let Some(base) = &base {
                anyhow::ensure!(
                    !crate::knowledge::dependencies::stale(self, &base.id)?,
                    "上游知识已变化，请重新检查更新"
                );
            }
            if !confirmed && !self.reference_is_unprotected(&proposal.target_slug)? {
                bail!("已有人工编辑，自动整理保留原知识页");
            }
            if !confirmed {
                let metadata = self.knowledge_metadata(&proposal.target_slug)?;
                if metadata.confirmed_at.is_some() || metadata.strength != "reference" {
                    bail!("人工确认的适用条件与强度不能自动改写");
                }
            }
            if proposal.page_id.is_some() {
                let base = base.as_ref().context("目标页面已删除")?;
                let basis: Option<String> = self.connection.query_row(
                    "SELECT base_state FROM knowledge_proposals WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                let basis = basis
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()?;
                crate::knowledge::authoring::compare_revision_basis(self, base, basis.as_ref())?;
                if Some(&base.id) != proposal.page_id.as_ref()
                    || Some(content_hash(&base.content_md)) != proposal.base_hash
                {
                    bail!("页面已被修改，请重新审阅，不能覆盖新的编辑");
                }
            } else if base.is_some() {
                bail!("同名页面已存在，请重新生成修订建议");
            }
            for id in &proposal.snapshot_ids {
                let snap = self.source_snapshot(id)?.context("来源不存在")?;
                let newest: i64 = self.connection.query_row(
                    "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
                    [&snap.source_id],
                    |r| r.get(0),
                )?;
                let source_active = snap
                    .page_slug
                    .as_deref()
                    .map(|slug| self.get_wiki_page(slug))
                    .transpose()?
                    .flatten()
                    .is_some_and(|p| p.status != "archived");
                if snap.version != newest
                    || snap.opinion.as_deref() == Some("reject")
                    || !source_active
                {
                    bail!("来源已更新或被拒绝，请重新生成建议");
                }
            }
            for id in &proposal.event_ids {
                if !self.recordable_event(id)? {
                    bail!("来源事件已被标为讨论，请重新审阅");
                }
            }
            let tags = base
                .as_ref()
                .map(|p| p.tags.clone())
                .unwrap_or_else(|| vec!["编译知识".into()]);
            let draft = WikiPageDraft {
                slug: proposal.target_slug.clone(),
                kind: proposal.kind.clone(),
                title: proposal.title.clone(),
                summary: proposal.content_md.chars().take(120).collect(),
                content_md: proposal.content_md.clone(),
                tags,
                source_event_ids: proposal.event_ids.clone(),
                status: "active".into(),
                reason: format!(
                    "{}：{}",
                    if confirmed {
                        "用户确认知识建议"
                    } else {
                        "自动整理参考知识"
                    },
                    proposal.reason
                ),
                source_url: None,
            };
            let result = self
                .upsert_wiki_page_in_tx(&draft, ContentPolicy::Always)?
                .page;
            self.connection.execute("UPDATE wiki_pages SET human_edited_at=?1,source_event_ids=?3,evidence_count=?4 WHERE id=?2",
                params![confirmed.then(|| chrono::Utc::now().to_rfc3339()),result.id,serde_json::to_string(&proposal.event_ids)?,proposal.event_ids.len() as i64])?;
            self.connection.execute(
                "DELETE FROM knowledge_page_sources WHERE page_id=?1",
                [&result.id],
            )?;
            self.bind_page_sources(&result.id, &proposal.snapshot_ids)?;
            if let Some(bases) = &dependencies {
                crate::knowledge::dependencies::bind(self, &result.id, bases)?;
            }
            // Confirming a revision must not silently downgrade a personal rule.
            let strength = if confirmed && base.is_some() {
                self.knowledge_metadata(&result.slug)?.strength
            } else {
                "reference".into()
            };
            self.connection.execute("INSERT INTO knowledge_metadata(page_id,applicable_when,strength,confirmed_at)
                VALUES (?1,?2,?4,?3) ON CONFLICT(page_id) DO UPDATE SET
                applicable_when=excluded.applicable_when,strength=excluded.strength,confirmed_at=excluded.confirmed_at",
                params![result.id,proposal.applicable_when,confirmed.then(|| chrono::Utc::now().to_rfc3339()),strength])?;
            self.append_wiki_log(&format!(
                "{} {} → {}",
                if confirmed {
                    "确认知识建议"
                } else {
                    "自动整理参考知识"
                },
                proposal.id,
                result.slug
            ))?;
            Some(
                self.get_wiki_page(&result.slug)?
                    .context("保存后页面不存在")?,
            )
        } else {
            self.append_wiki_log(&format!("拒绝知识建议 {}（保留历史）", proposal.id))?;
            None
        };
        self.connection.execute(
            "UPDATE knowledge_proposals SET status=?1,resolved_at=?2 WHERE id=?3",
            params![
                if accept { "accepted" } else { "rejected" },
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )?;
        let revision: Option<String> = if let Some(p) = &page {
            self.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&p.id],|r|r.get(0)).optional()?
        } else {
            None
        };
        self.connection.execute("INSERT OR IGNORE INTO knowledge_review_decisions(proposal_id,original_content,original_applicable,selected_parts,issue_ids,created_at,before_content,result_content,result_applicable,revision_id)
            VALUES(?1,?2,?3,'[]','[]',?4,?5,?6,?7,?8)",params![id,proposal.content_md,proposal.applicable_when,chrono::Utc::now().to_rfc3339(),previous_content,page.as_ref().map(|p|&p.content_md),accept.then_some(&proposal.applicable_when),revision])?;
        Ok(page)
    }

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
        let mut issues = Vec::new();
        for page in pages.into_iter().filter(|p| p.status != "archived") {
            let mut add = |kind: &str, detail: String, key: String| -> Result<()> {
                let fingerprint = content_hash(&format!("{}|{kind}|{key}", page.id));
                let reviewed:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_maintenance_reviews WHERE fingerprint=?1)",[&fingerprint],|r|r.get(0))?;
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
                if !self.recordable_event(id)? {
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
        let issue = self
            .knowledge_issues(None)?
            .into_iter()
            .find(|i| i.fingerprint == fingerprint)
            .context("提示已变化，请刷新")?;
        let page = self
            .get_wiki_page(&issue.page_slug)?
            .context("页面不存在")?;
        self.connection.execute("INSERT OR IGNORE INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,issue_description)
            VALUES (?1,?2,'dismissed',?3,?4)",params![fingerprint,page.id,chrono::Utc::now().to_rfc3339(),issue.description])?;
        Ok(())
    }
}
