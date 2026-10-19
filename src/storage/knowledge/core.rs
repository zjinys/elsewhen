//! 来源快照基础：结构体、内容指纹、locator 规范化与 backfill/capture 管线。

use super::*;

pub struct RejectedSourceRow {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub identity: String,
    pub source_kind: String,
    pub locator: Option<String>,
    pub reject_reason: String,
    pub created_at: String,
}

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

pub(crate) fn source_identity(locator: Option<&str>, content: &str) -> String {
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
    /// AI 编译产出的挂载摘要（迁移 49 前的建议行为空，应用时退回正文预览）
    pub summary: String,
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

pub(crate) const SNAPSHOT_SELECT: &str = "SELECT v.id,v.source_id,v.version,v.title,v.content_md,
 v.content_hash,v.captured_at,s.kind,s.locator,s.opinion,
 (SELECT p.slug FROM knowledge_source_pages m JOIN wiki_pages p ON p.id=m.page_id
  WHERE m.source_id=s.id ORDER BY p.created_at,p.id LIMIT 1)
 FROM knowledge_snapshots v JOIN knowledge_sources s ON s.id=v.source_id";

pub(crate) fn snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceSnapshot> {
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
