//! Validate logical native exchanges before and after budget selection. Keep
//! identities alongside message policies: Ollama omits IDs on the wire, whereas
//! OpenAI includes them. Normalized text-tool history has no native exchanges.
use super::RequestBudgetError;
use crate::ai::memory::ContextMessage;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Default)]
pub(super) enum MessageExchange {
    #[default]
    Plain,
    Calls(Vec<String>),
    Result(String),
    Invalid,
}

impl MessageExchange {
    pub(super) fn from_context(message: &ContextMessage) -> Self {
        let calls = message
            .tool_calls
            .as_ref()
            .map(|calls| calls.iter().map(|call| call.id.clone()).collect::<Vec<_>>());
        Self::from_parts(&message.role, calls, message.tool_call_id.clone())
    }

    pub(super) fn from_value(message: &Value) -> Self {
        let calls = match message.get("tool_calls").filter(|v| !v.is_null()) {
            None => None,
            Some(Value::Array(calls)) => {
                let Some(ids) = calls
                    .iter()
                    .map(|call| call["id"].as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                else {
                    return Self::Invalid;
                };
                Some(ids)
            }
            Some(_) => return Self::Invalid,
        };
        let result = match message.get("tool_call_id").filter(|v| !v.is_null()) {
            None => None,
            Some(Value::String(id)) => Some(id.clone()),
            Some(_) => return Self::Invalid,
        };
        Self::from_parts(message["role"].as_str().unwrap_or(""), calls, result)
    }

    fn from_parts(role: &str, calls: Option<Vec<String>>, result: Option<String>) -> Self {
        if let Some(calls) = calls.filter(|calls| !calls.is_empty()) {
            if role != "assistant"
                || result.is_some()
                || calls.iter().any(|id| id.trim().is_empty())
            {
                return Self::Invalid;
            }
            return Self::Calls(calls);
        }
        if role == "tool" {
            return match result.filter(|id| !id.trim().is_empty()) {
                Some(id) => Self::Result(id),
                None => Self::Invalid,
            };
        }
        if result.is_some() {
            return Self::Invalid;
        }
        Self::Plain
    }
}

pub(super) fn validate<'a>(messages: impl IntoIterator<Item = &'a MessageExchange>) -> Result<()> {
    let mut seen = HashSet::new();
    let mut pending = HashSet::new();
    for message in messages {
        let reason = match message {
            MessageExchange::Invalid => Some("工具调用或结果缺少有效身份，或角色不匹配"),
            MessageExchange::Calls(calls) => {
                if !pending.is_empty() {
                    Some("上一组工具调用尚未收到全部结果")
                } else if calls.iter().any(|id| !seen.insert(id.as_str())) {
                    Some("工具调用身份重复")
                } else {
                    pending.extend(calls.iter().map(String::as_str));
                    None
                }
            }
            MessageExchange::Result(id) => {
                if pending.remove(id.as_str()) {
                    None
                } else {
                    Some("工具结果重复或找不到对应调用")
                }
            }
            MessageExchange::Plain => {
                if pending.is_empty() {
                    None
                } else {
                    Some("工具调用与结果之间出现了其他消息")
                }
            }
        };
        if let Some(reason) = reason {
            return Err(RequestBudgetError(format!(
                "原生工具上下文校验失败：{reason}；未发送请求，也未裁剪当前问题或必要证据"
            ))
            .into());
        }
    }
    if !pending.is_empty() {
        return Err(RequestBudgetError(
            "原生工具上下文校验失败：工具调用缺少结果；未发送请求，也未裁剪当前问题或必要证据"
                .into(),
        )
        .into());
    }
    Ok(())
}
