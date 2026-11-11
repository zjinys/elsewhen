//! 知识绑定与使用记账：上下文挂载、引用校验、usage 记录与最近引用。

use super::*;

pub fn candidates_in_context(context: &[ContextMessage]) -> Vec<KnowledgeCitation> {
    let mut found: Vec<KnowledgeCitation> = Vec::new();
    for message in context
        .iter()
        .filter(|m| m.role == "system" || m.role == "tool")
    {
        // Only complete system-authored candidate objects survive budgeting.
        let payload = message
            .content
            .strip_prefix("知识选材：\n")
            .or_else(|| {
                if message.role == "tool" {
                    Some(message.content.as_str())
                } else {
                    None
                }
            })
            .or_else(|| {
                message
                    .content
                    .strip_prefix("工具「search_knowledge_base」执行结果：")
            })
            .or_else(|| {
                message
                    .content
                    .strip_prefix("工具「get_wiki_page」执行结果：")
            })
            .or_else(|| {
                message
                    .content
                    .strip_prefix("Result of tool \"search_knowledge_base\": ")
            })
            .or_else(|| {
                message
                    .content
                    .strip_prefix("Result of tool \"get_wiki_page\": ")
            })
            .or_else(|| message.content.strip_prefix("（已执行工具的历史结果）\n"))
            .or_else(|| {
                message
                    .content
                    .strip_prefix("[Historical result of an executed tool]\n")
            });
        if let Some(payload) = payload {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                if let Some(raw) = value.get("knowledge_candidates") {
                    if let Ok(items) = serde_json::from_value::<Vec<KnowledgeCitation>>(raw.clone())
                    {
                        for item in items {
                            if let Some(previous) =
                                found.iter_mut().find(|c| c.page_slug == item.page_slug)
                            {
                                // A later explicit read can contain the complete page or a
                                // newer revision than the initial discovery summary.
                                *previous = item;
                            } else {
                                found.push(item);
                            }
                        }
                    }
                }
            }
        }
    }
    found
}

pub fn tool_context(candidates: &[KnowledgeCitation]) -> Result<String> {
    Ok(serde_json::json!({"knowledge_candidates":candidates}).to_string())
}

/// Bound every provider round, including tool results and tool-call arguments.
/// Keep protocol messages paired; reduce old payloads rather than deleting a
/// tool result and leaving its assistant call dangling.
pub fn bound_model_context(context: &mut [ContextMessage], max_chars: usize) -> Result<()> {
    let latest_user = context.iter().rposition(|m| m.role == "user");
    let fixed = context
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let protocol = m
                .tool_calls
                .iter()
                .flatten()
                .map(|call| {
                    call.id.chars().count()
                        + call.name.chars().count()
                        + call.raw_arguments.as_ref().map_or_else(
                            || call.arguments.to_string().chars().count(),
                            |s| s.chars().count(),
                        )
                })
                .sum::<usize>();
            protocol
                + if i == 0 || Some(i) == latest_user {
                    m.content.chars().count()
                } else {
                    0
                }
        })
        .sum::<usize>();
    if fixed > max_chars {
        bail!("本轮输入或工具参数超过上下文上限，请缩短输入后重试");
    }
    let mut room = max_chars - fixed;
    // Recent results take priority over earlier conversation; the latest user
    // request and the primary system rules are never truncated.
    for (i, message) in context.iter_mut().enumerate().rev() {
        if i == 0 || Some(i) == latest_user {
            continue;
        }
        let cost = message.content.chars().count();
        if cost > room {
            if message.content.contains("knowledge_candidates") {
                message.content.clear();
            } else {
                message.content = shorten(&message.content, room);
            }
        }
        room = room.saturating_sub(message.content.chars().count());
    }
    Ok(())
}

/// Returns citations that the model actually emitted AND were supplied by the
/// system. Both supported wiki-link spellings use the same allowlist. Unknown
/// explicit citation markers are removed; ordinary navigation links stay links.
pub fn validate_answer_citations(
    answer: &str,
    candidates: &[KnowledgeCitation],
) -> (String, Vec<KnowledgeCitation>) {
    let mut rest = answer;
    let mut rendered = String::new();
    let mut cited = Vec::new();
    while let Some(start) = rest.find("[[") {
        rendered.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let Some(end) = tail.find("]]") else {
            rendered.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let link = &tail[..end];
        let target = link.strip_prefix("kb:").unwrap_or(link);
        let slug = target.split('|').next().unwrap_or(target);
        if let Some(citation) = candidates.iter().find(|c| c.page_slug == slug) {
            if !cited
                .iter()
                .any(|c: &KnowledgeCitation| c.page_slug == slug)
            {
                cited.push(citation.clone());
            }
            rendered.push_str(&format!("[[{target}]]"));
        } else if !link.starts_with("kb:") {
            rendered.push_str(&rest[start..start + 2 + end + 2]);
        }
        rest = &tail[end + 2..];
    }
    rendered.push_str(rest);
    (rendered, cited)
}

pub fn record_usage(
    store: &Store,
    task: &str,
    owner: &str,
    candidates: &[KnowledgeCitation],
    cited: &[KnowledgeCitation],
) -> Result<()> {
    store.connection.execute(
        "INSERT INTO knowledge_usage(id,task,owner_id,candidates_json,cited_json,created_at)
        VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            task,
            owner,
            serde_json::to_string(candidates)?,
            serde_json::to_string(cited)?,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn latest_citations(store: &Store, task: &str, owner: &str) -> Result<Vec<KnowledgeCitation>> {
    let raw: Option<String> = store
        .connection
        .query_row(
            "SELECT cited_json FROM knowledge_usage WHERE task=?1 AND owner_id=?2
        ORDER BY created_at DESC,rowid DESC LIMIT 1",
            params![task, owner],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|r| serde_json::from_str(&r).map_err(Into::into))
        .unwrap_or(Ok(vec![]))
}
