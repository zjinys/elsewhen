//! Ollama 本地 provider：配置、wire 结构与生成实现。

use super::*;

/// Ollama provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            context_window: None,
            max_tokens: None,
            model: "llama2".to_string(),
            temperature: 0.7,
        }
    }
}

/// Ollama local provider
pub struct OllamaProvider {
    config: OllamaConfig,
    client: reqwest::blocking::Client,
    /// 运行时输出档位覆盖：None = 生成档；聊天路径构造后切低预留档（同 OpenAI 侧）。
    output_reserve_override: Option<usize>,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = shared_blocking_client(120)?;

        Ok(Self {
            config,
            client,
            output_reserve_override: None,
        })
    }

    /// 覆盖输出预留档位（聊天路径用 `budget::chat_output`）。
    pub fn with_output_reserve(mut self, output: usize) -> Self {
        self.output_reserve_override = Some(output);
        self
    }

    /// 运行时输出预留：provider 配置不再提供输出上限，按模型族系统默认。
    fn output_reserve(&self) -> usize {
        self.output_reserve_override
            .unwrap_or_else(|| crate::ai::budget::default_output(&self.config.model))
    }
}

#[derive(Serialize)]
pub(crate) struct OllamaMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<OllamaMessageToolCall>>,
}

#[derive(Serialize)]
pub(crate) struct OllamaMessageToolCall {
    pub(crate) function: OllamaMessageToolFunction,
}

#[derive(Serialize)]
pub(crate) struct OllamaMessageToolFunction {
    pub(crate) name: String,
    /// Ollama 的 arguments 是对象
    pub(crate) arguments: Value,
}

/// Ollama tools 定义
#[derive(Serialize)]
pub(crate) struct OllamaToolDef {
    #[serde(rename = "type")]
    pub(crate) tool_type: &'static str,
    pub(crate) function: OllamaToolFunctionDef,
}

#[derive(Serialize)]
pub(crate) struct OllamaToolFunctionDef {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Value,
}

#[derive(Serialize)]
pub(crate) struct OllamaRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<OllamaMessage>,
    pub(crate) stream: bool,
    pub(crate) options: OllamaOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<OllamaToolDef>>,
}

#[derive(Serialize)]
pub(crate) struct OllamaOptions {
    pub(crate) temperature: f32,
    pub(crate) num_ctx: usize,
    pub(crate) num_predict: usize,
}

#[derive(Deserialize)]
pub(crate) struct OllamaResponse {
    pub(crate) message: OllamaMessageResponse,
    pub(crate) model: Option<String>,
    pub(crate) prompt_eval_count: Option<u64>,
    pub(crate) eval_count: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct OllamaMessageResponse {
    #[serde(default)]
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) tool_calls: Option<Vec<OllamaResponseToolCall>>,
}

#[derive(Deserialize)]
pub(crate) struct OllamaResponseToolCall {
    pub(crate) function: OllamaResponseToolFunction,
}

#[derive(Deserialize)]
pub(crate) struct OllamaResponseToolFunction {
    pub(crate) name: String,
    pub(crate) arguments: Value,
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
    fn visit_request_budgets(
        &self,
        visitor: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        visitor(&|messages| self.fits_request(messages))
    }

    fn fits_request(&self, messages: &[ContextMessage]) -> Result<bool> {
        crate::ai::budget::validate_context_exchanges(messages)?;
        let (payload, _) = self.request_parts(messages.to_vec(), None)?;
        let limit = crate::ai::budget::input_limit(
            self.config.context_window.unwrap_or(32768) as usize,
            self.output_reserve(),
        )?;
        Ok(
            crate::ai::budget::input_tokens(&self.config.model, &payload).saturating_add(512)
                <= limit,
        )
    }

    fn input_headroom(
        &self,
        messages: &[ContextMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<Option<(usize, String)>> {
        crate::ai::budget::validate_context_exchanges(messages)?;
        let (payload, _) = self.request_parts(messages.to_vec(), tools)?;
        let limit = crate::ai::budget::input_limit(
            self.config.context_window.unwrap_or(32768) as usize,
            self.output_reserve(),
        )?;
        Ok(Some((
            limit.saturating_sub(crate::ai::budget::input_tokens(
                &self.config.model,
                &payload,
            )),
            self.config.model.clone(),
        )))
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
            .map_err(|error| error.context(ObservedRequestFailure))
    }
}

impl OllamaProvider {
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
                num_ctx: self.config.context_window.unwrap_or(32768) as usize,
                num_predict: self.output_reserve(),
            },
            tools: None,
        };
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(ollama_tool_defs(tools));
            }
        }

        Ok((serde_json::to_value(&request)?, policies))
    }

    fn generate_impl(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        mut observer: Option<&mut dyn FnMut(ProviderEvent<'_>) -> Result<()>>,
    ) -> Result<AiReply> {
        let (mut payload, mut policies) = self.request_parts(messages, tools)?;
        let window = self.config.context_window.unwrap_or(32768) as usize;
        let output = self.output_reserve();
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));
        if let Some(observer) = observer.as_deref_mut() {
            observer(ProviderEvent::Poll)?;
            let (result, report) =
                policies.fit_reported(&self.config.model, &mut payload, window, output);
            observer(ProviderEvent::Budget {
                attempt: 1,
                trigger: "initial",
                report: &report,
            })?;
            result?;
        } else {
            policies.fit(&self.config.model, &mut payload, window, output)?;
        }
        let ollama_response: OllamaResponse = if let Some(observer) = observer {
            serde_json::from_value(streaming::cancellable_json(&url, &payload, observer)?)?
        } else {
            let response = self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&payload)
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
            response.json().context("Failed to parse Ollama response")?
        };

        let usage = match (
            ollama_response.prompt_eval_count,
            ollama_response.eval_count,
        ) {
            (Some(prompt_tokens), Some(completion_tokens)) => Some(TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens.saturating_add(completion_tokens),
                prompt_cached_tokens: None,
            }),
            _ => None,
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
