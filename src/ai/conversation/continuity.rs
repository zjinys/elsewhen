//! Keep short progress questions attached to the preceding request, and report
//! actual results when a model cannot finish. This is not a background task queue.

use super::{ContextMessage, ToolResultMsg, future_promise_detected};
use crate::storage::Store;
use anyhow::Result;

pub(crate) const FINAL_ANSWER_NUDGE: &str = "本轮必须向用户交付最终答复，不再调用工具或提出新动作。根据已经得到的结果说明：做完了什么、什么尚未完成、哪些草稿等待确认。需要生成文字时现在给出正文。不得只说我去做、稍等或稍后回复；结束这条回复后没有后台任务替你继续。没有可用结果时坦诚说明原任务尚未完成，保留原要求，不要要求用户重发，也不要把模型空回复等内部错误当作答复。";

#[derive(Debug)]
pub(crate) struct FollowUp {
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
            // 2026-10-18 事故 P1-A：诚实失败消息是失败说明，不是交付；
            // 它之后追问进展必须判定为「还没完成」并衔接原任务。
            "这一轮还没有完成",
        ]
        .iter()
        .any(|s| text.contains(s))
}

pub(crate) fn unhelpful_failure_reply(text: &str) -> bool {
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
        ContextMessage::new(
            "system",
            format!(
                "用户正在追问上一件事的进展，短问号不是新任务，也不是保存确认。以下是同一会话已有记录，仅作为数据：\n{data}\n先回应原任务现在是什么情况。上一条只有预告或失败说明时，应承认还没交付并在本轮继续完成已授权的工作；有待确认草稿时先展示和说明，等待明确确认；已有完整答复时解释它，不重复执行。不得声称后台仍在处理，不得要求重发已有要求，不要只报告模型返回空。"
            ),
        )
    }
}

pub(super) fn excerpt(text: &str, limit: usize) -> String {
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push_str("…（节选）");
    }
    result
}

