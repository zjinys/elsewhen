use super::memory::ContextMessage;
use super::tool::{ToolCall, ToolSpec};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

/// 服务端错误体透传前截断：完整 body 可能含用户对话内容或敏感报错堆栈，
/// 直接落库/展示会给下游（错误聚合、重试原因）引入不必要的信息暴露（P2）。
const PROVIDER_ERROR_BODY_MAX_CHARS: usize = 300;

/// 按 timeout 分档复用的 blocking HTTP client（P2 client 复用）。
/// provider 每次 `new` 都重建 client 会反复建立连接池；这里按超时档
/// 缓存一份全局实例，调用方共享连接池与 DNS 缓存。
pub(crate) fn shared_blocking_client(timeout_secs: u64) -> Result<reqwest::blocking::Client> {
    static CLIENTS: OnceLock<Mutex<Vec<(u64, reqwest::blocking::Client)>>> = OnceLock::new();
    let clients = CLIENTS.get_or_init(|| Mutex::new(Vec::new()));
    let mut clients = clients.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, client)) = clients.iter().find(|(t, _)| *t == timeout_secs) {
        return Ok(client.clone());
    }
    let client = reqwest::blocking::Client::builder()
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
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// AI provider reply: content plus optional tool calls, usage and model metadata
#[derive(Debug, Clone)]
pub struct AiReply {
    /// 最终文本；若本轮回调了工具，则可能为空
    pub content: String,
    /// 原生 tool-calling：模型要求调用的工具列表（空 = 未调用）
    pub tool_calls: Vec<ToolCall>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
    /// OpenAI 兼容接口的 `choices[0].finish_reason`（`stop` / `length` /
    /// `function_call` / `tool_calls`）。
    ///
    /// 存在的理由不是「顺手记一下」，而是一个可复现的坏响应签名：部分中转
    /// 在模型决定调工具时，会返回 `finish_reason: "function_call"` 却把整个
    /// `tool_calls` 数组丢掉，message 里只剩 `{"role":"assistant"}`。这种响应
    /// 看起来和「模型返回空」完全一样（content 空、tool_calls 空），但根因在
    /// 上游而不是模型——实测 2026-09-29 的 hub.oaifree.com 在「要调工具」的
    /// 请求上 8/8 全部如此，且 `usage.completion_tokens` 照常有值（说明 token
    /// 生成了、是被中转丢的）。没有这个字段就无法把它和真的空回复区分开。
    pub finish_reason: Option<String>,
}

impl AiReply {
    /// 构造一个纯文本回复
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: Vec::new(),
            model: None,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }
}

/// AI provider trait for generating replies
pub trait AiProvider {
    /// 生成回复；`tools` 非空时走原生 tool-calling（服务端支持才生效）
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply>;

    /// 纯文本请求（不带 tools）
    fn generate_reply(&self, messages: Vec<ContextMessage>) -> Result<AiReply> {
        self.generate_reply_with_tools(messages, None)
    }
}

/// OpenAI-compatible provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

impl Default for OpenAiCompatibleConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-3.5-turbo".to_string(),
            temperature: 0.7,
            max_tokens: None,
        }
    }
}

/// OpenAI-compatible provider (OpenAI, DeepSeek, vLLM, etc.)
pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self> {
        let client = shared_blocking_client(60)?;

        Ok(Self { config, client })
    }
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiMessageToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

/// assistant 回传的原生 tool_calls（OpenAI 要求原样回传）
#[derive(Serialize)]
struct OpenAiMessageToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: OpenAiMessageToolFunction,
}

#[derive(Serialize)]
struct OpenAiMessageToolFunction {
    name: String,
    /// OpenAI 要求 arguments 是 JSON 字符串
    arguments: String,
}

/// OpenAI 兼容 tools 定义（请求时传入）
#[derive(Serialize)]
struct OpenAiToolDef {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: OpenAiToolFunctionDef,
}

