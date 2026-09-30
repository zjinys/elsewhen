//! Human review operates on a frozen base and records the exact accepted changes.
use crate::storage::{knowledge::content_hash, Store, WikiPage, WikiPageDraft};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Transaction, TransactionBehavior};

#[derive(Clone, Debug)]
pub struct KnowledgeDiffPart {
    pub before: String,
    pub after: String,
    pub changed: bool,
}

/// Paragraph LCS gives stable, selectable edits without splitting Markdown lines.
pub fn diff(before: &str, after: &str) -> Vec<KnowledgeDiffPart> {
    let a: Vec<_> = before.split("\n\n").collect();
    let b: Vec<_> = after.split("\n\n").collect();
    if a.len() > 500 || b.len() > 500 {
        return vec![KnowledgeDiffPart {
            before: before.into(),
            after: after.into(),
            changed: before != after,
        }];
    }
    let mut lcs = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out: Vec<KnowledgeDiffPart> = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    fn flush(out: &mut Vec<KnowledgeDiffPart>, a: &mut Vec<&str>, b: &mut Vec<&str>) {
        if !a.is_empty() || !b.is_empty() {
            out.push(KnowledgeDiffPart {
                before: a.join("\n\n"),
                after: b.join("\n\n"),
                changed: true,
            });
            a.clear();
            b.clear();
        }
    }
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            flush(&mut out, &mut removed, &mut added);
            out.push(KnowledgeDiffPart {
                before: a[i].into(),
                after: b[j].into(),
                changed: false,
            });
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            removed.push(a[i]);
            i += 1;
        } else {
            added.push(b[j]);
            j += 1;
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}

pub fn proposal_diff(store: &Store, id: &str) -> Result<Vec<KnowledgeDiffPart>> {
    let p = store
        .find_knowledge_proposals(None, Some(id))?
        .into_iter()
        .find(|p| p.id == id)
        .context("提案不存在")?;
    let base = store.get_wiki_page(&p.target_slug)?;
    Ok(diff(
        base.as_ref().map(|p| p.content_md.as_str()).unwrap_or(""),
        &p.content_md,
    ))
}

pub fn accept_parts(
    store: &Store,
    id: &str,
    selected: &[i64],
    accept_applicability: bool,
    issues: &[String],
) -> Result<WikiPage> {
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    let proposal = store
        .find_knowledge_proposals(None, Some(id))?
        .into_iter()
        .find(|p| p.id == id)
        .context("提案不存在")?;
    ensure!(proposal.status == "pending", "提案已经处理，请刷新");
    let base = store.get_wiki_page(&proposal.target_slug)?;
    let parts = proposal_diff(store, id)?;
    ensure!(
        selected
            .iter()
            .all(|i| *i >= 0 && parts.get(*i as usize).is_some_and(|p| p.changed)),
        "选中的差异已失效"
    );
    let partial = parts
        .iter()
        .enumerate()
        .any(|(i, p)| p.changed && !selected.contains(&(i as i64)));
    ensure!(
        !selected.is_empty() || (accept_applicability && !partial),
        "至少选择一处修改"
    );
    if partial {
        let page = base.as_ref().context("新知识请完整确认")?;
        ensure!(
            !super::dependencies::stale(store, &page.id)?,
            "上游知识有变化，请完整审阅确认"
        );
        let mut old: Vec<_> = store
            .page_source_snapshots(&page.slug)?
            .iter()
            .map(|s| s.id.clone())
            .collect();
        old.sort();
        let mut new = proposal.snapshot_ids.clone();
        new.sort();
        // Avoid falsely attaching a new source version to unrevised paragraphs.
        ensure!(
            old == new && page.source_event_ids == proposal.event_ids,
            "本次来源依据也有变化，请阅读全文后完整确认，不能仅更新部分段落的来源"
        );
    }
    let content = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if !p.changed || selected.contains(&(i as i64)) {
                p.after.as_str()
            } else {
                p.before.as_str()
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    ensure!(!content.trim().is_empty(), "采纳后的正文不能为空");
    let applicable = if accept_applicability {
        proposal.applicable_when.clone()
    } else {
        store
            .knowledge_metadata(&proposal.target_slug)?
            .applicable_when
    };
    let visible = store.knowledge_issues(Some(&proposal.target_slug))?;
    ensure!(
        issues
            .iter()
            .all(|id| visible.iter().any(|i| &i.fingerprint == id)),
        "关联提示已变化，请刷新"
    );
    // Preserve the original suggestion in the decision record. The proposal row
    // contains the exact human-selected result used by the existing validator.
    store.connection.execute("INSERT INTO knowledge_review_decisions(proposal_id,original_content,original_applicable,selected_parts,issue_ids,created_at,before_content,result_content,result_applicable) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,proposal.content_md,proposal.applicable_when,serde_json::to_string(selected)?,serde_json::to_string(issues)?,chrono::Utc::now().to_rfc3339(),base.as_ref().map(|p|&p.content_md),content,applicable])?;
    store.connection.execute(
        "UPDATE knowledge_proposals SET content_md=?2,applicable_when=?3 WHERE id=?1",
        params![id, content, applicable],
    )?;
    let page = store
        .apply_knowledge_proposal_in_tx(id, true, true)?
        .context("保存失败")?;
    let revision:String=store.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&page.id],|r|r.get(0))?;
    for fingerprint in issues {
        let description = &visible
            .iter()
            .find(|i| &i.fingerprint == fingerprint)
            .context("提示已变化")?
            .description;
        store.connection.execute("INSERT INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,revision_id,issue_description,target_page_id) VALUES(?1,?2,'resolved',?3,?4,?5,?2)",params![fingerprint,page.id,chrono::Utc::now().to_rfc3339(),revision,description])?;
    }
    store.connection.execute(
        "UPDATE knowledge_review_decisions SET revision_id=?2 WHERE proposal_id=?1",
        params![id, revision],
    )?;
    tx.commit()?;
    Ok(page)
}

/// Restoring historical text is itself reviewable against today's sources and rules.
pub fn restore_proposal(store: &Store, slug: &str, revision: &str) -> Result<String> {
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    let page = store.get_wiki_page(slug)?.context("页面不存在")?;
    ensure!(
        page.kind != "source" && store.source_history(slug)?.is_empty(),
        "原始资料版本不可覆盖"
    );
    let content: String = store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_revisions WHERE id=?1 AND page_id=?2",
            params![revision, page.id],
            |r| r.get(0),
        )
        .context("历史版本不存在")?;
    ensure!(
        content_hash(&content) != content_hash(&page.content_md),
        "当前正文已经是此版本"
    );
    let sources = store.page_source_snapshots(slug)?;
    super::authoring::validate_revision_basis(
        store,
        &page,
        Some(&super::authoring::revision_basis(store, &page)?),
    )?;
    let draft = WikiPageDraft {
        slug: page.slug.clone(),
        kind: page.kind.clone(),
        title: page.title.clone(),
        summary: content.chars().take(120).collect(),
        content_md: content,
        tags: page.tags.clone(),
        source_event_ids: page.source_event_ids.clone(),
        status: page.status.clone(),
        reason: format!("恢复历史正文 {revision}；来源与适用条件按当前页重新确认"),
        source_url: None,
    };
    let id = store.record_proposal_with_origin(
        &draft,
        &store.knowledge_metadata(slug)?.applicable_when,
        &sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        Some(&page),
        &draft.reason,
        "manual",
    )?;
    tx.commit()?;
    Ok(id)
}
