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
pub(crate) fn text_protocol_history(context: &mut [ContextMessage]) {
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
            message.content = format!("（已执行工具的历史结果）\n{}", message.content);
        }
        message.tool_call_id = None;
    }
}
/// 本机数据直查：对高频统计/清单类问题直接在本地查库并注入上下文，
/// 不依赖模型的 tool-calling 能力（数据本就存在本地数据库）。
/// 命中返回（查询主题, 已查好的数据文本）。
pub(crate) fn direct_query(store: &Store, user_message: &str) -> Result<Option<(String, String)>> {
    let msg = user_message.trim().to_lowercase();

    // token 用量
    let wants_token = msg.contains("token")
        || msg.contains("用量")
        || (msg.contains("统计") && (msg.contains("token") || msg.contains("用量")));
    if wants_token {
        let daily = store.daily_token_usage(7)?;
        let mut out = String::from("每日 token 用量：\n");
        if daily.is_empty() {
            out.push_str("（最近 7 天没有 AI 调用记录）");
        } else {
            for d in daily {
                out.push_str(&format!(
                    "- {}：{} tokens（{} 次调用）\n",
                    d.date, d.total_tokens, d.call_count
                ));
            }
        }
        return Ok(Some(("token 用量".to_string(), out)));
    }

    // 规则清单
    let wants_rules = msg.contains("规则库")
        || msg.contains("有哪些规则")
        || msg.contains("什么规则")
        || msg.contains("我的规则")
        || msg.contains("规则清单")
        || msg.contains("看看规则");
    if wants_rules {
        let rules = store.list_active_rules()?;
        let mut out = String::from("已生效的个人规则：\n");
        if rules.is_empty() {
            out.push_str("（规则库为空，还没有沉淀过规则）");
        } else {
            for r in rules {
                out.push_str(&format!("- {}\n", r.content));
            }
        }
        return Ok(Some(("个人规则库".to_string(), out)));
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
    .any(|keyword| msg.contains(keyword));
    if wants_goals {
        let hits = store.search_knowledge_base("目标", 8)?;
        let mut out = String::from("知识库中的目标相关页面：\n");
        if hits.is_empty() {
            out.push_str("（暂未找到目标页面）");
        } else {
            for hit in hits {
                out.push_str(&format!("- 《{}》：{}\n", hit.title, hit.snippet));
            }
        }
        return Ok(Some(("当前目标".to_string(), out)));
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