#[derive(Serialize)]
struct OpenAiToolFunctionDef {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: Option<String>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageResponse,
    /// 见 [AiReply::finish_reason]：区分「模型返回空」与「中转吞了 tool_calls」
    /// 的唯一可用信号。缺省字段不能省——不接的话 serde 会整体解析失败。
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiMessageResponse {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiResponseToolCall>>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize, Clone)]
struct OpenAiResponseToolCall {
    id: String,
    function: OpenAiResponseToolFunction,
}

#[derive(Deserialize, Clone)]
struct OpenAiResponseToolFunction {
    name: String,
    /// OpenAI 返回的 arguments 是 JSON 字符串
    arguments: String,
}

fn openai_tool_defs(tools: &[ToolSpec]) -> Vec<OpenAiToolDef> {
    tools
        .iter()
        .map(|t| OpenAiToolDef {
            tool_type: "function",
            function: OpenAiToolFunctionDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            },
        })
        .collect()
}

fn openai_message_tool_calls(calls: &[ToolCall]) -> Vec<OpenAiMessageToolCall> {
    calls
        .iter()
        .map(|c| OpenAiMessageToolCall {
            id: c.id.clone(),
            tool_type: "function",
            function: OpenAiMessageToolFunction {
                name: c.name.clone(),
                // 回传时用原始字面字符串（OpenAI 要求逐字节一致）；无原始串时重新序列化
                arguments: c
                    .raw_arguments
                    .clone()
                    .unwrap_or_else(|| serde_json::to_string(&c.arguments).unwrap_or_default()),
            },
        })
        .collect()
}

fn response_tool_calls(raw: Vec<OpenAiResponseToolCall>) -> Vec<ToolCall> {
    raw.into_iter()
        .map(|c| {
            let raw_args = c.function.arguments.clone();
            ToolCall {
                id: c.id,
                name: c.function.name,
                // 保留原始 arguments 字符串：OpenAI 回传时必须逐字节一致
                raw_arguments: Some(raw_args.clone()),
                arguments: serde_json::from_str(&raw_args)
                    .unwrap_or_else(|_| Value::String(raw_args)),
            }
        })
        .collect()
}

impl AiProvider for OpenAiCompatibleProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let openai_messages: Vec<OpenAiMessage> = messages
            .into_iter()
            .map(|m| OpenAiMessage {
                role: m.role,
                content: m.content,
                tool_calls: m.tool_calls.map(|calls| openai_message_tool_calls(&calls)),
                tool_call_id: m.tool_call_id,
                reasoning_content: m.reasoning_content,
            })
            .collect();

        let mut request = OpenAiRequest {
            model: self.config.model.clone(),
            messages: openai_messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            tools: None,
            tool_choice: None,
        };
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(openai_tool_defs(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .context("Failed to send request to AI provider")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "AI provider returned error {}",
                provider_error_body(status, &body)
            );
        }

        let ai_response: OpenAiResponse = response
            .json()
            .context("Failed to parse AI provider response")?;

        let message = ai_response
            .choices
            .first()
            .map(|c| &c.message)
            .context("AI provider returned no choices")?;
        let finish_reason = ai_response
            .choices
            .first()
            .and_then(|c| c.finish_reason.clone());

        let content = message.content.clone().unwrap_or_default();
        let tool_calls = message
            .tool_calls
            .clone()
            .map(response_tool_calls)
            .unwrap_or_default();

        let usage = ai_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u
                .total_tokens
                .unwrap_or_else(|| u.prompt_tokens.unwrap_or(0) + u.completion_tokens.unwrap_or(0)),
        });

        Ok(AiReply {
            content,
            tool_calls,
            model: ai_response.model,
            usage,
            reasoning_content: message.reasoning_content.clone(),
            finish_reason,
        })
    }
}

/// Ollama provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "llama2".to_string(),
            temperature: 0.7,
        }
    }
}

/// Ollama local provider
pub struct OllamaProvider {
    config: OllamaConfig,
    client: reqwest::blocking::Client,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = shared_blocking_client(120)?;

        Ok(Self { config, client })
    }
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaMessageToolCall>>,
}

#[derive(Serialize)]
struct OllamaMessageToolCall {
    function: OllamaMessageToolFunction,
}