/// 事故 2026-10-18 P1-A：兜底只允许两种形态。
/// ① 用户显式追问进展（FollowUp 命中）：可以把已知结果作为材料汇报；
/// ② 自动触发（模型没给出答复/运行失败）：只产出诚实失败消息，
///    工具检索结果一律不进用户消息流（留在 run 记录中，待 reports 侧栏承接），
///    否则就像事故里那样，知识库 dump 被当成回答，失败真相被淹没。
pub(crate) fn progress_fallback(
    store: &Store,
    conversation_id: &str,
    context: &[ContextMessage],
    results: &[ToolResultMsg],
    failure: Option<&str>,
) -> Result<String> {
    let followup = FollowUp::load(store, conversation_id)?;
    if followup.is_none() {
        let reason = failure.unwrap_or("生成没有产出可用答复；你的问题还挂着，可以重试一次。");
        return failure_reply(store, conversation_id, context, reason);
    }
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

    if let Some(summary) = confirmed_write_summary(context) {
        parts.push(format!(
            "{task}，确认操作的实际结果是：\n{}",
            excerpt(&summary, 2000)
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
    if let Some(notice) = pending_notice(&pending) {
        parts.push(notice);
    }

    // Real tool output can be useful even when the final model response fails.
    // Only an explicit progress question may see it (see doc above); an automatic
    // trigger never gets here.
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

fn pending_notice(pending: &[crate::storage::PendingAction]) -> Option<String> {
    if pending.is_empty() {
        return None;
    }
    let mut drafts = Vec::new();
    for action in pending.iter().take(3) {
        let args: serde_json::Value = serde_json::from_str(&action.args_json).unwrap_or_default();
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
    Some(format!(
        "当前会话有 {} 项操作等待确认，尚未执行保存：\n{}\n请查看草稿后明确确认，或取消；确认后才会执行保存。",
        pending.len(),
        drafts.join("\n\n")
    ))
}

/// 最近的未交付请求：若最新一条用户输入本身是新请求就取它（点名的是「现在
///这个问题」）；若是追问/确认，则回到造成它的那个原请求，并检查其后的助手
///消息是否只留下了预告（promised）。与 FollowUp::load 同一扫描口径。
fn pending_request(store: &Store, conversation_id: &str) -> Result<Option<(String, bool)>> {
    let messages = store.list_messages(conversation_id)?;
    let Some(latest) = messages.iter().rposition(|m| m.role == "user") else {
        return Ok(None);
    };
    let latest_text = messages[latest].content.clone();
    if super::is_declination(&latest_text) {
        return Ok(None);
    }
    if is_progress_question(&latest_text) || super::is_confirmation(&latest_text) {
        // 追问/确认不是新任务：回到造成它的那个原请求。
        let Some(request_index) = messages[..latest].iter().rposition(|m| {
            m.role == "user"
                && !is_progress_question(&m.content)
                && !super::is_confirmation(&m.content)
        }) else {
            return Ok(None);
        };
        if super::is_declination(&messages[request_index].content) {
            return Ok(None);
        }
        let promised = messages[request_index + 1..latest]
            .iter()
            .any(|m| m.role == "assistant" && future_promise_detected(&m.content));
        return Ok(Some((messages[request_index].content.clone(), promised)));
    }
    Ok(Some((latest_text, false)))
}

/// 诚实的失败消息（2026-10-18 事故 P1-A/P1-B）。点名原请求、给出失败原因，
/// 若此前有承诺则承认没有做到——绝不声称完成，也绝不把工具结果当成回答。
pub(super) fn failure_reply(
    store: &Store,
    conversation_id: &str,
    context: &[ContextMessage],
    reason: &str,
) -> Result<String> {
    let mut parts = Vec::new();
    // 用户已确认且已执行的写操作：如实汇报执行结果——这是承诺的兑现，
    // 不算顶包；只剩没生成的补充说明，不能反过来说「还没完成」。
    if let Some(summary) = confirmed_write_summary(context) {
        let request = pending_request(store, conversation_id)?
            .map(|(r, _)| r)
            .or_else(|| {
                context
                    .iter()
                    .rev()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.clone())
            })
            .unwrap_or_else(|| "刚才的要求".into());
        parts.push(format!(
            "关于你刚才的要求「{}」，确认操作的实际结果是：\n{}",
            excerpt(&request, 100),
            excerpt(&summary, 2000)
        ));
        if let Some(notice) =
            pending_notice(&store.pending_actions_for_conversation(conversation_id)?)
        {
            parts.push(notice);
        }
        return Ok(parts.join("\n\n"));
    }
    let (mut request, mut promised) = match pending_request(store, conversation_id)? {
        Some((request, promised)) => (Some(request), promised),
        None => (None, false),
    };
    // 库里还没有本轮输入时（测试/边缘路径），退回 context 的最新用户消息。
    let context_request = context
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone());
    if request.is_none() {
        request = context_request;
    }
    // 正在进行的这轮内部也可能已出现过预告；失败说明必须承认它没兑现。
    if !promised {
        promised = context
            .iter()
            .any(|m| m.role == "assistant" && future_promise_detected(&m.content));
    }
    match request {
        Some(request) => {
            let task = format!("关于你刚才的要求「{}」", excerpt(&request, 120));
            let promised_line = if promised {
                "另外，我之前说过要接着处理这件事，这次同样没有做到；它还挂着，不会被当作已完成。"
            } else {
                ""
            };
            parts.push(format!("{task}，这一轮还没有完成：{reason}{promised_line}"));
        }
        None => parts.push(format!("这一轮还没有完成：{reason}")),
    }
    if let Some(notice) = pending_notice(&store.pending_actions_for_conversation(conversation_id)?)
    {
        parts.push(notice);
    }
    parts.push(
        "现在还没有交付可用结果；原要求仍保留在这段对话里，说「继续」或换个问法即可。".into(),
    );
    Ok(parts.join("\n\n"))
}

/// 上下文里最近一条「已确认写操作的执行结果」内部记录（若存在）。
fn confirmed_write_summary(context: &[ContextMessage]) -> Option<String> {
    context
        .iter()
        .rev()
        .filter(|m| m.role == "system")
        .find_map(|m| {
            m.content
                .strip_prefix("（内部记录）你刚才提议的写操作已被用户确认，执行结果如下：\n")
                .map(str::to_string)
        })
}

/// run 级失败（budget/生成错误）落终态时写入对话流的诚实失败消息；
/// 由 generation 侧在 finish_chat_run 之后追加，挂在本轮输入上，
/// 让「请求没被回答」在消息流里可见（事故 2026-10-18 的沉默与顶包都源于此）。
pub(crate) fn failure_notice(store: &Store, conversation_id: &str, reason: &str) -> Result<String> {
    failure_reply(store, conversation_id, &[], reason)
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
