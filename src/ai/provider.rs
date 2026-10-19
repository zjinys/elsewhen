use super::memory::ContextMessage;
use super::tool::{ToolCall, ToolSpec};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

mod streaming;

/// Streaming transport failures are not evidence that native tools are unsupported.
#[derive(Debug)]
pub(crate) struct ObservedRequestFailure;
impl std::fmt::Display for ObservedRequestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("observable provider request failed")
    }
}
impl std::error::Error for ObservedRequestFailure {}

#[derive(Debug)]
pub(crate) struct NativeToolsUnsupported;
impl std::fmt::Display for NativeToolsUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("provider explicitly rejected native tools")
    }
}
impl std::error::Error for NativeToolsUnsupported {}

/// 服务端错误体透传前截断：完整 body 可能含用户对话内容或敏感报错堆栈，
/// 直接落库/展示会给下游（错误聚合、重试原因）引入不必要的信息暴露（P2）。
const PROVIDER_ERROR_BODY_MAX_CHARS: usize = 300;

/// 按 timeout 分档复用的 blocking HTTP client（P2 client 复用）。
/// provider 每次 `new` 都重建 client 会反复建立连接池；这里按超时档
/// 缓存一份全局实例，调用方共享连接池与 DNS 缓存。
/// 共享 client 池（模块级：代理策略变化时由 clear_shared_http_clients 清空）。
fn shared_http_clients() -> &'static Mutex<Vec<(u64, reqwest::blocking::Client)>> {
    static CLIENTS: OnceLock<Mutex<Vec<(u64, reqwest::blocking::Client)>>> = OnceLock::new();
    CLIENTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 代理策略变化时清空共享 client 池（client 长持有代理设置，不清不生效）。
pub(crate) fn clear_shared_http_clients() {
    shared_http_clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

pub(crate) fn shared_blocking_client(timeout_secs: u64) -> Result<reqwest::blocking::Client> {
    let clients = shared_http_clients();
    let mut clients = clients.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, client)) = clients.iter().find(|(t, _)| *t == timeout_secs) {
        return Ok(client.clone());
    }
    let client = crate::http_proxy::with_outbound_proxy(reqwest::blocking::Client::builder())
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .context("Failed to create HTTP client")?;
    clients.push((timeout_secs, client.clone()));
    Ok(client)
}

/// 截断 provider 错误描述：只保留可诊断的前缀，避免完整服务端 body 泄漏。
pub(crate) fn provider_error_body(status: impl std::fmt::Display, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= PROVIDER_ERROR_BODY_MAX_CHARS {
        format!("{status}: {trimmed}")
    } else {
        let prefix: String = trimmed
            .chars()
            .take(PROVIDER_ERROR_BODY_MAX_CHARS)
            .collect();
        format!("{status}: {prefix}…（已截断）")
    }
}

/// Token usage reported by an AI provider
mod contract;
mod ollama;
mod openai;

pub use contract::{AiProvider, AiReply, ProviderEvent, TokenUsage};
pub use ollama::{OllamaConfig, OllamaProvider};
#[cfg(test)]
pub(crate) use ollama::{
    OllamaOptions, OllamaRequest, OllamaResponse, OllamaResponseToolCall,
    OllamaResponseToolFunction,
};
#[cfg(test)]
pub(crate) use openai::{
    OpenAiChoice, OpenAiMessage, OpenAiMessageResponse, OpenAiRequest, OpenAiResponseToolCall,
    OpenAiResponseToolFunction, OpenAiUsage, openai_message_tool_calls, openai_tool_defs,
    response_tool_calls,
};
pub use openai::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
#[cfg(test)]
pub(crate) use openai::{
    OpenAiMessageToolCall, OpenAiMessageToolFunction, OpenAiPromptTokensDetails, OpenAiToolDef,
    OpenAiToolFunctionDef,
};
pub(crate) use openai::{OpenAiResponse, response_message_tool_calls};

#[cfg(test)]
#[path = "provider_tests/mod.rs"]
mod tests;
