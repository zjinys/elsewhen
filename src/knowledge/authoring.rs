//! Bind authored knowledge to the evidence actually supplied to the conversation.
use super::{current_candidates, latest_citations, KnowledgeCitation};
use crate::storage::{knowledge::content_hash, ContentPolicy, Store, WikiPage, WikiPageDraft};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Transaction, TransactionBehavior};
use serde_json::{json, Value};

pub(crate) fn previous_answer_sources(
    store: &Store,
    conversation: &str,
) -> Result<Vec<KnowledgeCitation>> {
    let Some(message) = store
        .list_messages(conversation)?
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
    else {
        return Ok(vec![]);
    };
    latest_citations(
        store,
        "conversation",
        &format!("{conversation}:{}", content_hash(&message.content)),
    )
}

pub(crate) fn draft_sources(
    store: &Store,
    conversation: &str,
    args: &Value,
    supplied: &[KnowledgeCitation],
) -> Result<Vec<KnowledgeCitation>> {
    let mut available = supplied.to_vec();
    for old in previous_answer_sources(store, conversation)? {
        if !available.iter().any(|c| c.page_slug == old.page_slug) {
            available.push(old);
        }
    }
    let mut requested: Vec<String> = match args.get("source_slugs") {
        Some(v) => serde_json::from_value(v.clone()).context("source_slugs 必须是页面标识数组")?,
        None => vec![],
    };
    // Existing rendered answer links also carry an explicit source selection.
    for part in args["content_md"]
        .as_str()
        .unwrap_or("")
        .split("[[")
        .skip(1)
    {
        if let Some((slug, _)) = part.split_once("]]") {
            let slug = slug
                .split('|')
                .next()
                .unwrap_or(slug)
                .trim()
                .trim_start_matches("kb:");
            if available.iter().any(|c| c.page_slug == slug) || part.starts_with("kb:") {
                requested.push(slug.to_owned());
            }
        }
    }
    requested.sort();
    requested.dedup();
    ensure!(requested.len() <= 8, "每份草稿最多使用 8 个知识来源");
    let mut selected = Vec::new();
    for slug in requested {
        selected.push(
            available
                .iter()
                .find(|c| c.page_slug == slug)
                .with_context(|| format!("来源 {slug} 未提供给当前对话，请先检索或打开后再保存"))?
                .clone(),
        );
    }
    ensure!(
        current_candidates(store, &selected)?.len() == selected.len(),
        "草拟期间来源已变化或被拒绝，请重新检索"
    );
    ensure!(
        selected
            .iter()
            .all(|c| !c.sources.is_empty() || !c.event_ids.is_empty()),
        "所选页面仅有链接，没有可继承的原料或事件依据；请先导入原料，或保存为自由笔记"
    );
    Ok(selected)
}

pub(crate) fn save_draft(store: &Store, draft: &WikiPageDraft, args: &Value) -> Result<WikiPage> {
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    if let Some(existing) = store.find_wiki_page_by_title(&draft.title)? {
        tx.commit()?;
        return Ok(existing);
    }
    // This is an internal field created by draft_sources, never copied from tool arguments.
    let evidence: Vec<KnowledgeCitation> = args
        .get("verified_evidence")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    ensure!(
        current_candidates(store, &evidence)?.len() == evidence.len(),
        "来源已变化或被拒绝，请重新生成草稿；尚未保存"
    );
    let applicable = args["applicable_when"].as_str().unwrap_or("").trim();
    ensure!(applicable.chars().count() <= 600, "适用条件最多 600 字");
    let mut draft = draft.clone();
    draft.source_event_ids = evidence.iter().flat_map(|c| c.event_ids.clone()).collect();
    draft.source_event_ids.sort();
    draft.source_event_ids.dedup();
    let mut snapshots: Vec<_> = evidence
        .iter()
        .flat_map(|c| c.sources.iter().map(|s| s.snapshot_id.clone()))
        .collect();
    snapshots.sort();
    snapshots.dedup();
    ensure!(
        snapshots.len() <= 8,
        "草稿来源超出 8 份原料，请拆分知识主题"
    );
    let page = store
        .upsert_wiki_page_in_tx(&draft, ContentPolicy::Always)?
        .page;
    store.bind_page_sources(&page.id, &snapshots)?;
    let bases = super::dependencies::capture(
        store,
        &evidence
            .iter()
            .map(|c| c.page_slug.clone())
            .collect::<Vec<_>>(),
    )?;
    super::dependencies::bind(store, &page.id, &bases)?;
    store.confirm_authored_metadata_in_tx(&page.id, applicable, "reference")?;
    store.append_wiki_log(&format!(
        "确认对话知识：{}，{} 份原料，{} 个事件",
        page.slug,
        snapshots.len(),
        draft.source_event_ids.len()
    ))?;
    tx.commit()?;
    store
        .get_wiki_page(&page.slug)?
        .context("保存后知识页不存在")
}

