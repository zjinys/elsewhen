//! write_tools.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::{ToolContext, ToolPolicy};
use crate::storage::Store;
use anyhow::Result;
use serde_json::{Value, json};

// ── 写类工具（事件直接入库；知识页走 WriteConfirm：草拟 → 用户确认 → 执行） ───

/// 记录一条个人事件（直接写入 events 真源，无需确认）
pub(super) struct RecordEventTool;
impl Tool for RecordEventTool {
    fn name(&self) -> &'static str {
        "record_event"
    }
    fn description(&self) -> &'static str {
        "把一段值得长期回看的客观经历、决定、行动或进展记录为个人事件，立即保存到事件记录（无需确认）。只记录用户自己的事实；不要记录对 AI 回复的评价、对话过程、寒暄、纯提问或闲聊。用户明确说‘记一下/帮我记/存进事件’时，按用户意图记录。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "raw_text":{"type":"string","description":"值得回看的第一人称事实、决定、行动或进展；不要填入对 AI 的评价或闲聊，必填"},
                "occurred_at":{"type":"string","description":"发生时间（ISO 8601，如 2026-09-15T10:30:00Z），可选，默认现在"}
            },
            "required":["raw_text"],
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteDirect
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        if let Some(event_id) = &ctx.source_event_id {
            return Ok(format!(
                "当前对话输入已自动保存为事件（{event_id}）并进入分析队列，无需重复记录"
            ));
        }
        insert_event_from_args(args, ctx.store)
    }
}

/// 解析事件参数并写入 events 真源。
/// record_event 直接执行时用；历史遗留的 record_event 待确认动作也复用同一逻辑。
pub(super) fn insert_event_from_args(args: &Value, store: &Store) -> Result<String> {
    let raw_text = arg_str(args, "raw_text")?;
    let occurred_at = match arg_str_opt(args, "occurred_at") {
        Some(iso) => chrono::DateTime::parse_from_rfc3339(&iso)
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        None => chrono::Utc::now(),
    };
    let id = store.insert_event(crate::event::NewEvent {
        raw_text: &raw_text,
        occurred_at,
        recorded_at: chrono::Utc::now(),
        source: "tool",
    })?;
    Ok(format!("已保存事件（{id}）：{raw_text}"))
}

/// 保存一篇知识库草稿（草拟，确认后写入 wiki_pages）
pub(super) struct SaveKnowledgeDraftTool;
impl Tool for SaveKnowledgeDraftTool {
    fn name(&self) -> &'static str {
        "save_knowledge_draft"
    }
    fn description(&self) -> &'static str {
        "把对话中的知识草拟成页面，用户确认后保存。保存有引用的回答时，source_slugs 填实际采用且已提供的知识页面，applicable_when 写适用条件；系统固定来源版本，确认时再校验。无依据笔记可以保存但不能伪造来源。同名页不覆盖，改用 save_wiki_revision。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"页面标题，必填"},
                "content_md":{"type":"string","description":"正文（Markdown），必填"},
                "kind":{"type":"string","description":"页面类型，可选，默认 topic。已知类型：topic/source/insight/principle/method/case/relationship/decision/habit/project"},
                "tags":{"type":"array","items":{"type":"string"},"description":"标签数组，可选"},
                "source_slugs":{"type":"array","items":{"type":"string"},"description":"实际采用的已检索页面标识，最多 8 项；不填无关来源"},
                "applicable_when":{"type":"string","description":"适用条件，保存有依据的方法/案例/规律时必填，最多 600 字"}
            },
            "required":["title","content_md"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str(args, "title")?;
        let content_md = arg_str(args, "content_md")?;
        if let Some(existing) = ctx.store.find_wiki_page_by_title(&title)? {
            return Ok(format!(
                "知识库已有《{}》（slug={}），本次没有新建或覆盖。请先结合现有页面内容，再用 save_wiki_revision 草拟补充；仍需用户确认后才保存。",
                existing.title, existing.slug
            ));
        }
        if ctx
            .store
            .pending_action_for_title(ctx.conversation_id, "save_knowledge_draft", &title)?
            .is_some()
        {
            return Ok(format!(
                "《{}》已经有一份待确认草稿（尚未保存），本次不重复创建。在审阅卡点击接受即可保存这一份。",
                title
            ));
        }
        let kind = arg_str_opt(args, "kind").unwrap_or_else(|| "topic".to_string());
        let evidence = crate::knowledge::authoring::draft_sources(
            ctx.store,
            ctx.conversation_id,
            args,
            ctx.knowledge_candidates,
        )?;
        let applicable = arg_str_opt(args, "applicable_when").unwrap_or_default();
        anyhow::ensure!(applicable.chars().count() <= 600, "适用条件最多 600 字");
        anyhow::ensure!(
            evidence.is_empty()
                || !matches!(kind.as_str(), "method" | "case" | "principle")
                || !applicable.trim().is_empty(),
            "有依据的方法、案例或规律需要说明适用条件"
        );
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let action_args = json!({
            "title": title,
            "content_md": content_md,
            "kind": kind,
            "tags": tags,
            "verified_evidence": evidence,
            "applicable_when": applicable,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "save_knowledge_draft",
            &action_args,
        )?;
        Ok(format!(
            "已为你草拟一篇知识库页面（待确认，尚未保存）：\n标题：{title}\n---\n{content}\n---\n—— 等你确认后才会真正保存，在审阅卡点击接受即可。",
            content = {
                let mut c = content_md.clone();
                if c.chars().count() > 120 {
                    c = c.chars().take(120).collect::<String>() + "…";
                }
                c
            }
        ))
    }
}

pub(crate) fn store_create_pending(
    store: &Store,
    conversation_id: &str,
    action: &str,
    args: &Value,
) -> Result<String> {
    let args_json = serde_json::to_string(args)?;
    // Analysis may finish after cancellation. Fence the final proposal insert
    // inside the same write transaction, including tools that do network work.
    let transaction = store.begin_write_transaction()?;
    crate::ai::generation::check(store)?;
    let id = store.create_pending_action(conversation_id, action, &args_json)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    Ok(id)
}
