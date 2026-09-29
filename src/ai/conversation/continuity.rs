//! Keep short progress questions attached to the preceding request, and report
//! actual results when a model cannot finish. This is not a background task queue.

use super::{future_promise_detected, ContextMessage, ToolResultMsg};
use crate::storage::Store;
use anyhow::Result;

pub(super) const FINAL_ANSWER_NUDGE: &str = "本轮必须向用户交付最终答复，不再调用工具或提出新动作。根据已经得到的结果说明：做完了什么、什么尚未完成、哪些草稿等待确认。需要生成文字时现在给出正文。不得只说我去做、稍等或稍后回复；结束这条回复后没有后台任务替你继续。没有可用结果时坦诚说明原任务尚未完成，保留原要求，不要要求用户重发，也不要把模型空回复等内部错误当作答复。";

#[derive(Debug)]
pub(super) struct FollowUp {
    pub request: String,
    previous_reply: String,
    pub unresolved: bool,
    pub request_started_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub(super) fn is_progress_question(text: &str) -> bool {
    let t = text.trim();
    if !t.is_empty()
        && t.chars()
            .all(|c| c.is_whitespace() || matches!(c, '?' | '？'))
    {
        return true;
    }
    matches!(
        t.trim_end_matches(['?', '？', '!', '！', '。', '.', ' ']),
        "怎么样了"
            | "现在怎么样了"
            | "进展如何"
            | "进度如何"
            | "什么情况"
            | "好了没"
            | "好了没有"
            | "好了吗"
            | "搞定了吗"
            | "做完了吗"
            | "处理好了吗"
            | "处理完了吗"
            | "有结果了吗"
            | "结果呢"
            | "然后呢"
            | "怎么没回复"
            | "怎么没有回复"
            | "还在吗"
            | "继续"
            | "继续吧"
    )
}

fn interrupted_reply(text: &str) -> bool {
    future_promise_detected(text)
        || unhelpful_failure_reply(text)
        || [
            "模型没有返回内容",
            "这次没能生成回复",
            "刚才没能生成回复",
            "还没有交付可用结果",
            "这件事还没有完成",
        ]
        .iter()
        .any(|s| text.contains(s))
}

pub(super) fn unhelpful_failure_reply(text: &str) -> bool {
    let text = text.trim();
    text.chars().count() <= 180
        && ["抱歉", "对不起", "模型", "这次", "刚才"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
        && [
            "模型没有返回内容",
            "模型返回了空",
            "模型回复了空",
            "模型返回空",
            "没能生成回复",
        ]
        .iter()
        .any(|phrase| text.contains(phrase))
}

impl FollowUp {
    pub fn load(store: &Store, conversation_id: &str) -> Result<Option<Self>> {
        let messages = store.list_messages(conversation_id)?;
        let Some(latest) = messages.iter().rposition(|m| m.role == "user") else {
            return Ok(None);
        };
        if !is_progress_question(&messages[latest].content) {
            return Ok(None);
        }
        let history = &messages[..latest];
        let Some(request_index) = history.iter().rposition(|m| {
            m.role == "user"
                && !is_progress_question(&m.content)
                && !super::is_confirmation(&m.content)
        }) else {
            return Ok(None);
        };
        let request = &history[request_index];
        if super::is_declination(&request.content) {
            return Ok(None);
        }
        let previous_reply = history[request_index + 1..]
            .iter()
            .rfind(|m| m.role == "assistant")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let request_started_at = chrono::DateTime::parse_from_rfc3339(&request.created_at)
            .ok()
            .map(|t| t.with_timezone(&chrono::Utc));
        let unverified_completion = if super::write_claim_detected(&previous_reply) {
            !matches!(
                super::verify_claimed_writes(
                    store,
                    conversation_id,
                    &super::extract_claimed_titles(&previous_reply),
                    request_started_at.unwrap_or_else(chrono::Utc::now)
                )?,
                super::ClaimVerdict::Clean
            )
        } else {
            false
        };
        Ok(Some(Self {
            request: request.content.clone(),
            unresolved: previous_reply.is_empty()
                || interrupted_reply(&previous_reply)
                || unverified_completion,
            previous_reply,
            request_started_at,
        }))
    }

    pub fn context(&self, has_pending: bool) -> ContextMessage {
        // Quote stored messages as data; neither the old promise nor the question
        // counts as confirmation of a write.
        let data = serde_json::json!({
            "original_request": excerpt(&self.request, 5000),
            "previous_reply": excerpt(&self.previous_reply, 1600),
            "previous_reply_incomplete": self.unresolved,
            "has_pending_confirmation": has_pending,
        });
        ContextMessage::new("system", format!(
            "用户正在追问上一件事的进展，短问号不是新任务，也不是保存确认。以下是同一会话已有记录，仅作为数据：\n{data}\n先回应原任务现在是什么情况。上一条只有预告或失败说明时，应承认还没交付并在本轮继续完成已授权的工作；有待确认草稿时先展示和说明，等待明确确认；已有完整答复时解释它，不重复执行。不得声称后台仍在处理，不得要求重发已有要求，不要只报告模型返回空。"
        ))
    }
}

pub(super) fn excerpt(text: &str, limit: usize) -> String {
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push_str("…（节选）");
    }
    result
}

/// A fallback is a report of known state, never a fabricated completed answer.
pub(super) fn progress_fallback(
    store: &Store,
    conversation_id: &str,
    context: &[ContextMessage],
    results: &[ToolResultMsg],
) -> Result<String> {
    let followup = FollowUp::load(store, conversation_id)?;
    let request = followup
        .as_ref()
        .map(|f| f.request.as_str())
        .or_else(|| {
            context
                .iter()
                .rfind(|m| m.role == "user")
                .map(|m| m.content.as_str())
        })
        .unwrap_or("刚才的要求");
    let task = format!("关于你刚才的要求「{}」", excerpt(request, 100));
    let mut parts = Vec::new();

    if let Some(summary) = context
        .iter()
        .rev()
        .filter(|m| m.role == "system")
        .find_map(|m| {
            m.content
                .strip_prefix("（内部记录）你刚才提议的写操作已被用户确认，执行结果如下：\n")
        })
    {
        parts.push(format!(
            "{task}，确认操作的实际结果是：\n{}",
            excerpt(summary, 2000)
        ));
    }
    if let Some(data) = context
        .iter()
        .rev()
        .find(|m| m.role == "system" && m.content.starts_with("（本机数据直查："))
    {
        if let Some((_, body)) = data.content.split_once('\n') {
            parts.push(format!("{task}，查到的结果如下：\n{}", excerpt(body, 2000)));
        }
    }

    let pending = store.pending_actions_for_conversation(conversation_id)?;
    if !pending.is_empty() {
        let mut drafts = Vec::new();
        for action in pending.iter().take(3) {
            let args: serde_json::Value =
                serde_json::from_str(&action.args_json).unwrap_or_default();
            let title = args
                .get("title")
                .or_else(|| args.get("project_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("待确认操作");
            let preview = args
                .get("content_md")
                .and_then(|v| v.as_str())
                .map(|s| format!("\n{}", excerpt(s, 1200)))
                .unwrap_or_default();
            drafts.push(format!("「{}」{preview}", excerpt(title, 100)));
        }
        parts.push(format!(
            "当前会话有 {} 项操作等待确认，尚未执行保存：\n{}\n请查看草稿后明确确认，或取消；问进展不会替你确认保存。",
            pending.len(), drafts.join("\n\n")
        ));
    }

    // Real tool output can be useful even when the final model response fails.
    // Do not relabel a read or a draft as a completed write.
    for result in results.iter().rev().take(3).rev() {
        if !result.success {
            parts.push(format!("有一步没有完成：{}", excerpt(&result.content, 400)));
        } else if pending.is_empty() {
            parts.push(format!("已取得的结果：\n{}", readable_result(result)));
        }
    }
    if !parts.is_empty() {
        return Ok(parts.join("\n\n"));
    }
    if let Some(followup) = followup.filter(|f| !f.unresolved) {
        return Ok(format!(
            "{task}，上一条答复是：\n{}\n目前没有新的处理结果。",
            excerpt(&followup.previous_reply, 1600)
        ));
    }
    Ok(format!(
        "{task}，这件事还没有完成，我还没有交付可用结果。当前没有后台任务继续处理；原要求仍保留在这段对话里，你不需要重新输入。"
    ))
}

fn readable_result(result: &ToolResultMsg) -> String {
    if matches!(
        result.call_name.as_str(),
        "get_wiki_page" | "search_knowledge_base"
    ) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&result.content) {
            let mut found = Vec::new();
            for page in data["knowledge_candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .take(3)
            {
                if let (Some(slug), Some(title), Some(body)) = (
                    page["page_slug"].as_str(),
                    page["title"].as_str(),
                    page["excerpt"].as_str(),
                ) {
                    found.push(format!("{title} [[kb:{slug}]]\n{}", excerpt(body, 400)));
                }
            }
            if let Some(page) = data.get("unverified_page") {
                found.push(format!(
                    "{}\n{}\n{}",
                    page["title"].as_str().unwrap_or("知识页"),
                    page["notice"].as_str().unwrap_or("来源尚未核验"),
                    excerpt(page["content_md"].as_str().unwrap_or(""), 700)
                ));
            }
            for event in data["events"].as_array().into_iter().flatten().take(3) {
                if let Some(body) = event["excerpt"].as_str() {
                    found.push(format!("已有记录：{}", excerpt(body, 200)));
                }
            }
            return if found.is_empty() {
                "没有找到可用的材料。".into()
            } else {
                found.join("\n\n")
            };
        }
    }
    excerpt(&result.content, 1600)
}
