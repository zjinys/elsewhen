//! Knowledge sources, review and provenance. Writes are explicit GUI commands;
//! background digest and model tools never call the confirmation endpoints.
use super::*;
use crate::knowledge::KnowledgeCitation;
use crate::storage::{KnowledgeIssue, KnowledgeMetadata, KnowledgeProposal, SourceSnapshot};

fn store() -> Result<Store> {
    Store::open(&crate::config::AppConfig::load()?.database_path)
}

/// Background worker only; no manual insight trigger in the UI.
pub fn tick_knowledge_insights() -> Result<i64> {
    let store = store()?;
    let Some((provider, _)) = crate::wiki::digest_provider(&store)? else {
        return Ok(0);
    };
    let compiled =
        crate::knowledge_background::run_automatic_source_compilation(&store, &provider)?;
    Ok(compiled + crate::knowledge_background::run_automatic_insights(&store, &provider)?)
}

#[derive(Clone, Debug)]
pub struct KnowledgeBackgroundRunDto {
    pub task: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub result_count: i64,
}

pub fn list_knowledge_background_runs() -> Result<Vec<KnowledgeBackgroundRunDto>> {
    let store = store()?;
    let mut stmt = store.connection.prepare(
        "SELECT status,started_at,finished_at,error,result_count,task
        FROM knowledge_background_runs ORDER BY started_at DESC LIMIT 50",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(KnowledgeBackgroundRunDto {
                task: r.get(5)?,
                status: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                error: r.get(3)?,
                result_count: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Clone, Debug)]
pub struct KnowledgePageDetails {
    pub source_pages: Vec<WikiPageDto>,
    pub output_pages: Vec<WikiPageDto>,
    pub sources: Vec<SourceSnapshot>,
    pub history: Vec<SourceSnapshot>,
    pub proposals: Vec<KnowledgeProposal>,
    pub issues: Vec<KnowledgeIssue>,
    pub metadata: KnowledgeMetadata,
}

pub fn get_knowledge_page_details(slug: String) -> Result<KnowledgePageDetails> {
    let store = store()?;
    Ok(KnowledgePageDetails {
        source_pages: store
            .knowledge_origin_pages(&slug)?
            .into_iter()
            .map(WikiPageDto::from)
            .collect(),
        output_pages: store
            .knowledge_output_pages(&slug)?
            .into_iter()
            .map(WikiPageDto::from)
            .collect(),
        sources: store.page_source_snapshots(&slug)?,
        history: store.source_history(&slug)?,
        proposals: store.knowledge_proposals(Some(&slug))?,
        issues: store.knowledge_issues(Some(&slug))?,
        metadata: store.knowledge_metadata(&slug)?,
    })
}

pub fn get_knowledge_source_snapshot(id: String) -> Result<Option<SourceSnapshot>> {
    store()?.source_snapshot(&id)
}

pub fn list_knowledge_proposals() -> Result<Vec<KnowledgeProposal>> {
    store()?.knowledge_proposals(None)
}
pub fn list_knowledge_issues() -> Result<Vec<KnowledgeIssue>> {
    store()?.knowledge_issues(None)
}

pub fn propose_knowledge_page(slug: String, kind: String) -> Result<String> {
    if kind == "auto" {
        anyhow::bail!("自动整理仅由后台调度");
    }
    let store = store()?;
    let Some((provider, _)) = crate::wiki::digest_provider(&store)? else {
        anyhow::bail!("请先在设置中配置 AI Provider");
    };
    crate::knowledge::propose_knowledge(&store, &slug, &kind, &provider)
}

pub fn resolve_knowledge_proposal(id: String, accept: bool) -> Result<Option<WikiPageDto>> {
    Ok(store()?
        .resolve_knowledge_proposal(&id, accept)?
        .map(WikiPageDto::from))
}

pub fn update_knowledge_metadata(
    slug: String,
    applicable_when: String,
    strength: String,
) -> Result<()> {
    store()?.set_knowledge_metadata(&slug, &applicable_when, &strength)
}

pub fn dismiss_knowledge_issue(fingerprint: String) -> Result<()> {
    store()?.dismiss_knowledge_issue(&fingerprint)
}

#[derive(Clone, Debug)]
pub struct KnowledgeRevisionDto {
    pub content_md: String,
    pub reason: String,
    pub created_at: String,
}