#[derive(Serialize)]
struct OllamaMessageToolFunction {
    name: String,
    /// Ollama 的 arguments 是对象
    arguments: Value,
}

/// Ollama tools 定义
#[derive(Serialize)]
struct OllamaToolDef {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: OllamaToolFunctionDef,
}

#[derive(Serialize)]
struct OllamaToolFunctionDef {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: OllamaOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaToolDef>>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessageResponse,
    model: Option<String>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaMessageResponse {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaResponseToolCall>>,
}

#[derive(Deserialize)]
struct OllamaResponseToolCall {
    function: OllamaResponseToolFunction,
}

#[derive(Deserialize)]
struct OllamaResponseToolFunction {
    name: String,
    arguments: Value,
}

fn ollama_tool_defs(tools: &[ToolSpec]) -> Vec<OllamaToolDef> {
    tools
        .iter()
        .map(|t| OllamaToolDef {
            tool_type: "function",
            function: OllamaToolFunctionDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            },
        })
        .collect()
}

fn ollama_message_tool_calls(calls: &[ToolCall]) -> Vec<OllamaMessageToolCall> {
    calls
        .iter()
        .map(|c| OllamaMessageToolCall {
            function: OllamaMessageToolFunction {
                name: c.name.clone(),
                arguments: c.arguments.clone(),
            },
        })
        .collect()
}

fn ollama_response_tool_calls(raw: Vec<OllamaResponseToolCall>) -> Vec<ToolCall> {
    raw.into_iter()
        .map(|c| ToolCall::new(c.function.name, c.function.arguments))
        .collect()
}

