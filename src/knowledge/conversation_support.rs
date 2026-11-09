//! Conservative reuse hints and atomic evidence additions for authored updates.
use super::*;
use serde_json::Value;

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn grams(text: &str) -> HashSet<String> {
    let chars: Vec<_> = normalized(text).chars().take(12000).collect();
    chars.windows(3).map(|w| w.iter().collect()).collect()
}

/// Candidate hints are not merge decisions. Keep them bounded and require the
/// author to choose explicitly before creating distinct knowledge.
pub(crate) fn reuse_candidates(store: &Store, title: &str, body: &str) -> Result<Vec<WikiPage>> {
    let query = format!("{} {}", title, shorten(body, 500));
    let terms = query_terms(&query);
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let predicates = terms
        .iter()
        .enumerate()
        .map(|(i, _)| format!("instr(lower(title||' '||content_md),?{})>0", i + 1))
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        "SELECT {} FROM wiki_pages WHERE status<>'archived' AND area NOT IN ('imported','derivative') AND (opinion IS NULL OR opinion<>'reject') AND ({predicates}) ORDER BY updated_at DESC,id LIMIT 80",
        crate::storage::WIKI_PAGE_COLS
    );
    let pages = store
        .connection
        .prepare(&sql)?
        .query_map(
            rusqlite::params_from_iter(&terms),
            crate::storage::map_wiki_page,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let input = grams(body);
    let title = normalized(title);
    let mut matches = Vec::new();
    for page in pages {
        let old = grams(&page.content_md);
        let overlap = input.intersection(&old).count();
        let union = input.union(&old).count().max(1);
        let similar = input.len() >= 12 && old.len() >= 12 && overlap as f64 / union as f64 >= 0.72;
        if normalized(&page.title) == title
            || normalized(&page.content_md) == normalized(body)
            || similar
        {
            matches.push(page);
        }
        if matches.len() == 3 {
            break;
        }
    }
    Ok(matches)
}

pub(crate) fn has_additions(
    store: &Store,
    page: &WikiPage,
    evidence: &[KnowledgeCitation],
    events: &[String],
    bases: &[dependencies::Basis],
) -> Result<bool> {
    let old = store.page_source_snapshots(&page.slug)?;
    if events
        .iter()
        .chain(evidence.iter().flat_map(|e| e.event_ids.iter()))
        .any(|e| !page.source_event_ids.contains(e))
        || evidence
            .iter()
            .flat_map(|e| e.sources.iter())
            .any(|s| !old.iter().any(|o| o.id == s.snapshot_id))
    {
        return Ok(true);
    }
    for base in bases {
        let prior: Option<String> = store
            .connection
            .query_row(
                "SELECT basis FROM knowledge_dependencies WHERE page_id=?1 AND upstream_id=?2",
                params![page.id, base.page_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if prior.as_deref() != Some(base.hash.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn bind_additions(store: &Store, page: &WikiPage, args: &Value) -> Result<()> {
    // Page-task materials may already have added evidence in this transaction.
    // Merge with the current row, not the pre-binding caller snapshot.
    let page = store.get_wiki_page(&page.slug)?.context("修订目标不存在")?;
    let evidence: Vec<KnowledgeCitation> = args
        .get("verified_evidence")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    anyhow::ensure!(
        current_candidates(store, &evidence)?.len() == evidence.len(),
        "新增来源已变化或失效，请重新生成修订"
    );
    anyhow::ensure!(
        evidence.iter().all(|e| e.page_slug != page.slug),
        "页面不能作为自己的新增来源"
    );
    let bases: Vec<dependencies::Basis> = args
        .get("added_dependency_bases")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    dependencies::validate(store, &bases)?;
    let events: Vec<String> = args
        .get("added_event_ids")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    anyhow::ensure!(
        events.len() <= 1
            && events
                .iter()
                .all(|id| store.recordable_event(id).unwrap_or(false)),
        "新增来源事件已失效，请重新审阅"
    );
    let mut snapshots: Vec<String> = store
        .page_source_snapshots(&page.slug)?
        .into_iter()
        .map(|s| s.id)
        .collect();
    snapshots.extend(
        evidence
            .iter()
            .flat_map(|e| e.sources.iter().map(|s| s.snapshot_id.clone())),
    );
    snapshots.sort();
    snapshots.dedup();
    anyhow::ensure!(snapshots.len() <= 8, "修订来源超过8份，请拆分知识主题");
    store.bind_page_sources(&page.id, &snapshots)?;
    let mut all_events = page.source_event_ids.clone();
    all_events.extend(events);
    all_events.extend(evidence.into_iter().flat_map(|e| e.event_ids));
    all_events.sort();
    all_events.dedup();
    store.connection.execute(
        "UPDATE wiki_pages SET source_event_ids=?2,evidence_count=?3 WHERE id=?1",
        params![
            page.id,
            serde_json::to_string(&all_events)?,
            all_events.len() as i64
        ],
    )?;
    dependencies::bind(store, &page.id, &bases)?;
    Ok(())
}
