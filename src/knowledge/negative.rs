//! Historical rejection context, kept outside the positive citation allowlist.
use super::*;

pub(crate) fn select_warnings(
    store: &Store,
    queries: &[String],
    budget: usize,
) -> Result<Vec<serde_json::Value>> {
    let mut seen_terms = HashSet::new();
    let terms = queries
        .iter()
        .take(4)
        .flat_map(|q| select::query_terms(q))
        .filter(|term| seen_terms.insert(term.clone()))
        .take(64)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let text = "lower(p.title||' '||p.tags||' '||COALESCE(s.reject_reason,'')||' '||p.content_md)";
    let predicates = (1..=terms.len())
        .map(|i| format!("instr({text},?{i})>0"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let score = (1..=terms.len()).map(|i| format!("4*(instr(lower(p.title||' '||COALESCE(s.reject_reason,'')),?{i})>0)+(instr({text},?{i})>0)")).collect::<Vec<_>>().join("+");
    // Reject is an explicit source decision. Archived-only and incubating pages
    // are not negative knowledge. Never infer a missing historical reason.
    let sql = format!(
        "SELECT s.id,p.slug,p.title,s.reject_reason,p.updated_at
        FROM knowledge_sources s JOIN knowledge_source_pages m ON m.source_id=s.id
        JOIN wiki_pages p ON p.id=m.page_id
        WHERE s.opinion='reject' AND p.opinion='reject'
          AND length(trim(COALESCE(s.reject_reason,'')))>0 AND ({predicates})
        ORDER BY ({score}) DESC,p.updated_at DESC,p.slug LIMIT 80"
    );
    let rows = store
        .connection
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(terms.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let mut warnings = vec![];
    for (id, slug, title, reason, updated) in rows {
        if !seen.insert(id) {
            continue;
        }
        let item = serde_json::json!({
            "page_slug":slug,"title":select::shorten(&title,100),
            "rejection_reason":select::shorten(&reason,600),
            "reason_truncated":reason.chars().count()>600,
            "page_updated_at":updated,
            "usage":"historical_rejection_only"
        });
        let mut next = warnings.clone();
        next.push(item);
        if serde_json::to_string(&next)?.chars().count() <= budget {
            warnings = next;
        }
        if warnings.len() == 3 {
            break;
        }
    }
    Ok(warnings)
}