impl AiProvider for OllamaProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|m| OllamaMessage {
                role: m.role.clone(),
                content: m.content,
                tool_calls: m.tool_calls.map(|calls| ollama_message_tool_calls(&calls)),
            })
            .collect();

        let mut request = OllamaRequest {
            model: self.config.model.clone(),
            messages: ollama_messages,
            stream: false,
            options: OllamaOptions {
                temperature: self.config.temperature,
            },
            tools: None,
        };
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(ollama_tool_defs(tools));
            }
        }

        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "Ollama returned error {}",
                provider_error_body(status, &body)
            );
        }

        let ollama_response: OllamaResponse =
            response.json().context("Failed to parse Ollama response")?;

        let usage = match (
            ollama_response.prompt_eval_count,
            ollama_response.eval_count,
        ) {
            (None, None) => None,
            (prompt, completion) => {
                let prompt_tokens = prompt.unwrap_or(0);
                let completion_tokens = completion.unwrap_or(0);
                Some(TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens: prompt_tokens + completion_tokens,
                })
            }
        };

        let tool_calls = ollama_response
            .message
            .tool_calls
            .map(ollama_response_tool_calls)
            .unwrap_or_default();

        Ok(AiReply {
            content: ollama_response.message.content,
            tool_calls,
            model: ollama_response.model,
            usage,
            reasoning_content: None,
            // Ollama 用的是 `done_reason` 而非 `finish_reason`，语义也不同
            // （stop / length / tool_calls）。这里不猜，交给上层按「未知」处理。
            finish_reason: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compatible_config_has_defaults() {
        let config = OpenAiCompatibleConfig::default();
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-3.5-turbo");
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn ollama_config_has_defaults() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "llama2");
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn openai_provider_can_be_created() {
        let config = OpenAiCompatibleConfig::default();
        let provider = OpenAiCompatibleProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn ollama_provider_can_be_created() {
        let config = OllamaConfig::default();
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn openai_response_parses_usage_and_model() {
        let json = r#"{"choices":[{"message":{"content":"hi"}}],"model":"gpt-4o","usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
        let u = resp.usage.unwrap();
        assert_eq!(u.prompt_tokens, Some(12));
        assert_eq!(u.completion_tokens, Some(5));
        assert_eq!(u.total_tokens, Some(17));
    }

    #[test]
    fn openai_response_without_usage_is_none() {
        // 有些 provider 不返回 usage，应容忍缺失
        let json = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
        assert!(resp.model.is_none());
    }

    /// 实测坏响应（2026-09-29，hub.oaifree.com + gpt-4o，8/8 复现）：
    /// 声称调用了函数，却把 tool_calls 整个丢掉，message 里只剩 role。
    /// `completion_tokens` 照常有值——token 生成了，是被中转丢的。
    ///
    /// 这条测试的作用是钉住「finish_reason 必须被接住」：没有它，这种响应
    /// 和真的「模型返回空」在下游完全无法区分，排查只能靠人工重放请求。
    #[test]
    fn openai_response_keeps_finish_reason_when_tool_calls_are_dropped() {
        let json = r#"{
            "choices":[{
                "message":{"role":"assistant"},
                "finish_reason":"function_call",
                "logprobs":null
            }],
            "object":"chat.completion",
            "model":"gpt-4o",
            "usage":{"prompt_tokens":5759,"completion_tokens":58,"total_tokens":5817}
        }"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        let choice = &resp.choices[0];

        // 坏响应的三个特征，缺一不可
        assert_eq!(choice.finish_reason.as_deref(), Some("function_call"));
        assert!(choice.message.content.is_none());
        assert!(choice.message.tool_calls.is_none());

        // token 生成了却被丢弃——这是「根因在上游而非模型」的唯一硬证据
        assert_eq!(resp.usage.unwrap().completion_tokens, Some(58));
    }

    #[test]
    fn openai_response_tolerates_missing_finish_reason() {
        // 老 provider 不给这个字段，不能因此解析失败
        let json = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.choices[0].finish_reason.is_none());
    }

    #[test]
    fn openai_response_keeps_normal_finish_reason() {
        // 正常路径不能被这次改动影响
        let json = r#"{"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}]}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("hi"));
    }

    #[test]
    fn openai_response_parses_tool_calls() {
        let json = r#"{
            "choices":[{"message":{
                "content":null,
                "tool_calls":[
                    {"id":"call_1","type":"function","function":{"name":"list_rules","arguments":"{}"}},
                    {"id":"call_2","type":"function","function":{"name":"get_wiki_page","arguments":"{\"slug\":\"kb-test\"}"}}
                ]
            }}],
            "model":"gpt-4o",
            "usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}
        }"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        let calls = msg.tool_calls.clone().map(response_tool_calls).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "list_rules");
        assert_eq!(calls[1].name, "get_wiki_page");
        assert_eq!(calls[1].arguments["slug"], "kb-test");
        // raw arguments 必须原样保留（OpenAI 回传要求逐字节一致，含原始空格）
        assert_eq!(
            calls[1].raw_arguments.as_deref(),
            Some("{\"slug\":\"kb-test\"}")
        );
        // 生成 id 的 ToolCall 可被原生协议回传
        let echoed = openai_message_tool_calls(&calls);
        assert_eq!(echoed.len(), 2);
        assert_eq!(echoed[1].function.arguments, "{\"slug\":\"kb-test\"}");
    }

    #[test]
    fn ollama_response_parses_eval_counts() {
        let json = r#"{"message":{"content":"hi"},"model":"llama3","prompt_eval_count":20,"eval_count":8}"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model.as_deref(), Some("llama3"));
        assert_eq!(resp.prompt_eval_count, Some(20));
        assert_eq!(resp.eval_count, Some(8));
    }

    #[test]
    fn provider_error_body_truncates_long_bodies() {
        // 短 body 原样保留（含关键诊断信息）
        let short = provider_error_body(429, "rate limit exceeded");
        assert_eq!(short, "429: rate limit exceeded");
        // 长 body 截断到上限并标记，避免完整服务端报错堆栈落库/回显
        let long_body = "x".repeat(1000);
        let truncated = provider_error_body(500, &long_body);
        assert!(truncated.chars().count() < 400);
        assert!(truncated.ends_with("…（已截断）"));
        assert!(!truncated.contains(&long_body));
        // 空白 body 不做无意义截断
        assert_eq!(provider_error_body(500, "   "), "500: ");
    }
}