/// A revision confirmation covers both content and the rule metadata seen by the user.
pub(crate) fn revision_basis(store: &Store, page: &WikiPage) -> Result<Value> {
    let metadata = store.knowledge_metadata(&page.slug)?;
    let mut value = json!({"page_id":page.id,"hash":content_hash(&page.content_md),"status":page.status,"opinion":page.opinion,
        "snapshots":store.page_source_snapshots(&page.slug)?.iter().map(|s|&s.id).collect::<Vec<_>>(),
        "events":page.source_event_ids,"strength":metadata.strength,"applicable_when":metadata.applicable_when,"confirmed_at":metadata.confirmed_at,
        "dependencies":super::dependencies::state(store,&page.id)?});
    // Keep v7 proposal/confirmation signatures stable when no new knowledge
    // dependency exists. Unknown legacy derivative bases must still be reviewed.
    if value["dependencies"].as_array().is_some_and(Vec::is_empty) {
        value.as_object_mut().unwrap().remove("dependencies");
    }
    Ok(value)
}

pub(crate) fn validate_revision_basis(
    store: &Store,
    page: &WikiPage,
    expected: Option<&Value>,
) -> Result<()> {
    compare_revision_basis(store, page, expected)?;
    ensure!(
        !super::dependencies::stale(store, &page.id)?,
        "上游知识已变化，请先检查更新并审阅"
    );
    validate_retained_sources(store, page)
}

pub(crate) fn compare_revision_basis(
    store: &Store,
    page: &WikiPage,
    expected: Option<&Value>,
) -> Result<()> {
    let current = revision_basis(store, page)?;
    if let Some(expected) = expected {
        ensure!(
            expected == &current,
            "页面、来源或规则设置已变化，请重新审阅"
        );
    } else {
        ensure!(
            current["strength"] == "reference",
            "旧规则草稿缺少确认依据，请重新生成修订"
        );
    }
    Ok(())
}

fn validate_retained_sources(store: &Store, page: &WikiPage) -> Result<()> {
    for source in store.page_source_snapshots(&page.slug)? {
        let latest: i64 = store.connection.query_row(
            "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
            [&source.source_id],
            |r| r.get(0),
        )?;
        ensure!(
            source.version == latest && source.opinion.as_deref() != Some("reject"),
            "来源已变化或被拒绝，请先检查更新"
        );
    }
    for id in &page.source_event_ids {
        ensure!(store.recordable_event(id)?, "来源事件已失效，请重新审阅");
    }
    Ok(())
}

impl Store {
    pub(crate) fn confirm_authored_metadata_in_tx(
        &self,
        page_id: &str,
        applicable: &str,
        strength: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE wiki_pages SET human_edited_at=?2 WHERE id=?1",
            params![page_id, now],
        )?;
        self.connection.execute("INSERT INTO knowledge_metadata(page_id,applicable_when,strength,confirmed_at) VALUES(?1,?2,?3,?4)
            ON CONFLICT(page_id) DO UPDATE SET applicable_when=excluded.applicable_when,strength=excluded.strength,confirmed_at=excluded.confirmed_at",
            params![page_id,applicable,strength,now])?;
        Ok(())
    }
}
