//! Shared bounded event recall for search tools and conversation background.
use super::*;

pub(crate) fn event_excerpts(
    store: &Store,
    queries: &[String],
    limit: usize,
    exclude_event: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let mut seen = HashSet::new();
    let terms: Vec<_> = queries
        .iter()
        .flat_map(|q| query_terms(q))
        .filter(|term| seen.insert(term.clone()))
        .take(40)
        .collect();
    let wanted = limit.min(8);
    if terms.is_empty() || wanted == 0 {
        return Ok(vec![]);
    }
    let hit = |n: usize| format!("(instr(lower(raw_text),?{n})>0)");
    let excluded = terms.len() + 1;
    let sql = format!(
        "SELECT id,recorded_at,raw_text FROM events WHERE ({}) AND (?{excluded} IS NULL OR id<>?{excluded})
         ORDER BY ({}) DESC,recorded_at DESC,id LIMIT {}",
        (1..=terms.len()).map(hit).collect::<Vec<_>>().join(" OR "),
        (1..=terms.len()).map(hit).collect::<Vec<_>>().join("+"), wanted * 3);
    let mut bindings: Vec<rusqlite::types::Value> = terms.iter().cloned().map(Into::into).collect();
    bindings.push(
        exclude_event
            .map(|id| id.to_owned().into())
            .unwrap_or(rusqlite::types::Value::Null),
    );
    let rows = store
        .connection
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(bindings), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut events = Vec::new();
    let mut texts = HashSet::new();
    for (id, recorded_at, text) in rows {
        if events.len() == wanted {
            break;
        }
        if !store.recordable_event(&id)?
            || !store.memory_event_visible(&id)?
            || store.memory_source_suppressed("event", &id)?
            || !texts.insert(text.split_whitespace().collect::<String>())
        {
            continue;
        }
        let (excerpt, _) = select::matching_excerpt(&text, &terms, 350);
        events.push(serde_json::json!({"event_id":id,"occurred_at":recorded_at,"excerpt":excerpt,"category":"用户记录"}));
    }
    Ok(events)
}
