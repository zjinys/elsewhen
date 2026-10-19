//! OpenAI 兼容 provider：配置、wire 结构、工具转换与生成实现。

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub context_window: Option<u32>,
}

impl Default for OpenAiCompatibleConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-3.5-turbo".to_string(),
            temperature: 0.7,
            max_tokens: None,
            context_window: None,
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
pub(crate) struct OpenAiMessage {
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
pub(crate) struct OpenAiMessageToolCall {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) tool_type: &'static str,
    pub(crate) function: OpenAiMessageToolFunction,
}

#[derive(Serialize)]
pub(crate) struct OpenAiMessageToolFunction {
    pub(crate) name: String,
    /// OpenAI 要求 arguments 是 JSON 字符串
    pub(crate) arguments: String,
}

/// OpenAI 兼容 tools 定义（请求时传入）
#[derive(Serialize)]
pub(crate) struct OpenAiToolDef {
    #[serde(rename = "type")]
    pub(crate) tool_type: &'static str,
    pub(crate) function: OpenAiToolFunctionDef,
}

#[derive(Serialize)]
pub(crate) struct OpenAiToolFunctionDef {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Value,
}

#[derive(Serialize)]
pub(crate) struct OpenAiRequest {
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
pub(crate) struct OpenAiResponse {
    pub(crate) choices: Vec<OpenAiChoice>,
    pub(crate) model: Option<String>,
    pub(crate) usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiUsage {
    pub(crate) prompt_tokens: Option<u64>,
    pub(crate) completion_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
    /// deepseek 网关直接报 prompt_cache_hit_tokens。
    pub(crate) prompt_cache_hit_tokens: Option<u64>,
    /// OpenAI 报 prompt_tokens_details.cached_tokens。
    pub(crate) prompt_tokens_details: Option<OpenAiPromptTokensDetails>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiPromptTokensDetails {
    pub(crate) cached_tokens: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiChoice {
    pub(crate) message: OpenAiMessageResponse,
    /// 上游结束原因；缺失时仍允许解析兼容响应。
    #[serde(default)]
    pub(crate) finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiMessageResponse {
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) tool_calls: Option<Vec<OpenAiResponseToolCall>>,
    #[serde(default)]
    pub(crate) function_call: Option<OpenAiResponseToolFunction>,
    #[serde(default)]
    pub(crate) reasoning_content: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct OpenAiResponseToolCall {
    pub(crate) id: String,
    pub(crate) function: OpenAiResponseToolFunction,
}

#[derive(Deserialize, Clone)]
pub(crate) struct OpenAiResponseToolFunction {
    pub(crate) name: String,
    /// OpenAI 返回的 arguments 是 JSON 字符串
    pub(crate) arguments: String,
}

pub(crate) fn openai_tool_defs(tools: &[ToolSpec]) -> Vec<OpenAiToolDef> {
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

pub(crate) fn openai_message_tool_calls(calls: &[ToolCall]) -> Vec<OpenAiMessageToolCall> {
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

pub(crate) fn response_tool_calls(raw: Vec<OpenAiResponseToolCall>) -> Vec<ToolCall> {
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

pub(crate) fn response_message_tool_calls(message: &OpenAiMessageResponse) -> Vec<ToolCall> {
    if let Some(calls) = &message.tool_calls {
        if !calls.is_empty() {
            return response_tool_calls(calls.clone());
        }
    }
    // Some compatible endpoints answer modern tools requests with the deprecated
    // single function_call field. Normalize it once, with a stable ID for the
    // assistant/result pair; never dispatch both forms if both are present.
    message
        .function_call
        .as_ref()
        .map(|function| {
            response_tool_calls(vec![OpenAiResponseToolCall {
                id: format!("call_{}", uuid::Uuid::new_v4().simple()),
                function: function.clone(),
            }])
        })
        .unwrap_or_default()
}

impl AiProvider for OpenAiCompatibleProvider {
    fn accepts_supplement(
        &self,
        messages: &[ContextMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<bool> {
        crate::ai::budget::validate_context_exchanges(messages)?;
        let (mut payload, _) = self.request_parts(messages.to_vec(), tools)?;
        payload["stream"] = Value::Bool(true);
        payload["stream_options"] = serde_json::json!({"include_usage":true});
        let window = self
            .config
            .context_window
            .map(|v| v as usize)
            .unwrap_or_else(|| crate::ai::budget::default_window(&self.config.model));
        let limit = crate::ai::budget::input_limit(
            window,
            self.config.max_tokens.unwrap_or(4096) as usize,
        )?;
        // Do not trim any existing exchange to fit a supplement. Extra reserve
        // covers clock refresh and protocol framing between probe and request.
        Ok(
            crate::ai::budget::input_tokens(&self.config.model, &payload).saturating_add(512)
                <= limit,
        )
    }

    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.generate_impl(messages, tools, None)
    }

    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        self.generate_impl(messages, tools, Some(observer))
            .map_err(|error| {
                if error.downcast_ref::<NativeToolsUnsupported>().is_some() {
                    error
                } else {
                    error.context(ObservedRequestFailure)
                }
            })
    }
}

impl OpenAiCompatibleProvider {
    fn request_parts(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<(Value, crate::ai::budget::RequestPolicies)> {
        let messages = crate::ai::time_context::enrich_history(
            messages,
            &crate::ai::time_context::SystemClock,
        );
        let policies = crate::ai::budget::RequestPolicies::from_context(&messages);
        let request = OpenAiRequest {
            model: self.config.model.clone(),
            messages: messages
                .into_iter()
                .map(|m| OpenAiMessage {
                    role: m.role,
                    content: m.content,
                    tool_calls: m.tool_calls.map(|calls| openai_message_tool_calls(&calls)),
                    tool_call_id: m.tool_call_id,
                    reasoning_content: m.reasoning_content,
                })
                .collect(),
            temperature: self.config.temperature,
            max_tokens: Some(self.config.max_tokens.unwrap_or(4096)),
            tools: tools
                .filter(|tools| !tools.is_empty())
                .map(openai_tool_defs),
            tool_choice: tools
                .filter(|tools| !tools.is_empty())
                .map(|_| "auto".to_string()),
        };
        Ok((serde_json::to_value(request)?, policies))
    }

    fn generate_impl(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: Option<&mut dyn FnMut(ProviderEvent<'_>) -> Result<()>>,
    ) -> Result<AiReply> {
        let (mut payload, mut policies) = self.request_parts(messages, tools)?;

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let output = self.config.max_tokens.unwrap_or(4096) as usize;
        let mut window = self
            .config
            .context_window
            .map(|v| v as usize)
            .unwrap_or_else(|| crate::ai::budget::default_window(&self.config.model));
        if let Some(observer) = observer {
            return streaming::generate(
                &self.config,
                &url,
                payload,
                policies,
                window,
                output,
                observer,
            );
        }
        let mut retry = false;
        let response = loop {
            policies.fit(&self.config.model, &mut payload, window, output)?;
            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .json(&payload)
                .send()
                .context("Failed to send request to AI provider")?;
            if response.status().is_success() {
                break response;
            }
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let code = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["error"]["code"].as_str().map(str::to_owned));
            if !retry
                && status.as_u16() == 400
                && code.as_deref() == Some("context_length_exceeded")
            {
                let used = crate::ai::budget::input_tokens(&self.config.model, &payload);
                window = (used + output).min(window) / 2;
                retry = true;
                continue;
            }
            anyhow::bail!(
                "AI provider returned error {}",
                provider_error_body(status, &body)
            );
        };

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
        let tool_calls = response_message_tool_calls(message);

        let usage = ai_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u
                .total_tokens
                .unwrap_or_else(|| u.prompt_tokens.unwrap_or(0) + u.completion_tokens.unwrap_or(0)),
            prompt_cached_tokens: u
                .prompt_cache_hit_tokens
                .or_else(|| u.prompt_tokens_details.and_then(|d| d.cached_tokens)),
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
