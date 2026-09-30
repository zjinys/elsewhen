//! Human maintenance commands and their readable audit history.
use crate::storage::{KnowledgeIssue, Store, WikiPage};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

fn issue(store: &Store, fingerprint: &str) -> Result<KnowledgeIssue> {
    store
        .knowledge_issues(None)?
        .into_iter()
        .find(|i| i.fingerprint == fingerprint)
        .context("提示已变化或已处理，请刷新")
}

fn can_resolve(store: &Store, issue: &KnowledgeIssue, page: &WikiPage) -> Result<bool> {
    if matches!(page.kind.as_str(), "source" | "note")
        || super::citation_for_page(store, page, "冲突处理".into(), 100)?.is_none()
    {
        return Ok(false);
    }
    if issue.page_slug == page.slug {
        return Ok(true);
    }
    let raw: Option<String> = store
        .connection
        .query_row(
            "SELECT snapshot_ids FROM knowledge_semantic_issues WHERE fingerprint=?1",
            [&issue.fingerprint],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(false);
    };
    let required: Vec<String> = serde_json::from_str(&raw)?;
    let actual = store.page_source_snapshots(&page.slug)?;
    Ok(!required.is_empty() && required.iter().all(|id| actual.iter().any(|s| &s.id == id)))
}

pub(crate) fn resolution_targets(store: &Store, fingerprint: &str) -> Result<Vec<WikiPage>> {
    let issue = issue(store, fingerprint)?;
    let mut out = Vec::new();
    for p in store.list_wiki_pages(None, None)? {
        if can_resolve(store, &issue, &p)? {
            out.push(p);
        }
    }
    Ok(out)
}

pub(crate) fn resolve_issue(
    store: &Store,
    fingerprint: &str,
    slug: &str,
    revision: &str,
    note: &str,
) -> Result<()> {
    ensure!(
        !note.trim().is_empty() && note.chars().count() <= 1000,
        "请说明本次修订如何处理分歧（最多 1000 字）"
    );
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    let issue = issue(store, fingerprint)?;
    let page = store
        .get_wiki_page(slug)?
        .context("解决问题的知识页不存在")?;
    ensure!(
        can_resolve(store, &issue, &page)?,
        "修订页未覆盖提示的全部有效来源，不能关联解决"
    );
    let latest:(String,String)=store.connection.query_row("SELECT id,content_md FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&page.id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    ensure!(
        latest.0 == revision && latest.1 == page.content_md,
        "修订已经变化，请重新选择当前版本"
    );
    let origin = store
        .get_wiki_page(&issue.page_slug)?
        .context("提示来源页不存在")?;
    store.connection.execute("INSERT INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,revision_id,issue_description,resolution_note,target_page_id)
        VALUES(?1,?2,'resolved',?3,?4,?5,?6,?7)",params![fingerprint,origin.id,chrono::Utc::now().to_rfc3339(),revision,issue.description,note.trim(),page.id])?;
    store.append_wiki_log(&format!(
        "用户将提示 {fingerprint} 关联到 {slug} 修订 {revision} 解决"
    ))?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct KnowledgeReviewRecord {
    pub id: String,
    pub action: String,
    pub title: String,
    pub created_at: String,
    pub before_content: Option<String>,
    pub original_content: Option<String>,
    pub result_content: Option<String>,
    pub original_applicable: Option<String>,
    pub result_applicable: Option<String>,
    pub selected_parts: Vec<i64>,
    pub description: String,
    pub note: Option<String>,
    pub revision_id: Option<String>,
    pub target_slug: Option<String>,
}

pub(crate) fn history(
    store: &Store,
    slug: &str,
    offset: i64,
) -> Result<Vec<KnowledgeReviewRecord>> {
    ensure!(offset >= 0, "历史记录位置无效");
    // Each saved decision is immutable. Old rows explicitly lack unavailable
    // before/selection details rather than reconstructing them from today's page.
    let sql="SELECT * FROM (
        SELECT k.id,k.status action,k.title,COALESCE(k.resolved_at,k.created_at) at,
            d.before_content,COALESCE(d.original_content,k.content_md),
            CASE WHEN k.status='accepted' THEN COALESCE(d.result_content,k.content_md) END,
            COALESCE(d.original_applicable,k.applicable_when),CASE WHEN k.status='accepted' THEN COALESCE(d.result_applicable,k.applicable_when) END,
            COALESCE(d.selected_parts,'[]'),k.reason,NULL,d.revision_id,COALESCE(p.slug,k.target_slug)
        FROM knowledge_proposals k LEFT JOIN knowledge_review_decisions d ON d.proposal_id=k.id LEFT JOIN wiki_pages p ON p.id=k.page_id
        WHERE k.status<>'pending' AND (p.slug=?1 OR k.target_slug=?1 OR EXISTS(
            SELECT 1 FROM json_each(k.snapshot_ids) j JOIN knowledge_snapshots s ON s.id=j.value JOIN knowledge_source_pages m ON m.source_id=s.source_id JOIN wiki_pages o ON o.id=m.page_id WHERE o.slug=?1))
        UNION ALL
        SELECT m.fingerprint,m.resolution,'维护提示',m.created_at,NULL,NULL,r.content_md,NULL,NULL,'[]',
            COALESCE(m.issue_description,i.description,'历史提示未保存说明'),m.resolution_note,m.revision_id,t.slug
        FROM knowledge_maintenance_reviews m JOIN wiki_pages o ON o.id=m.page_id LEFT JOIN wiki_pages t ON t.id=m.target_page_id
        LEFT JOIN knowledge_semantic_issues i ON i.fingerprint=m.fingerprint LEFT JOIN wiki_revisions r ON r.id=m.revision_id
        WHERE o.slug=?1 OR t.slug=?1
        ) ORDER BY at DESC,id DESC LIMIT 50 OFFSET ?2";
    Ok(store
        .connection
        .prepare(sql)?
        .query_map(params![slug, offset], |r| {
            Ok(KnowledgeReviewRecord {
                id: r.get(0)?,
                action: r.get(1)?,
                title: r.get(2)?,
                created_at: r.get(3)?,
                before_content: r.get(4)?,
                original_content: r.get(5)?,
                result_content: r.get(6)?,
                original_applicable: r.get(7)?,
                result_applicable: r.get(8)?,
                selected_parts: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
                description: r.get(10)?,
                note: r.get(11)?,
                revision_id: r.get(12)?,
                target_slug: r.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}
