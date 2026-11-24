//! Request-boundary refresh of optional personal memory and supplied evidence.
use super::*;
use anyhow::Context;
use serde_json::Value;

fn identity(message: &ContextMessage, name: &str) -> bool {
    message.segment.identity_hash.as_deref()
        == Some(crate::storage::knowledge::content_hash(name).as_str())
}

pub(crate) fn related_events(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<ContextMessage>> {
    if store.get_conversation(conversation_id)?.is_some_and(|c| {
        c.assistant_mode == "knowledge_mentor"
            || c.assistant_mode == "goal_mentor"
            || c.wiki_page_slug.is_some()
    }) {
        return Ok(None);
    }
    let messages = history::messages(store, conversation_id)?;
    let query = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let excluded = store.latest_event_id_for_conversation(conversation_id)?;
    let events = crate::knowledge::recall::event_excerpts(store, &[query], 3, excluded.as_deref())?;
    if events.is_empty() {
        return Ok(None);
    }
    let header = store.render_context_instruction("recent_background", &[])?;
    Ok(Some(
        ContextMessage::new(
            "system",
            format!("{header}{}", serde_json::json!({"events":events})),
        )
        .with_segment(
            SegmentPurpose::CrossConversation,
            SegmentOrigin::StoredMessage,
            Some("related-recorded-events"),
            OmissionPolicy::OptionalBackground,
        ),
    ))
}

/// No history rewriting or semantic correction guessing. Refresh the existing
/// profile and automatic recall, then drop whole stale evidence payloads. Native
/// assistant/tool pairs remain intact; stale data is replaced, never reattached.
pub(crate) fn refresh_recalled_context(
    store: &Store,
    conversation_id: &str,
    context: &mut Vec<ContextMessage>,
) -> Result<()> {
    let had_profile = context
        .iter()
        .any(|m| m.segment.purpose == SegmentPurpose::PersonalProfile);
    context.retain(|m| {
        m.segment.purpose != SegmentPurpose::PersonalProfile
            && ![
                "recent-other-conversations",
                "related-recorded-events",
                "today-recorded-events",
                "user-cognition-profile",
            ]
            .iter()
            .any(|name| identity(m, name))
    });
    if had_profile {
        if let Some(profile) = system_context::personal_profile_context(store, conversation_id)? {
            context.push(profile);
        }
    }
    // Recall failure must not block the user's conversation.
    if let Ok(Some(events)) = related_events(store, conversation_id) {
        context.push(events);
    }
    if let Ok(Some(today)) = today_memory_background(store, conversation_id) {
        context.push(today);
    }
    if let Ok(Some(profile)) = user_cognition_profile_background(store, conversation_id) {
        context.push(profile);
    }
    for message in context
        .iter_mut()
        .filter(|m| m.role == "system" || m.role == "tool")
    {
        if !evidence_current(store, message).unwrap_or(false) {
            message.content = store
                .render_context_instruction("memory_stale", &[])
                .unwrap_or_else(|_| {
                    "Earlier evidence is unavailable; recheck current sources.".into()
                });
            message.segment.version = crate::storage::knowledge::content_hash(&message.content);
        }
    }
    Ok(())
}

fn evidence_current(store: &Store, message: &ContextMessage) -> Result<bool> {
    let candidates = crate::knowledge::candidates_in_context(std::slice::from_ref(message));
    if crate::knowledge::current_candidates(store, &candidates)?.len() != candidates.len() {
        return Ok(false);
    }
    // The search tool also contains original event snippets; a later manual
    // recordability decision invalidates these even when page citations survive.
    let payload = message
        .content
        .strip_prefix("知识选材：\n")
        .unwrap_or(&message.content);
    if let Ok(value) = serde_json::from_str::<Value>(payload) {
        if let Some(events) = value.get("events").and_then(Value::as_array) {
            for event in events {
                let id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .context("recall_event_id_missing")?;
                if !store.recordable_event(id)?
                    || !store.memory_event_visible(id)?
                    || store.memory_source_suppressed("event", id)?
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
