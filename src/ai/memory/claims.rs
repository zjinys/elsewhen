use super::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose};
use crate::storage::{Store, personal_memory::Claim};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashSet;

fn hash(text: &str) -> String {
    crate::storage::knowledge::content_hash(text)
}

fn payload(text: &str) -> Option<Value> {
    serde_json::from_str(text.get(text.find('{')?..)?).ok()
}

fn identity(claim: &Claim) -> String {
    format!("personal-memory:{}:{}", claim.id, claim.version)
}

pub(crate) fn supplied(store: &Store, context: &[ContextMessage]) -> Vec<Claim> {
    context
        .iter()
        .filter(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
        .filter_map(|message| payload(&message.content))
        .filter_map(|value| store.memory_claim(value["id"].as_str()?).ok().flatten())
        .collect()
}

pub(crate) fn refresh(
    store: &Store,
    conversation: &str,
    context: &mut Vec<ContextMessage>,
) -> Result<()> {
    context.retain(|message| message.segment.purpose != SegmentPurpose::PersonalMemory);
    let stale = store
        .render_context_instruction("memory_stale", &[])
        .unwrap_or_else(|_| {
            "Historical evidence is unavailable or superseded. Recheck current sources.".into()
        });
    let messages = super::history::messages(store, conversation)?;
    let mut suppressed = HashSet::new();
    let mut suppress_turn = false;
    for message in messages.iter() {
        if message.role == "user" {
            suppress_turn = store.memory_message_suppressed(&message.id)?;
        }
        if suppress_turn {
            suppressed.insert(hash(&message.id));
        }
    }
    let mut in_suppressed_turn = false;
    let last_user = context.iter().rposition(|message| message.role == "user");
    for (position, message) in context.iter_mut().enumerate() {
        if message.role == "user" {
            in_suppressed_turn = Some(position) != last_user
                && message
                    .segment
                    .identity_hash
                    .as_ref()
                    .is_some_and(|id| suppressed.contains(id));
        }
        let mut invalid =
            in_suppressed_turn && message.segment.origin == SegmentOrigin::StoredMessage;
        if message.segment.purpose == SegmentPurpose::Summary && !suppressed.is_empty() {
            invalid = true;
        }
        if matches!(message.role.as_str(), "system" | "tool") {
            for citation in crate::knowledge::candidates_in_context(std::slice::from_ref(message)) {
                if !store.memory_page_auto_current(&citation.page_slug)?
                    || (message.segment.identity_hash.as_deref()
                        == Some(hash("user-cognition-profile").as_str())
                        && !store
                            .memory_page_background_current(&citation.page_slug, conversation)?)
                {
                    invalid = true;
                }
            }
        }
        if matches!(message.role.as_str(), "system" | "tool")
            && message.content.contains("\"memory_claims\"")
        {
            if let Some(mut value) = payload(&message.content) {
                if let Some(claims) = value["memory_claims"].as_array_mut() {
                    claims.retain(|claim| {
                        store
                            .memory_claim(claim["id"].as_str().unwrap_or(""))
                            .ok()
                            .flatten()
                            .is_some_and(|current| {
                                current.version == claim["version"].as_i64().unwrap_or(0)
                                    && (!current.excluded
                                        || store.memory_inspection_current(claim).unwrap_or(false))
                                    && store.memory_goal_context_current(claim).unwrap_or(false)
                                    && !matches!(current.state.as_str(), "deleted" | "dismissed")
                                    && store.memory_validity(&current).ok() == Some("current")
                            })
                    });
                    let prefix = message.content.find('{').unwrap_or(0);
                    message.content = format!("{}{}", &message.content[..prefix], value);
                }
            } else {
                invalid = true;
            }
        }
        if invalid {
            message.content = stale.clone();
        }
        message.segment.version = hash(&message.content);
    }
    let Some(conversation_data) = store.get_conversation(conversation)? else {
        return Ok(());
    };
    if conversation_data.assistant_mode != "personal_secretary"
        || conversation_data.wiki_page_slug.is_some()
        || conversation_data.goal_id.is_some()
    {
        return Ok(());
    }
    let query = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.as_str())
        .unwrap_or("");
    if crate::knowledge::query_terms(query).is_empty() {
        return Ok(());
    }
    let compact = store.memory_policy("compact_index")?.as_deref() == Some("true");
    let instruction = store.render_context_instruction("personal_memory", &[])?;
    let mut remaining = if compact { 800 } else { 1440 };
    for mut value in store.search_memories(
        conversation,
        &json!({"query":query,"limit":if compact {8} else {3}}),
    )? {
        if value["scope"] == "reference" {
            continue;
        }
        let excerpt = value["excerpt"].as_str().unwrap_or("");
        let text: String = excerpt.chars().take(remaining.min(480)).collect();
        if text.is_empty() {
            break;
        }
        if text != excerpt {
            value["complete"] = false.into();
        }
        value["excerpt"] = text.into();
        value
            .as_object_mut()
            .map(|object| object.remove("latest_review"));
        value
            .as_object_mut()
            .map(|object| object.remove("goal_links"));
        if let Some(sources) = value["sources"].as_array_mut() {
            for source in sources {
                source.as_object_mut().map(|object| object.remove("quote"));
            }
        }
        if compact {
            let excerpt: String = value["excerpt"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            value = json!({"id":value["id"],"version":value["version"],"page_slug":value["page_slug"],"title":value["title"].as_str().unwrap_or("").chars().take(50).collect::<String>(),"excerpt":excerpt,"scope":value["scope"],"support":value["support"],"goal_context":value["goal_context"],"complete":false});
        }
        let rendered = format!("{instruction}\n{value}");
        if compact {
            if rendered.chars().count() > remaining {
                break;
            }
            remaining -= rendered.chars().count();
        } else {
            remaining -= value["excerpt"].as_str().unwrap_or("").chars().count();
        }
        let Some(claim) = store.memory_claim(value["id"].as_str().unwrap_or(""))? else {
            continue;
        };
        context.push(ContextMessage::new("system", rendered).with_segment(
            SegmentPurpose::PersonalMemory,
            SegmentOrigin::KnowledgeSelection,
            Some(&identity(&claim)),
            OmissionPolicy::OptionalBackground,
        ));
    }
    Ok(())
}

pub(crate) fn current(store: &Store, context: &[ContextMessage]) -> Result<bool> {
    for message in context {
        if !matches!(message.role.as_str(), "system" | "tool") {
            continue;
        }
        let candidates = crate::knowledge::candidates_in_context(std::slice::from_ref(message));
        if crate::knowledge::current_candidates(store, &candidates)?.len() != candidates.len() {
            return Ok(false);
        }
        for citation in candidates {
            if !store.memory_page_auto_current(&citation.page_slug)? {
                return Ok(false);
            }
            if let Some(snapshot) = &store.conversation_snapshot {
                if message.segment.identity_hash.as_deref()
                    == Some(hash("user-cognition-profile").as_str())
                    && !store.memory_page_background_current(
                        &citation.page_slug,
                        &snapshot.conversation_id,
                    )?
                {
                    return Ok(false);
                }
            }
        }
        if message.segment.identity_hash.as_deref() == Some(hash("today-recorded-events").as_str())
        {
            if let Some(snapshot) = &store.conversation_snapshot {
                if super::today_memory_background(store, &snapshot.conversation_id)?
                    .is_none_or(|current| current.content != message.content)
                {
                    return Ok(false);
                }
            }
        }
        if let Some(value) = payload(&message.content) {
            for event in value["events"].as_array().into_iter().flatten() {
                let Some(id) = event["event_id"].as_str() else {
                    return Ok(false);
                };
                if !store.recordable_event(id)?
                    || !store.memory_event_visible(id)?
                    || store.memory_source_suppressed("event", id)?
                {
                    return Ok(false);
                }
            }
        }
        if message.content.contains("\"memory_claims\"") {
            let Some(value) = payload(&message.content) else {
                return Ok(false);
            };
            for value in value["memory_claims"].as_array().into_iter().flatten() {
                let Some(claim) = store.memory_claim(value["id"].as_str().unwrap_or(""))? else {
                    return Ok(false);
                };
                if claim.version != value["version"].as_i64().unwrap_or(0)
                    || (claim.excluded && !store.memory_inspection_current(value)?)
                    || !store.memory_goal_context_current(value)?
                    || matches!(claim.state.as_str(), "deleted" | "dismissed")
                    || store.memory_validity(&claim)? != "current"
                {
                    return Ok(false);
                }
            }
        }
    }
    if let Some(snapshot) = &store.conversation_snapshot {
        if !super::history::context_current(store, &snapshot.conversation_id, context)? {
            return Ok(false);
        }
        let last_user = context.iter().rposition(|message| message.role == "user");
        let mut suppressed = std::collections::HashMap::new();
        let mut in_suppressed_turn = false;
        for message in snapshot.messages.iter() {
            if message.role == "user" {
                in_suppressed_turn = store.memory_message_suppressed(&message.id)?;
            }
            if in_suppressed_turn {
                suppressed.insert(hash(&message.id), hash(&message.content));
            }
        }
        let stale = store
            .render_context_instruction("memory_stale", &[])
            .unwrap_or_else(|_| {
                "Historical evidence is unavailable or superseded. Recheck current sources.".into()
            });
        if !suppressed.is_empty()
            && context.iter().any(|message| {
                message.segment.purpose == SegmentPurpose::Summary && message.content != stale
            })
        {
            return Ok(false);
        }
        if context.iter().enumerate().any(|(position, message)| {
            Some(position) != last_user
                && message.segment.origin == SegmentOrigin::StoredMessage
                && message.segment.identity_hash.as_ref().is_some_and(|id| {
                    suppressed
                        .get(id)
                        .is_some_and(|content| content == &hash(&message.content))
                })
        }) {
            return Ok(false);
        }
    }
    for message in context
        .iter()
        .filter(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
    {
        let Some(value) = payload(&message.content) else {
            return Ok(false);
        };
        let Some(claim) = store.memory_claim(value["id"].as_str().unwrap_or(""))? else {
            return Ok(false);
        };
        if claim.state != "active"
            || claim.excluded
            || !store.memory_goal_context_current(&value)?
            || claim.version != value["version"].as_i64().unwrap_or(0)
            || claim.confirmed_hash.as_deref() != Some(&claim.body_hash)
            || store.memory_validity(&claim)? != "current"
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn budget_receipts(
    store: &Store,
    context: &[ContextMessage],
    request: &str,
    logical_request: &str,
    attempt: u32,
    report: &crate::ai::budget::BudgetReport,
) -> Result<()> {
    if report.outcome != "ready" {
        return Ok(());
    }
    for claim in supplied(store, context) {
        let included = report.segments.iter().any(|segment| {
            segment.segment.identity_hash.as_deref() == Some(hash(&identity(&claim)).as_str())
                && segment.tokens_after > 0
                && segment.omission_reason.is_none()
        });
        if included {
            store.memory_receipt(
                logical_request,
                &format!("{request}:{attempt}"),
                &claim,
                "injected",
                "prepared",
            )?;
        }
    }
    Ok(())
}
