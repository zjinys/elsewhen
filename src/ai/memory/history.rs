use super::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose};
use crate::storage::{MessageSummary, Store};
use anyhow::Result;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashSet;

const MAX_TURNS: usize = 3;
const MAX_CANDIDATES: usize = 32;
const MAX_MESSAGES_PER_TURN: usize = 6;
const MAX_EXCERPT_CHARS: usize = 480;
const MAX_PAYLOAD_CHARS: usize = 12_000;

#[derive(Serialize)]
pub(crate) struct HistoryMessage {
    source_message_id: String,
    source_version: String,
    role: String,
    recorded_at: String,
    excerpt: String,
    excerpt_offset: Option<usize>,
    complete: bool,
}

#[derive(Serialize)]
pub(crate) struct HistoryTurn {
    source_input_id: String,
    messages: Vec<HistoryMessage>,
    omitted_messages: usize,
}

#[derive(Serialize)]
pub(crate) struct HistoryRecall {
    conversation_id: String,
    conversation_title: Option<String>,
    scope: &'static str,
    coverage: &'static str,
    turns: Vec<HistoryTurn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission: Option<serde_json::Value>,
}

fn terms(queries: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    queries
        .iter()
        .flat_map(|query| crate::knowledge::query_terms(query))
        .filter(|term| {
            ![
                "the", "and", "what", "that", "this", "which", "about", "have", "was", "were",
                "when", "why", "how", "can", "you", "your", "are", "for", "please", "last", "time",
                "previous", "earlier", "hello", "hi", "thanks", "thank", "上次", "之前", "那个",
                "这个", "记得", "以前", "当时", "说过", "我们", "你好", "谢谢", "继续", "接着",
                "刚才",
            ]
            .contains(&term.as_str())
        })
        .filter(|term| seen.insert(term.clone()))
        .take(32)
        .collect()
}

fn score(text: &str, query: &[String]) -> usize {
    let lower = text.to_lowercase();
    query
        .iter()
        .filter(|term| {
            if term.is_ascii() {
                lower
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word == term.as_str())
            } else {
                lower.contains(term.as_str())
            }
        })
        .count()
}

pub(crate) fn messages<'a>(
    store: &'a Store,
    conversation_id: &str,
) -> Result<Cow<'a, [MessageSummary]>> {
    if let Some(snapshot) = &store.conversation_snapshot {
        if snapshot.conversation_id == conversation_id {
            return Ok(Cow::Borrowed(&snapshot.messages));
        }
    }
    Ok(Cow::Owned(store.list_messages(conversation_id)?))
}

pub(crate) fn eligible(store: &Store, input_id: &str) -> Result<bool> {
    if store.memory_message_suppressed(input_id)? {
        return Ok(false);
    }
    source_eligible(store, input_id)
}

