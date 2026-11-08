//! 工具文本协议助手：上下文协议头部、直查、瞬时错误判定（实体平移）。
use super::*;
use crate::storage::Store;
use anyhow::Result;

/// 工具通信协议：原生 tool-calling 或文本信封
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolProtocol {
    Native,
    Text,
}
/// A text fallback must also remove native history fields. Merely omitting the
/// tools definition can still trigger the same gateway conversion/validation.
/// Preserve completed calls/results as text; never dispatch historical calls.
pub(crate) fn text_protocol_history(store: &Store, context: &mut [ContextMessage]) -> Result<()> {
    for message in context {
        if let Some(calls) = message.tool_calls.take() {
            for call in calls {
                let envelope = serde_json::json!({"name": call.name, "arguments": call.arguments});
                message
                    .content
                    .push_str(&format!("\n{TOOL_CALL_MARKER}{envelope}"));
            }
            message.reasoning_content = None;
        }
        if message.role == "tool" {
            message.role = "system".into();
            message.content = store.render_execution_instruction(
                "tool_history",
                &[("result", message.content.clone())],
            )?;
        }
        message.tool_call_id = None;
    }
    Ok(())
}
/// 本机数据直查：对高频统计/清单类问题直接在本地查库并注入上下文，
/// 不依赖模型的 tool-calling 能力（数据本就存在本地数据库）。
/// 命中返回（查询主题, 已查好的数据文本）。
pub(crate) fn direct_query(store: &Store, user_message: &str) -> Result<Option<(String, String)>> {
    let msg = user_message.trim().to_lowercase();

    // token 用量
    let wants_token = msg.contains("token")
        || msg.contains("ai usage")
        || msg.contains("model usage")
        || msg.contains("用量")
        || (msg.contains("统计") && (msg.contains("token") || msg.contains("用量")));
    if wants_token {
        let daily = store.daily_token_usage(7)?;
        let mut out = store.render_lookup_instruction("token_heading", &[])?;
        if daily.is_empty() {
            out.push_str(&store.render_lookup_instruction("token_empty", &[])?);
        } else {
            for d in daily {
                out.push_str(&store.render_lookup_instruction(
                    "token_line",
                    &[
                        ("date", d.date),
                        ("tokens", d.total_tokens.to_string()),
                        ("calls", d.call_count.to_string()),
                    ],
                )?);
            }
        }
        return Ok(Some((
            store.render_lookup_instruction("token_label", &[])?,
            out,
        )));
    }

    // 规则清单
    let wants_rules = msg.contains("规则库")
        || msg.contains("有哪些规则")
        || msg.contains("什么规则")
        || msg.contains("我的规则")
        || msg.contains("规则清单")
        || msg.contains("看看规则")
        || [
            "my rules",
            "list rules",
            "active rules",
            "rules library",
            "rule list",
        ]
        .iter()
        .any(|phrase| msg.contains(phrase));
    if wants_rules {
        let rules = store.list_active_rules()?;
        let mut out = store.render_lookup_instruction("rules_heading", &[])?;
        if rules.is_empty() {
            out.push_str(&store.render_lookup_instruction("rules_empty", &[])?);
        } else {
            for r in rules {
                out.push_str(&format!("- {}\n", r.content));
            }
        }
        return Ok(Some((
            store.render_lookup_instruction("rules_label", &[])?,
            out,
        )));
    }

    // 人生目标/主要任务是本地知识库中的稳定事实，不能依赖模型自行决定
    // 是否调用搜索工具，更不能受当前对话记忆窗口限制。
    let wants_goals = [
        "人生目标",
        "主要目标",
        "当前目标",
        "主要任务",
        "长期目标",
        "我想做什么",
        "搞钱",
    ]
    .iter()
    .any(|keyword| msg.contains(keyword))
        || [
            "my goals",
            "current goals",
            "long-term goals",
            "main goals",
            "life goals",
            "main tasks",
        ]
        .iter()
        .any(|phrase| msg.contains(phrase));
    if wants_goals {
        let mut hits = store.search_knowledge_base("目标", 8)?;
        for hit in store.search_knowledge_base("goal", 8)? {
            if !hits.iter().any(|old| {
                old.kind == hit.kind && old.title == hit.title && old.snippet == hit.snippet
            }) && hits.len() < 8
            {
                hits.push(hit);
            }
        }
        let mut out = store.render_lookup_instruction("goals_heading", &[])?;
        if hits.is_empty() {
            out.push_str(&store.render_lookup_instruction("goals_empty", &[])?);
        } else {
            for hit in hits {
                out.push_str(&store.render_lookup_instruction(
                    "goals_line",
                    &[("title", hit.title), ("excerpt", hit.snippet)],
                )?);
            }
        }
        return Ok(Some((
            store.render_lookup_instruction("goals_label", &[])?,
            out,
        )));
    }

    Ok(None)
}
pub(crate) fn is_transient_provider_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    [
        " 429",
        " 500",
        " 502",
        " 503",
        " 504",
        "failed dependency",
        "bad gateway",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}