pub fn list_knowledge_revisions(slug: String) -> Result<Vec<KnowledgeRevisionDto>> {
    let store = store()?;
    let mut stmt=store.connection.prepare("SELECT r.content_md,r.reason,r.created_at FROM wiki_revisions r
        JOIN wiki_pages p ON p.id=r.page_id WHERE p.slug=?1 ORDER BY r.created_at DESC,r.rowid DESC LIMIT 50")?;
    let rows = stmt
        .query_map([slug], |r| {
            Ok(KnowledgeRevisionDto {
                content_md: r.get(0)?,
                reason: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_knowledge_citations(task: String, owner_id: String) -> Result<Vec<KnowledgeCitation>> {
    crate::knowledge::latest_citations(&store()?, &task, &owner_id)
}

pub fn get_message_knowledge_citations(message_id: String) -> Result<Vec<KnowledgeCitation>> {
    use rusqlite::OptionalExtension;
    let store = store()?;
    let message:Option<(String,String,String)>=store.connection.query_row(
        "SELECT conversation_id,content,created_at FROM messages WHERE id=?1 AND role='assistant'",[message_id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((conversation, content, created_at)) = message else {
        return Ok(vec![]);
    };
    let owner = format!(
        "{conversation}:{}",
        crate::storage::knowledge::content_hash(&content)
    );
    let raw:Option<String>=store.connection.query_row("SELECT cited_json FROM knowledge_usage
        WHERE task='conversation' AND owner_id=?1 AND created_at<=?2 ORDER BY created_at DESC,rowid DESC LIMIT 1",
        rusqlite::params![owner,created_at],|r|r.get(0)).optional()?;
    raw.map(|r| serde_json::from_str(&r).map_err(Into::into))
        .unwrap_or(Ok(vec![]))
}

#[derive(Clone, Debug)]
pub struct SourceUpdatePreview {
    pub existing_slug: Option<String>,
    pub previous_content: Option<String>,
    pub previous_snapshot_id: Option<String>,
    pub changed: bool,
}

pub fn preview_knowledge_source(
    source_url: String,
    content_md: String,
) -> Result<SourceUpdatePreview> {
    let store = store()?;
    let existing = store.matching_source_page(Some(&source_url), content_md.trim())?;
    let snapshot = existing
        .as_ref()
        .map(|p| store.source_history(&p.slug))
        .transpose()?
        .and_then(|s| s.into_iter().next());
    Ok(SourceUpdatePreview {
        existing_slug: existing.map(|p| p.slug),
        previous_content: snapshot.as_ref().map(|s| s.content_md.clone()),
        previous_snapshot_id: snapshot.as_ref().map(|s| s.id.clone()),
        changed: snapshot
            .as_ref()
            .is_some_and(|s| s.content_md != content_md.trim()),
    })
}

/// Save the exact preview only if its base snapshot is still current.
pub fn confirm_knowledge_source(
    title: String,
    content_md: String,
    source_url: String,
    source_kind: String,
    tags: Vec<String>,
    expected_snapshot_id: Option<String>,
) -> Result<WikiPageDto> {
    let store = store()?;
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let existing = store.matching_source_page(Some(&source_url), content_md.trim())?;
    let current = existing
        .as_ref()
        .map(|p| store.source_history(&p.slug))
        .transpose()?
        .and_then(|s| s.into_iter().next());
    if current.as_ref().map(|s| s.id.as_str()) != expected_snapshot_id.as_deref() {
        if let Some(current) = &current {
            if current.content_md == content_md.trim() {
                return Ok(WikiPageDto::from(existing.context("来源页不存在")?));
            }
        }
        anyhow::bail!("来源已被更新，请重新预览后保存");
    }
    if content_md.trim().is_empty() {
        anyhow::bail!("内容为空");
    }
    let mut tags = tags;
    tags.push("import".into());
    tags.push(
        if source_kind == "tweet" {
            "tweet"
        } else {
            "web"
        }
        .into(),
    );
    tags.sort();
    tags.dedup();
    let draft = crate::storage::WikiPageDraft {
        slug: existing.map(|p| p.slug).unwrap_or_else(|| {
            if source_kind == "tweet" {
                crate::wiki::extract_tweet_id(&source_url)
                    .map(|id| format!("tweet-{id}"))
                    .unwrap_or_else(|| format!("import-{}", &uuid::Uuid::new_v4().to_string()[..8]))
            } else {
                format!("import-{}", &uuid::Uuid::new_v4().to_string()[..8])
            }
        }),
        kind: "source".into(),
        title: if title.trim().is_empty() {
            "未命名导入".into()
        } else {
            title.trim().into()
        },
        summary: content_md.trim().chars().take(120).collect(),
        content_md: content_md.trim().into(),
        tags,
        source_event_ids: vec![],
        status: "active".into(),
        reason: "用户确认来源快照".into(),
        source_url: Some(source_url),
    };
    let page = store
        .upsert_wiki_page_in_tx(&draft, crate::storage::ContentPolicy::Always)?
        .page;
    tx.commit()?;
    Ok(WikiPageDto::from(page))
}