pub(crate) fn source_eligible(store: &Store, input_id: &str) -> Result<bool> {
    let events = store
        .connection
        .prepare(
            "SELECT event_id FROM input_records WHERE message_id=?1
         UNION ALL SELECT event_id FROM chat_input_effects WHERE input_message_id=?1",
        )?
        .query_map([input_id], |row| row.get::<_, Option<String>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for event in events {
        let Some(event) = event else {
            return Ok(false);
        };
        if !store.recordable_event(&event)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn current_message(store: &Store, message: &MessageSummary) -> Result<bool> {
    Ok(store
        .connection
        .query_row(
            "SELECT content,role FROM messages WHERE id=?1 AND conversation_id=?2",
            rusqlite::params![message.id, message.conversation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .is_some_and(|(content, role)| content == message.content && role == message.role))
}

pub(crate) fn search(
    store: &Store,
    conversation_id: &str,
    queries: &[String],
    visible: &[ContextMessage],
) -> Result<Option<HistoryRecall>> {
    search_scoped(store, conversation_id, queries, visible, None)
}

pub(crate) fn search_shared(
    store: &Store,
    target: &str,
    queries: &[String],
) -> Result<Vec<HistoryRecall>> {
    let mut results = Vec::new();
    for source in store.memory_shared_sources(target)? {
        if let Some(permission) = store.memory_grant(&source, target)? {
            if let Some(recall) = search_scoped(store, &source, queries, &[], Some(permission))? {
                results.push(recall);
            }
        }
    }
    Ok(results)
}

fn search_scoped(
    store: &Store,
    conversation_id: &str,
    queries: &[String],
    visible: &[ContextMessage],
    permission: Option<serde_json::Value>,
) -> Result<Option<HistoryRecall>> {
    let Some(conversation) = store.get_conversation(conversation_id)? else {
        return Ok(None);
    };
    if conversation.archived
        || !crate::storage::personal_memory::grants::conversation_current(store, conversation_id)?
    {
        return Ok(None);
    }
    let query = terms(queries);
    if query.is_empty() {
        return Ok(None);
    }
    let mut messages = messages(store, conversation_id)?;
    if let Some(permission) = &permission {
        let bound = permission["message_bound"].as_i64().unwrap_or(0);
        let ids: HashSet<String> = store
            .connection
            .prepare("SELECT id FROM messages WHERE conversation_id=?1 AND rowid<=?2")?
            .query_map(rusqlite::params![conversation_id, bound], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        messages
            .to_mut()
            .retain(|message| ids.contains(&message.id));
    }
    let Some(current) = messages.iter().rposition(|message| message.role == "user") else {
        return Ok(None);
    };
    let mut starts: Vec<usize> = messages[..=current]
        .iter()
        .enumerate()
        .filter_map(|(position, message)| (message.role == "user").then_some(position))
        .collect();
    if permission.is_some() {
        starts.push(messages.len());
    }
    let fully_visible: HashSet<_> = visible
        .iter()
        .filter(|message| message.segment.origin == SegmentOrigin::StoredMessage)
        .map(|message| {
            (
                message.segment.identity_hash.clone(),
                message.content.as_str(),
            )
        })
        .collect();
    let mut candidates = Vec::new();
    for bounds in starts.windows(2) {
        let (start, end) = (bounds[0], bounds[1]);
        if messages[start].kind != "chat" {
            continue;
        }
        let relevance = messages[start..end]
            .iter()
            .filter(|message| message.kind == "chat")
            .map(|message| {
                score(&message.content, &query) * if message.role == "user" { 2 } else { 1 }
            })
            .max()
            .unwrap_or(0);
        if relevance == 0 {
            continue;
        }
        let all_visible = messages[start..end]
            .iter()
            .filter(|message| message.kind == "chat")
            .all(|message| {
                fully_visible.contains(&(
                    Some(crate::storage::knowledge::content_hash(&message.id)),
                    message.content.as_str(),
                ))
            });
        candidates.push((start, end, relevance, all_visible));
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse((candidate.2, candidate.0)));
    let mut shortlist: Vec<_> = candidates.iter().take(MAX_CANDIDATES).copied().collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    for candidate in candidates.into_iter().take(MAX_CANDIDATES) {
        if !shortlist.iter().any(|existing| existing.0 == candidate.0) {
            shortlist.push(candidate);
        }
    }
    let mut qualified = Vec::new();
    for candidate in shortlist {
        if eligible(store, &messages[candidate.0].id)?
            && current_message(store, &messages[candidate.0])?
        {
            qualified.push(candidate);
        }
    }
    qualified.sort_by_key(|candidate| candidate.0);
    let candidates = qualified;
    let Some(best) = candidates
        .iter()
        .filter(|candidate| !candidate.3)
        .max_by_key(|candidate| (candidate.2, candidate.0))
        .copied()
    else {
        return Ok(None);
    };
    let mut selected = vec![best];
    for candidate in candidates.iter().rev() {
        if selected.len() == MAX_TURNS {
            break;
        }
        if candidate.0 != best.0 {
            selected.push(*candidate);
        }
    }
    let mut positions = std::collections::BTreeSet::new();
    for (start, end, _, _) in selected {
        positions.insert((start, end));
        if let Some(next) = starts.iter().position(|position| *position == end) {
            if let Some(after) = starts.get(next + 1) {
                positions.insert((end, *after));
            }
        }
    }
    let mut turns = Vec::new();
    let mut used = 0;
    for (start, end) in positions.into_iter().rev() {
        let input = &messages[start];
        if input.kind != "chat" || !eligible(store, &input.id)? {
            continue;
        }
        let original: Vec<_> = messages[start..end]
            .iter()
            .filter(|message| message.kind == "chat")
            .collect();
        let mut current = true;
        for message in &original {
            if !current_message(store, message)? {
                current = false;
                break;
            }
        }
        if !current {
            continue;
        }
        let mut excerpts = Vec::new();
        for message in original.iter().take(MAX_MESSAGES_PER_TURN) {
            let (excerpt, offset) = crate::knowledge::select::matching_excerpt(
                &message.content,
                &query,
                MAX_EXCERPT_CHARS,
            );
            excerpts.push(HistoryMessage {
                source_message_id: message.id.clone(),
                source_version: crate::storage::knowledge::content_hash(&message.content),
                role: message.role.clone(),
                recorded_at: message.created_at.clone(),
                complete: offset.is_none(),
                excerpt,
                excerpt_offset: offset,
            });
        }
        let turn = HistoryTurn {
            source_input_id: input.id.clone(),
            omitted_messages: original.len().saturating_sub(excerpts.len()),
            messages: excerpts,
        };
        let size = serde_json::to_string(&turn)?.chars().count();
        if used + size > MAX_PAYLOAD_CHARS {
            continue;
        }
        used += size;
        turns.push(turn);
    }
    if turns.is_empty() {
        return Ok(None);
    }
    turns.reverse();
    Ok(Some(HistoryRecall {
        conversation_id: conversation.id,
        conversation_title: conversation
            .title
            .map(|title| title.chars().take(80).collect()),
        scope: if permission.is_some() {
            "approved_conversation_snapshot"
        } else {
            "current_conversation_only"
        },
        coverage: "partial_keyword_retrieval",
        turns,
        permission,
    }))
}

pub(crate) fn refresh(store: &Store, conversation_id: &str, context: &mut Vec<ContextMessage>) {
    context.retain(|message| message.segment.purpose != SegmentPurpose::RecalledHistory);
    refresh_tool_results(store, conversation_id, context);
    let Ok(Some(conversation)) = store.get_conversation(conversation_id) else {
        return;
    };
    if conversation.assistant_mode != "personal_secretary"
        || conversation.wiki_page_slug.is_some()
        || conversation.goal_id.is_some()
        || conversation.archived
    {
        return;
    }
    let Ok(messages) = messages(store, conversation_id) else {
        return;
    };
    let Some(current) = messages.iter().rposition(|message| message.role == "user") else {
        return;
    };
    let mut queries = vec![messages[current].content.clone()];
    let input = messages[current].content.to_lowercase();
    if [
        "上次",
        "之前",
        "那个",
        "这个",
        "刚才",
        "以前",
        "earlier",
        "previous",
        "that ",
        "last time",
    ]
    .iter()
    .any(|reference| input.contains(reference))
    {
        queries.extend(
            messages[..current]
                .iter()
                .rev()
                .filter(|message| message.kind == "chat")
                .take(3)
                .map(|message| message.content.chars().take(600).collect()),
        );
    }
    let recall = search(store, conversation_id, &queries, context)
        .ok()
        .flatten();
    let mut recalls: Vec<_> = recall.into_iter().collect();
    recalls.extend(search_shared(store, conversation_id, &queries).unwrap_or_default());
    let Ok(instructions) = store.render_context_instruction("history_recall", &[]) else {
        return;
    };
    for recall in recalls {
        let Ok(body) = serde_json::to_string(&recall) else {
            continue;
        };
        context.push(
            ContextMessage::new("system", format!("{instructions}\n{body}")).with_segment(
                SegmentPurpose::RecalledHistory,
                SegmentOrigin::StoredMessage,
                Some("current-conversation-history"),
                OmissionPolicy::OptionalBackground,
            ),
        );
    }
}

fn valid_recall(store: &Store, conversation_id: &str, value: &serde_json::Value) -> Result<bool> {
    if value.is_null() {
        return Ok(true);
    }
    let Some(source_id) = value
        .get("conversation_id")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(false);
    };
    if source_id != conversation_id {
        let grant = store.memory_grant(source_id, conversation_id)?;
        if grant.is_none() || grant.as_ref() != value.get("permission") {
            return Ok(false);
        }
    }
    if !crate::storage::personal_memory::grants::conversation_current(store, source_id)?
        || !crate::storage::personal_memory::grants::conversation_current(store, conversation_id)?
    {
        return Ok(false);
    }
    let Some(turns) = value.get("turns").and_then(serde_json::Value::as_array) else {
        return Ok(false);
    };
    for turn in turns {
        let Some(input) = turn
            .get("source_input_id")
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(false);
        };
        if !eligible(store, input)? {
            return Ok(false);
        }
        let Some(messages) = turn.get("messages").and_then(serde_json::Value::as_array) else {
            return Ok(false);
        };
        for message in messages {
            let Some(id) = message
                .get("source_message_id")
                .and_then(serde_json::Value::as_str)
            else {
                return Ok(false);
            };
            let source = store
                .connection
                .query_row(
                    "SELECT content,role,rowid FROM messages WHERE id=?1 AND conversation_id=?2",
                    rusqlite::params![id, source_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((content, role, position)) = source else {
                return Ok(false);
            };
            if source_id != conversation_id
                && value["permission"]["message_bound"]
                    .as_i64()
                    .is_none_or(|bound| position > bound)
            {
                return Ok(false);
            }
            if message
                .get("source_version")
                .and_then(serde_json::Value::as_str)
                != Some(crate::storage::knowledge::content_hash(&content).as_str())
                || message.get("role").and_then(serde_json::Value::as_str) != Some(role.as_str())
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn context_current(
    store: &Store,
    conversation: &str,
    context: &[ContextMessage],
) -> Result<bool> {
    for message in context
        .iter()
        .filter(|message| message.segment.purpose == SegmentPurpose::RecalledHistory)
    {
        let Some(start) = message.content.find('{') else {
            return Ok(false);
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content[start..]) else {
            return Ok(false);
        };
        if !valid_recall(store, conversation, &value)? {
            return Ok(false);
        }
    }
    let mut refreshed = context.to_vec();
    refresh_tool_results(store, conversation, &mut refreshed);
    Ok(refreshed
        .iter()
        .zip(context)
        .all(|(after, before)| after.content == before.content))
}

fn refresh_tool_results(store: &Store, conversation_id: &str, context: &mut [ContextMessage]) {
    for message in context
        .iter_mut()
        .filter(|message| message.role == "tool" || message.role == "system")
    {
        if !message.content.contains("\"conversation_history\"") {
            continue;
        }
        let payload = message
            .content
            .strip_prefix("工具「search_knowledge_base」执行结果：")
            .or_else(|| {
                message
                    .content
                    .strip_prefix("Result of tool \"search_knowledge_base\": ")
            })
            .or_else(|| message.content.strip_prefix("（已执行工具的历史结果）\n"))
            .or_else(|| {
                message
                    .content
                    .strip_prefix("[Historical result of an executed tool]\n")
            })
            .unwrap_or(&message.content);
        let prefix_len = message.content.len() - payload.len();
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(payload) else {
            message.content = serde_json::json!({"conversation_history":null,"history_status":"unreadable_history_payload"}).to_string();
            message.segment.version = crate::storage::knowledge::content_hash(&message.content);
            continue;
        };
        let mut changed = false;
        if let Some(shared) = value
            .get_mut("shared_history")
            .and_then(serde_json::Value::as_array_mut)
        {
            let previous_len = shared.len();
            shared.retain(|history| valid_recall(store, conversation_id, history).unwrap_or(false));
            changed |= shared.len() != previous_len;
        }
        if let Some(history) = value.get("conversation_history") {
            if !valid_recall(store, conversation_id, history).unwrap_or(false) {
                value["conversation_history"] = serde_json::Value::Null;
                value["history_status"] = "source_changed_or_unavailable".into();
                changed = true;
            }
        }
        if let Some(events) = value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
        {
            let previous_len = events.len();
            events.retain(|event| {
                event
                    .get("event_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| {
                        store.recordable_event(id).unwrap_or(false)
                            && store.memory_event_visible(id).unwrap_or(false)
                            && !store.memory_source_suppressed("event", id).unwrap_or(true)
                    })
            });
            changed |= events.len() != previous_len;
        }
        if changed {
            message.content = format!("{}{}", &message.content[..prefix_len], value);
            message.segment.version = crate::storage::knowledge::content_hash(&message.content);
        }
    }
}

#[cfg(test)]
mod tests;
