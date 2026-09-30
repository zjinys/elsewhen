//! Bounded workflow endpoints. Background work still has no manual trigger.
use crate::{
    knowledge::{organization, workflows as flow},
    storage::Store,
};
use anyhow::{ensure, Context, Result};
fn store() -> Result<Store> {
    Store::open(&crate::config::AppConfig::load()?.database_path)
}

pub fn browse_knowledge(
    query: String,
    area: Option<String>,
    kind: Option<String>,
    tag: Option<String>,
    state: Option<String>,
    offset: i64,
) -> Result<flow::LibraryPage> {
    flow::browse(
        &store()?,
        &query,
        area.as_deref(),
        kind.as_deref(),
        tag.as_deref(),
        state.as_deref(),
        offset,
    )
}
pub fn knowledge_reading_state(slug: String) -> Result<String> {
    flow::reading_state(&store()?, &slug)
}
pub fn set_knowledge_reading_state(slug: String, state: String) -> Result<()> {
    flow::set_reading_state(&store()?, &slug, &state)
}
pub fn list_artifact_versions(slug: String, offset: i64) -> Result<Vec<flow::ArtifactVersion>> {
    flow::versions(&store()?, &slug, offset)
}
pub fn adopt_artifact_version(slug: String, revision_id: String, adopt: bool) -> Result<()> {
    flow::adopt(&store()?, &slug, &revision_id, adopt)
}
pub fn topic_organization_history(
    slug: String,
    offset: i64,
) -> Result<Vec<organization::TopicOrganizationPreview>> {
    organization::history(&store()?, &slug, offset)
}
pub fn undo_topic_organization(id: String) -> Result<()> {
    organization::undo(&store()?, &id)
}
pub fn get_suggestion_feedback(message_id: String) -> Result<Option<flow::SuggestionFeedback>> {
    flow::feedback(&store()?, &message_id)
}
pub fn save_suggestion_feedback(
    message_id: String,
    decision: String,
    suggestion: String,
    rewrite: Option<String>,
) -> Result<()> {
    flow::save_feedback(
        &store()?,
        &message_id,
        &decision,
        &suggestion,
        rewrite.as_deref(),
    )
}

pub fn create_artifact_from_message(
    slug: String,
    message_id: String,
    content_type: String,
    title: String,
) -> Result<super::wiki::WikiPageDto> {
    let store = store()?;
    let (conversation, role, content): (String, String, String) = store.connection.query_row(
        "SELECT conversation_id,role,content FROM messages WHERE id=?1",
        [message_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    ensure!(role == "assistant", "只能从 AI 回复保存产物");
    let expected = store
        .find_wiki_chat_conversation(&slug)?
        .context("页面对话不存在")?;
    ensure!(expected == conversation, "回复不属于当前页面对话");
    let page = store.create_derivative_with_generation(
        &slug,
        &content_type,
        &title,
        &content,
        "用户从页内回复保存产物",
        Some(&conversation),
    )?;
    Ok(page.into())
}

#[derive(Clone, Debug)]
pub struct BatchReviewResult {
    pub id: String,
    pub success: bool,
    pub detail: String,
}
pub fn resolve_knowledge_batch(ids: Vec<String>, accept: bool) -> Result<Vec<BatchReviewResult>> {
    ensure!(!ids.is_empty() && ids.len() <= 20, "每批请选择 1–20 项修订");
    let store = store()?;
    let mut seen = std::collections::HashSet::new();
    let mut results = vec![];
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let result = (|| -> Result<()> {
            let revision: bool = store.connection.query_row(
                "SELECT page_id IS NOT NULL AND base_hash IS NOT NULL FROM knowledge_proposals WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            ensure!(revision, "批量审阅只处理正文修订");
            store.resolve_knowledge_proposal(&id, accept)?;
            Ok(())
        })();
        results.push(BatchReviewResult {
            id,
            success: result.is_ok(),
            detail: match result {
                Ok(()) => if accept { "已采纳" } else { "已拒绝" }.into(),
                Err(e) => format!("未处理：{e}"),
            },
        });
    }
    Ok(results)
}

/// Load exactly the revision displayed in a comparison; id and body come from
/// one SQLite read, so a concurrent edit cannot pair old prose with a new id.
pub fn read_artifact_version(slug: String, revision_id: Option<String>) -> Result<super::knowledge::KnowledgeRevisionDto> {
    Ok(store()?.connection.query_row("SELECT r.id,r.content_md,r.reason,r.created_at FROM wiki_revisions r JOIN wiki_pages p ON p.id=r.page_id WHERE p.slug=?1 AND (?2 IS NULL OR r.id=?2) ORDER BY r.created_at DESC,r.rowid DESC LIMIT 1",rusqlite::params![slug,revision_id],|r|Ok(super::knowledge::KnowledgeRevisionDto{id:r.get(0)?,content_md:r.get(1)?,reason:r.get(2)?,created_at:r.get(3)?}))?)
}
